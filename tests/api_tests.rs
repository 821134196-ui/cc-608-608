//! 端到端业务规则测试。
//! 重点覆盖用户强调的三条铁律：
//!   1) 未经客户确认的加价不会生效（不计费、不能施工）；
//!   2) 零件没有完整复装不能通过验收；
//!   3) 同一验收/收款重复提交只办理一次、不重复收费。

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use hoodcare::meter::MockInstrument;
use hoodcare::{build_test_app, run_migrations, AppState};
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use tower::ServiceExt;

struct Harness {
    app: axum::Router,
    state: AppState,
    _dir: TempDir,
}

impl Harness {
    async fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let uploads = dir.path().join("uploads");
        tokio::fs::create_dir_all(&uploads).await.unwrap();

        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();

        let state = AppState {
            pool,
            uploads_dir: uploads,
            meter: MockInstrument::default(),
        };
        let app = build_test_app(state.clone());
        Harness {
            app,
            state,
            _dir: dir,
        }
    }

    async fn send(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let req = Request::builder().method(method).uri(uri);
        let (req, ct) = match body {
            Some(v) => {
                let bytes = serde_json::to_vec(&v).unwrap();
                let r = req
                    .header("content-type", "application/json")
                    .body(Body::from(bytes))
                    .unwrap();
                (r, true)
            }
            None => (req.body(Body::empty()).unwrap(), false),
        };
        let _ = ct;
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 4_000_000).await.unwrap();
        let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, v)
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.send("GET", uri, None).await
    }
    async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.send("POST", uri, Some(body)).await
    }

    async fn scalar_i64(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
    }
}

