-- 油烟机清洗验收系统数据库结构
-- 订单状态机: booked -> confirmed -> in_progress -> awaiting_acceptance -> accepted -> closed

CREATE TABLE IF NOT EXISTS orders (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    customer_name     TEXT NOT NULL,
    phone             TEXT NOT NULL,
    address           TEXT NOT NULL,
    hood_model        TEXT NOT NULL,               -- 客户预约时填写的型号
    confirmed_model   TEXT,                        -- 师傅到场确认的型号（可能与预约不符）
    appointment_time  TEXT NOT NULL,
    status            TEXT NOT NULL DEFAULT 'booked',
    base_price_cents  INTEGER NOT NULL DEFAULT 0,  -- 基础清洗价格（分）
    scope_note        TEXT,                        -- 现场范围说明 / 型号不符记录
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS photos (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id   INTEGER NOT NULL REFERENCES orders(id),
    kind       TEXT NOT NULL CHECK (kind IN ('booking', 'before', 'after')),
    file_path  TEXT NOT NULL,                      -- 相对 uploads 目录的路径
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_photos_order ON photos(order_id);

CREATE TABLE IF NOT EXISTS parts (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id       INTEGER NOT NULL REFERENCES orders(id),
    name           TEXT NOT NULL,
    quantity       INTEGER NOT NULL DEFAULT 1,
    reinstalled    INTEGER NOT NULL DEFAULT 0,     -- 0/1
    removed_at     TEXT NOT NULL DEFAULT (datetime('now')),
    reinstalled_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_parts_order ON parts(order_id);

-- 现场加价项目：必须客户确认(approved)才计入账单；rejected 后按原范围执行
CREATE TABLE IF NOT EXISTS extra_items (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id    INTEGER NOT NULL REFERENCES orders(id),
    description TEXT NOT NULL,
    price_cents INTEGER NOT NULL CHECK (price_cents > 0),
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    decided_at  TEXT
);
CREATE INDEX IF NOT EXISTS idx_extras_order ON extra_items(order_id);

CREATE TABLE IF NOT EXISTS measurements (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id       INTEGER NOT NULL REFERENCES orders(id),
    phase          TEXT NOT NULL CHECK (phase IN ('before', 'after')),
    airflow_m3_min REAL NOT NULL,
    noise_db       REAL NOT NULL,
    measured_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_measurements_order ON measurements(order_id);

-- 验收记录：每单最多一条、幂等键全局唯一 —— 重复提交不会产生第二条
CREATE TABLE IF NOT EXISTS acceptances (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id        INTEGER NOT NULL UNIQUE REFERENCES orders(id),
    idempotency_key TEXT NOT NULL UNIQUE,
    total_cents     INTEGER NOT NULL,              -- 验收时冻结的应付总额
    confirmed_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 收款记录：每单最多一条、幂等键全局唯一 —— 重复提交不会重复收费
CREATE TABLE IF NOT EXISTS payments (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id        INTEGER NOT NULL UNIQUE REFERENCES orders(id),
    idempotency_key TEXT NOT NULL UNIQUE,
    amount_cents    INTEGER NOT NULL,
    paid_at         TEXT NOT NULL DEFAULT (datetime('now'))
);
