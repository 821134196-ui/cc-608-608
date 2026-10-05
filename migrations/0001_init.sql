-- 家庭厨房油烟机清洗验收系统 · 初始 schema

CREATE TABLE orders (
    id              TEXT PRIMARY KEY,
    model           TEXT NOT NULL,                    -- 客户预约时填写的型号
    customer_name   TEXT NOT NULL,
    customer_phone  TEXT NOT NULL,
    appointment_at  TEXT NOT NULL,                    -- RFC3339 预约时间
    base_fee_cents  INTEGER NOT NULL DEFAULT 0,       -- 原始方案价格（分）
    actual_model    TEXT,                             -- 现场确认后的型号（加价方案经客户确认后才写入）
    status          TEXT NOT NULL DEFAULT 'pending',
                    -- pending → in_progress → awaiting_acceptance → accepted → completed
    scope_note      TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL,
    scope_confirmed_at TEXT,
    completed_at    TEXT,                             -- 结单（收款完成）时间
    CHECK (status IN ('pending','in_progress','awaiting_acceptance','accepted','completed'))
);

-- 加价/改方案项目：必须客户确认（approved）才能实施和计费
CREATE TABLE addons (
    id            TEXT PRIMARY KEY,
    order_id      TEXT NOT NULL REFERENCES orders(id),
    code          TEXT NOT NULL,       -- MODEL_MISMATCH / DEEP_WHEEL / EXTRA_PART ...
    title         TEXT NOT NULL,
    description   TEXT NOT NULL DEFAULT '',
    price_cents   INTEGER NOT NULL DEFAULT 0,
    extra_json    TEXT NOT NULL DEFAULT '{}',
    status        TEXT NOT NULL DEFAULT 'pending',   -- pending / approved / rejected
    applied       INTEGER NOT NULL DEFAULT 0,        -- 师傅是否已按确认方案施工
    created_at    TEXT NOT NULL,
    decided_at    TEXT,
    applied_at    TEXT
);
-- 同一订单同一类型只允许存在一个待确认方案
CREATE UNIQUE INDEX idx_addons_one_pending
    ON addons(order_id, code) WHERE status = 'pending';
CREATE INDEX idx_addons_order ON addons(order_id);

-- 零件：拆下登记一次，复装把 reinstalled_at 填上；缺件 = removed 但未 reinstalled
CREATE TABLE parts (
    id             TEXT PRIMARY KEY,
    order_id       TEXT NOT NULL REFERENCES orders(id),
    name           TEXT NOT NULL,
    removed_at     TEXT NOT NULL,
    reinstalled_at TEXT,
    note           TEXT NOT NULL DEFAULT ''
);
CREATE UNIQUE INDEX idx_parts_order_name ON parts(order_id, name);
CREATE INDEX idx_parts_order ON parts(order_id);

-- 现场照片：本地落盘，数据库只存相对路径
CREATE TABLE photos (
    id           TEXT PRIMARY KEY,
    order_id     TEXT NOT NULL REFERENCES orders(id),
    phase        TEXT NOT NULL CHECK (phase IN ('before','after')),
    label        TEXT NOT NULL DEFAULT '',
    rel_path     TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'image/jpeg',
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_photos_order_phase ON photos(order_id, phase);

-- 风量测量（模拟测量仪）：每个阶段只保留最新一条，重复提交走 upsert，不产生重复读数
CREATE TABLE measurements (
    id            TEXT PRIMARY KEY,
    order_id      TEXT NOT NULL REFERENCES orders(id),
    phase         TEXT NOT NULL CHECK (phase IN ('before','after')),
    airflow_m3h   REAL NOT NULL,
    source        TEXT NOT NULL DEFAULT 'mock-instrument',
    note          TEXT NOT NULL DEFAULT '',
    recorded_at   TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_measure_order_phase ON measurements(order_id, phase);

-- 验收记录：accepted=1 全订单唯一 → 验收只能办理一次
CREATE TABLE acceptance_events (
    id         TEXT PRIMARY KEY,
    order_id   TEXT NOT NULL REFERENCES orders(id),
    accepted   INTEGER NOT NULL,
    note       TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_acceptance_one_accepted
    ON acceptance_events(order_id) WHERE accepted = 1;

-- 收款记录：一订单最多一笔 → 收款只能办理一次
CREATE TABLE payments (
    id           TEXT PRIMARY KEY,
    order_id     TEXT NOT NULL UNIQUE REFERENCES orders(id),
    amount_cents INTEGER NOT NULL,
    detail_json  TEXT NOT NULL DEFAULT '[]',
    created_at   TEXT NOT NULL
);

-- 模拟支付网关流水：只有真正发起扣款时才写入；重复提交收款不得多写
CREATE TABLE charge_events (
    id         TEXT PRIMARY KEY,
    order_id   TEXT NOT NULL REFERENCES orders(id),
    amount_cents INTEGER NOT NULL,
    created_at TEXT NOT NULL
);