async fn create_order(h: &Harness, model: &str) -> String {
    let (s, v) = h
        .post(
            "/api/orders",
            json!({
                "model": model,
                "customer_name": "王女士",
                "customer_phone": "13800000000",
                "appointment_at": "2026-10-10T10:00:00+08:00",
                "base_fee_cents": 19800,
                "scope_note": "标准清洗",
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "create order: {v}");
    v["order"]["id"].as_str().unwrap().to_string()
}

/// 直接在 DB 插前后照片记录（照片上传另有 multipart 测试）
async fn insert_photos(h: &Harness, id: &str) {
    for phase in ["before", "after"] {
        sqlx::query(
            "INSERT INTO photos (id, order_id, phase, label, rel_path, content_type, created_at)
             VALUES (?1,?2,?3,?4,?5,'image/jpeg','2026-10-10T11:00:00+08:00')",
        )
        .bind(format!("photo-{phase}-{id}"))
        .bind(id)
        .bind(phase)
        .bind(phase)
        .bind(format!("{id}/{phase}.jpg"))
        .execute(&h.state.pool)
        .await
        .unwrap();
    }
}

/// 走完整施工流程到“待客户验收”：到场一致 → 2 零件拆装 → 前后风量 → 前后照片 → 完工
async fn proceed_to_awaiting(h: &Harness, id: &str) {
    let (s, v) = h
        .post(
            &format!("/api/orders/{id}/arrive"),
            json!({ "actual_model": "老板 CXW-260-67X2H" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "arrive: {v}");
    assert_eq!(v["status"], "in_progress");

    for name in ["风轮", "油网"] {
        let (s, v) = h
            .post(&format!("/api/orders/{id}/parts"), json!({ "name": name }))
            .await;
        assert_eq!(s, StatusCode::OK, "add part {name}: {v}");
    }
    let (_, parts) = h.get(&format!("/api/orders/{id}/parts")).await;
    for p in parts["parts"].as_array().unwrap() {
        let pid = p["id"].as_str().unwrap();
        let (s, v) = h
            .post(&format!("/api/orders/{id}/parts/{pid}/reinstall"), json!({}))
            .await;
        assert_eq!(s, StatusCode::OK, "reinstall: {v}");
    }

    // 风量：不传读数，由服务端向模拟测量仪取数
    for phase in ["before", "after"] {
        let (s, v) = h
            .post(
                &format!("/api/orders/{id}/measurements"),
                json!({ "phase": phase }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "measure {phase}: {v}");
    }

    insert_photos(h, id).await;

    let (s, v) = h
        .post(&format!("/api/orders/{id}/complete-work"), json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "complete work: {v}");
    assert_eq!(v["status"], "awaiting_acceptance");
}

// ---------- 规则一：未确认的加价不会生效 ----------

#[tokio::test]
async fn pending_addon_is_never_charged_or_applied() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;

    // 到场开工
    let (s, _) = h
        .post(
            &format!("/api/orders/{id}/arrive"),
            json!({ "actual_model": "老板 CXW-260-67X2H" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);

    // 师傅提出 80 元风轮深度拆洗，客户尚未确认
    let (s, v) = h
        .post(
            &format!("/api/orders/{id}/addons"),
            json!({ "code": "DEEP_WHEEL", "title": "风轮深度拆洗", "price_cents": 8000 }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "propose: {v}");
    let addon_id = v["addon_id"].as_str().unwrap().to_string();

    // 未确认就尝试施工 → 必须被拒绝
    let (s, v) = h
        .post(&format!("/api/orders/{id}/addons/{addon_id}/apply"), json!({}))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "pending addon must not be applied: {v}");

    // 有待确认方案时不能完工
    let (s, _) = h
        .post(&format!("/api/orders/{id}/parts"), json!({ "name": "风轮" }))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, pv) = h.get(&format!("/api/orders/{id}/parts")).await;
    let pid = pv["parts"][0]["id"].as_str().unwrap();
    h.post(&format!("/api/orders/{id}/parts/{pid}/reinstall"), json!({})).await;
    for phase in ["before", "after"] {
        h.post(&format!("/api/orders/{id}/measurements"), json!({ "phase": phase })).await;
    }
    insert_photos(&h, &id).await;
    let (s, v) = h
        .post(&format!("/api/orders/{id}/complete-work"), json!({}))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "pending addon blocks completion: {v}");

    // 客户拒绝 → 仍不能施工
    let (s, _) = h
        .post(
            &format!("/api/orders/{id}/addons/{addon_id}/decision"),
            json!({ "decision": "rejected" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = h
        .post(&format!("/api/orders/{id}/addons/{addon_id}/apply"), json!({}))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 重复提交决定无效（决定不可更改）
    let (s, _) = h
        .post(
            &format!("/api/orders/{id}/addons/{addon_id}/decision"),
            json!({ "decision": "approved" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 走完施工 → 账单只有基础费，拒绝的项目不计入
    let (s, _) = h.post(&format!("/api/orders/{id}/complete-work"), json!({})).await;
    assert_eq!(s, StatusCode::OK);
    let (_, detail) = h.get(&format!("/api/orders/{id}")).await;
    assert_eq!(detail["bill"]["total_cents"], 19800, "rejected addon must not be billed");
    assert_eq!(detail["bill"]["addon_fee_cents"], 0);

    // 验收 → 收款金额仍然只有基础费
    let (s, _) = h
        .post(&format!("/api/orders/{id}/acceptance"), json!({ "accepted": true }))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = h.post(&format!("/api/orders/{id}/payment"), json!({})).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["amount_cents"], 19800);
}

#[tokio::test]
async fn approved_but_not_applied_addon_is_not_billed() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;
    let (s, _) = h
        .post(&format!("/api/orders/{id}/arrive"), json!({ "actual_model": "老板 CXW-260-67X2H" }))
        .await;
    assert_eq!(s, StatusCode::OK);

    let (_, v) = h
        .post(
            &format!("/api/orders/{id}/addons"),
            json!({ "code": "OIL_PAN", "title": "油杯更换", "price_cents": 4500 }),
        )
        .await;
    let addon_id = v["addon_id"].as_str().unwrap();

    // 客户同意但师傅还没施工 → 不计费
    let (s, _) = h
        .post(
            &format!("/api/orders/{id}/addons/{addon_id}/decision"),
            json!({ "decision": "approved" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, detail) = h.get(&format!("/api/orders/{id}")).await;
    assert_eq!(detail["bill"]["total_cents"], 19800, "approved but unapplied addon must not be billed");

    // 施工后才计入
    let (s, _) = h
        .post(&format!("/api/orders/{id}/addons/{addon_id}/apply"), json!({}))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, detail) = h.get(&format!("/api/orders/{id}")).await;
    assert_eq!(detail["bill"]["total_cents"], 19800 + 4500);

    // 重复登记施工无效
    let (s, _) = h
        .post(&format!("/api/orders/{id}/addons/{addon_id}/apply"), json!({}))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[tokio::test]
async fn addon_cannot_be_proposed_after_completion_submitted() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;
    proceed_to_awaiting(&h, &id).await;

    // 已提交完工（待验收）后不能再新增收费项目
    let (s, v) = h
        .post(
            &format!("/api/orders/{id}/addons"),
            json!({ "code": "DEEP_WHEEL", "title": "风轮深度拆洗", "price_cents": 8000 }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "late addon must be rejected: {v}");
    let (_, detail) = h.get(&format!("/api/orders/{id}")).await;
    assert_eq!(detail["bill"]["total_cents"], 19800);
}

#[tokio::test]
async fn duplicate_pending_proposal_is_rejected() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;
    h.post(&format!("/api/orders/{id}/arrive"), json!({ "actual_model": "老板 CXW-260-67X2H" })).await;
    let body = json!({ "code": "SEAL_KIT", "title": "密封件更换", "price_cents": 3000 });
    let (s, _) = h.post(&format!("/api/orders/{id}/addons"), body.clone()).await;
    assert_eq!(s, StatusCode::OK);
    // 同类型待确认方案不能重复提交，防止用第二份报价顶替
    let (s, v) = h.post(&format!("/api/orders/{id}/addons"), body).await;
    assert_eq!(s, StatusCode::CONFLICT, "dup proposal: {v}");
    let n = h
        .scalar_i64(&format!(
            "SELECT COUNT(*) FROM addons WHERE order_id='{id}' AND code='SEAL_KIT'"
        ))
        .await;
    assert_eq!(n, 1);
}

// ---------- 型号不符流程 ----------

#[tokio::test]
async fn model_mismatch_requires_confirmation_and_rejection_keeps_original_scope() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;

    // 现场型号不一致 → 不自动开工，产生待确认方案
    let (s, v) = h
        .post(&format!("/api/orders/{id}/arrive"), json!({ "actual_model": "方太 EMC2" }))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["started"], false);
    let addon_id = v["addon_id"].as_str().unwrap().to_string();

    let (_, detail) = h.get(&format!("/api/orders/{id}")).await;
    assert_eq!(detail["status"], "pending");

    // 未确认不能开工
    let (s, _) = h.post(&format!("/api/orders/{id}/start-work"), json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 客户拒绝 → 按原型号原范围开工
    let (s, _) = h
        .post(
            &format!("/api/orders/{id}/addons/{addon_id}/decision"),
            json!({ "decision": "rejected" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = h.post(&format!("/api/orders/{id}/start-work"), json!({})).await;
    assert_eq!(s, StatusCode::OK, "start after rejection: {v}");
    assert_eq!(v["actual_model"], "老板 CXW-260-67X2H");
    let (_, detail) = h.get(&format!("/api/orders/{id}")).await;
    assert_eq!(detail["status"], "in_progress");
    assert_eq!(detail["actual_model"], "老板 CXW-260-67X2H");
}

#[tokio::test]
async fn model_mismatch_approved_uses_actual_model() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;
    let (_, v) = h
        .post(&format!("/api/orders/{id}/arrive"), json!({ "actual_model": "方太 EMC2" }))
        .await;
    let addon_id = v["addon_id"].as_str().unwrap();
    let (s, _) = h
        .post(
            &format!("/api/orders/{id}/addons/{addon_id}/decision"),
            json!({ "decision": "approved" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = h.post(&format!("/api/orders/{id}/start-work"), json!({})).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["actual_model"], "方太 EMC2");
}

// ---------- 规则二：缺件阻止验收 ----------

#[tokio::test]
async fn missing_part_blocks_completion_and_acceptance() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;

    let (s, _) = h
        .post(&format!("/api/orders/{id}/arrive"), json!({ "actual_model": "老板 CXW-260-67X2H" }))
        .await;
    assert_eq!(s, StatusCode::OK);

    // 登记 2 个零件，只复装 1 个
    for name in ["风轮", "油网"] {
        h.post(&format!("/api/orders/{id}/parts"), json!({ "name": name })).await;
    }
    let (_, pv) = h.get(&format!("/api/orders/{id}/parts")).await;
    let first = pv["parts"][0]["id"].as_str().unwrap();
    let (s, _) = h
        .post(&format!("/api/orders/{id}/parts/{first}/reinstall"), json!({}))
        .await;
    assert_eq!(s, StatusCode::OK);

    // 完工被挡
    let (s, v) = h.post(&format!("/api/orders/{id}/complete-work"), json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT, "missing part blocks completion: {v}");
    assert!(v["error"].as_str().unwrap().contains("未复装"));
    let (_, detail) = h.get(&format!("/api/orders/{id}")).await;
    assert_eq!(detail["status"], "in_progress");

    // 客户在未完工状态也不能验收
    let (s, _) = h
        .post(&format!("/api/orders/{id}/acceptance"), json!({ "accepted": true }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 补装第二个零件，补齐照片/风量后可以完工
    let second = pv["parts"][1]["id"].as_str().unwrap();
    h.post(&format!("/api/orders/{id}/parts/{second}/reinstall"), json!({})).await;
    for phase in ["before", "after"] {
        h.post(&format!("/api/orders/{id}/measurements"), json!({ "phase": phase })).await;
    }
    insert_photos(&h, &id).await;
    let (s, _) = h.post(&format!("/api/orders/{id}/complete-work"), json!({})).await;
    assert_eq!(s, StatusCode::OK);

    // 关键场景：已提交完工后零件又变成缺件（模拟数据被改/漏装），
    // 验收闸门在服务端复核，必须一票否决。
    sqlx::query(&format!(
        "UPDATE parts SET reinstalled_at=NULL WHERE order_id='{id}' AND name='油网'"
    ))
    .execute(&h.state.pool)
    .await
    .unwrap();

    let (s, v) = h
        .post(&format!("/api/orders/{id}/acceptance"), json!({ "accepted": true }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "missing part blocks acceptance: {v}");
    assert!(v["error"].as_str().unwrap().contains("油网"));

    // 没有产生验收记录，订单仍是待验收
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM acceptance_events WHERE order_id='{id}' AND accepted=1"
    ))
    .fetch_one(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(n, 0);
    let (_, detail) = h.get(&format!("/api/orders/{id}")).await;
    assert_eq!(detail["status"], "awaiting_acceptance");
    assert!(detail["missing_parts"].as_array().unwrap().iter().any(|p| p == "油网"));

    // 未验收不能收款
    let (s, _) = h.post(&format!("/api/orders/{id}/payment"), json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 复装补齐后验收通过
    sqlx::query(&format!(
        "UPDATE parts SET reinstalled_at='2026-10-10T12:00:00+08:00'
         WHERE order_id='{id}' AND name='油网'"
    ))
    .execute(&h.state.pool)
    .await
    .unwrap();
    let (s, _) = h
        .post(&format!("/api/orders/{id}/acceptance"), json!({ "accepted": true }))
        .await;
    assert_eq!(s, StatusCode::OK);
}

// ---------- 规则三：重复提交只办理一次 ----------

#[tokio::test]
async fn duplicate_acceptance_and_payment_are_idempotent_guarded() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;
    proceed_to_awaiting(&h, &id).await;

    // 第一次验收成功
    let (s, v) = h
        .post(&format!("/api/orders/{id}/acceptance"), json!({ "accepted": true }))
        .await;
    assert_eq!(s, StatusCode::OK, "first acceptance: {v}");

    // 第二次验收必须失败
    let (s, v) = h
        .post(&format!("/api/orders/{id}/acceptance"), json!({ "accepted": true }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "duplicate acceptance: {v}");
    let n = h
        .scalar_i64(&format!(
            "SELECT COUNT(*) FROM acceptance_events WHERE order_id='{id}' AND accepted=1"
        ))
        .await;
    assert_eq!(n, 1, "acceptance must be recorded exactly once");

    // 第一次收款成功
    let (s, v) = h.post(&format!("/api/orders/{id}/payment"), json!({})).await;
    assert_eq!(s, StatusCode::OK, "first payment: {v}");
    assert_eq!(v["amount_cents"], 19800);

    // 第二次收款必须失败，且不产生第二笔扣款
    let (s, v) = h.post(&format!("/api/orders/{id}/payment"), json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT, "duplicate payment: {v}");

    let payments = h.scalar_i64(&format!("SELECT COUNT(*) FROM payments WHERE order_id='{id}'")).await;
    let charges = h.scalar_i64(&format!("SELECT COUNT(*) FROM charge_events WHERE order_id='{id}'")).await;
    let charged: i64 = sqlx::query_scalar(&format!(
        "SELECT COALESCE(SUM(amount_cents),0) FROM charge_events WHERE order_id='{id}'"
    ))
    .fetch_one(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(payments, 1, "payment only once");
    assert_eq!(charges, 1, "charge only once");
    assert_eq!(charged, 19800, "no double charge");
}

#[tokio::test]
async fn concurrent_payment_requests_charge_exactly_once() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;
    proceed_to_awaiting(&h, &id).await;
    h.post(&format!("/api/orders/{id}/acceptance"), json!({ "accepted": true })).await;

    // 5 个收款请求同时到达
    let mut futs = Vec::new();
    for _ in 0..5 {
        let app = h.app.clone();
        let oid = id.clone();
        futs.push(tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri(format!("/api/orders/{oid}/payment"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap();
            let resp = tokio::time::timeout(Duration::from_secs(10), app.oneshot(req))
                .await
                .expect("timed out")
                .unwrap();
            resp.status()
        }));
    }
    let mut ok = 0;
    let mut conflict = 0;
    for f in futs {
        match f.await.unwrap() {
            StatusCode::OK => ok += 1,
            StatusCode::CONFLICT => conflict += 1,
            other => panic!("unexpected status {other}"),
        }
    }
    assert_eq!(ok, 1, "exactly one payment succeeds");
    assert_eq!(conflict, 4, "the other 4 must be rejected as duplicates");

    let charges = h.scalar_i64(&format!("SELECT COUNT(*) FROM charge_events WHERE order_id='{id}'")).await;
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COALESCE(SUM(amount_cents),0) FROM charge_events WHERE order_id='{id}'"
    ))
    .fetch_one(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(charges, 1);
    assert_eq!(total, 19800);
}

// ---------- 其他幂等与模拟仪表 ----------

#[tokio::test]
async fn reinstall_is_idempotent() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;
    h.post(&format!("/api/orders/{id}/arrive"), json!({ "actual_model": "老板 CXW-260-67X2H" })).await;
    let (_, v) = h.post(&format!("/api/orders/{id}/parts"), json!({ "name": "油杯" })).await;
    assert_eq!(v["reinstalled"], false);
    let pid = v["part_id"].as_str().unwrap();

    for _ in 0..3 {
        let (s, v) = h
            .post(&format!("/api/orders/{id}/parts/{pid}/reinstall"), json!({}))
            .await;
        assert_eq!(s, StatusCode::OK, "reinstall repeat: {v}");
    }
    let n = h
        .scalar_i64(&format!(
            "SELECT COUNT(*) FROM parts WHERE order_id='{id}' AND name='油杯'"
        ))
        .await;
    assert_eq!(n, 1, "重复复装不能产生重复零件记录");
    let n_missing = h
        .scalar_i64(&format!(
            "SELECT COUNT(*) FROM parts WHERE order_id='{id}' AND reinstalled_at IS NULL"
        ))
        .await;
    assert_eq!(n_missing, 0);

    // 同名零件不能重复登记
    let (s, _) = h.post(&format!("/api/orders/{id}/parts"), json!({ "name": "油杯" })).await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[tokio::test]
async fn meter_readings_are_stable_and_after_beats_before() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;

    let (_, v1) = h.get(&format!("/api/orders/{id}/meter/before")).await;
    let (_, v2) = h.get(&format!("/api/orders/{id}/meter/before")).await;
    let b1 = v1["reading"]["airflow_m3h"].as_f64().unwrap();
    let b2 = v2["reading"]["airflow_m3h"].as_f64().unwrap();
    assert!((b1 - b2).abs() < f64::EPSILON, "同一测点重复读数应稳定");
    assert!((620.0..800.0).contains(&b1));

    let (_, va) = h.get(&format!("/api/orders/{id}/meter/after")).await;
    let a = va["reading"]["airflow_m3h"].as_f64().unwrap();
    assert!((980.0..1180.0).contains(&a));
    assert!(a > b1, "模拟仪表洗后风量应高于洗前");
}

#[tokio::test]
async fn measurement_upsert_keeps_one_row_per_phase() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;
    h.post(&format!("/api/orders/{id}/arrive"), json!({ "actual_model": "老板 CXW-260-67X2H" })).await;

    // 重复提交同一阶段读数：upsert，不产生多行
    for v in [700.0, 705.5, 710.0] {
        let (s, _) = h
            .post(
                &format!("/api/orders/{id}/measurements"),
                json!({ "phase": "before", "airflow_m3h": v }),
            )
            .await;
        assert_eq!(s, StatusCode::OK);
    }
    let n = h
        .scalar_i64(&format!(
            "SELECT COUNT(*) FROM measurements WHERE order_id='{id}' AND phase='before'"
        ))
        .await;
    assert_eq!(n, 1);
    let val: f64 = sqlx::query_scalar(&format!(
        "SELECT airflow_m3h FROM measurements WHERE order_id='{id}' AND phase='before'"
    ))
    .fetch_one(&h.state.pool)
    .await
    .unwrap();
    assert!((val - 710.0).abs() < 1e-9, "最新读数覆盖旧读数");
}

#[tokio::test]
async fn photo_multipart_upload_persists_file_and_row() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;

    // 真实 multipart 上传一张最小 JPEG（字节内容不做图像校验，仅验证落盘与记录）
    let boundary = "----hoodcaretestboundary";
    let file_bytes = vec![0xFFu8, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"phase\"\r\n\r\n");
    body.extend_from_slice(b"before\r\n");
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"hood.jpg\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: image/jpeg\r\n\r\n");
    body.extend_from_slice(&file_bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/orders/{id}/photos"))
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let resp = h.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 4_000_000).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let url = v["url"].as_str().unwrap();
    assert!(url.starts_with("/uploads/"));

    // 文件确实落在本地 uploads 目录
    let rel = url.trim_start_matches("/uploads/");
    let abs = h.state.uploads_dir.join(rel);
    let meta = tokio::fs::metadata(&abs).await.unwrap();
    assert_eq!(meta.len(), file_bytes.len() as u64);

    let n = h
        .scalar_i64(&format!("SELECT COUNT(*) FROM photos WHERE order_id='{id}'"))
        .await;
    assert_eq!(n, 1);
}

#[tokio::test]
async fn payment_requires_acceptance_and_acceptance_requires_work() {
    let h = Harness::new().await;
    let id = create_order(&h, "老板 CXW-260-67X2H").await;

    // 未验收不能收款
    let (s, _) = h.post(&format!("/api/orders/{id}/payment"), json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 未完工不能验收
    let (s, _) = h
        .post(&format!("/api/orders/{id}/acceptance"), json!({ "accepted": true }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
}
