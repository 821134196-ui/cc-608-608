//! 端到端 API 测试，重点验证：
//! 1. 未经客户确认的加价不计入账单、验收后加价冻结
//! 2. 零件未完整复装时无法通过验收
//! 3. 验收 / 收款重复提交（同幂等键或不同幂等键）都只生效一次

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::path::PathBuf;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// 测试辅助
// ---------------------------------------------------------------------------

async fn setup() -> (Router, SqlitePool) {
    let unique = uuid::Uuid::new_v4();
    let db_path = std::env::temp_dir().join(format!("hood-test-{unique}.db"));
    let url = format!("sqlite://{}", db_path.display());
    let pool = hood_acceptance::init_pool(&url).await.expect("init db");
    let uploads = std::env::temp_dir().join(format!("hood-uploads-{unique}"));
    std::fs::create_dir_all(&uploads).unwrap();
    let app = hood_acceptance::build_app(pool.clone(), uploads, PathBuf::from("frontend/dist"));
    (app, pool)
}

async fn req(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 16 * 1024 * 1024).await.unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
    req(app, Method::GET, uri, None).await
}
async fn post(app: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    req(app, Method::POST, uri, Some(body)).await
}
async fn post_empty(app: &Router, uri: &str) -> (StatusCode, Value) {
    req(app, Method::POST, uri, None).await
}

/// 构造 multipart 图片上传请求
async fn upload_photo(app: &Router, order_id: i64, kind: &str) -> StatusCode {
    let boundary = "TESTBOUNDARY";
    let mut data: Vec<u8> = Vec::new();
    data.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"p.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    data.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]); // PNG 头
    data.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/orders/{order_id}/photos?kind={kind}"))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(data))
                .unwrap(),
        )
        .await
        .unwrap();
    resp.status()
}

/// 创建订单并推进到「清洗中」，返回订单 id
async fn make_in_progress_order(app: &Router) -> i64 {
    let (s, v) = post(
        app,
        "/api/orders",
        json!({
            "customer_name": "张三",
            "phone": "13800000000",
            "address": "幸福小区 1-101",
            "hood_model": "方太 CXW-200",
            "appointment_time": "2026-10-06 10:00",
            "base_price_cents": 20000
        }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "创建订单失败: {v}");
    let id = v["id"].as_i64().unwrap();

    let (s, v) = post(
        app,
        &format!("/api/orders/{id}/confirm-scope"),
        json!({"confirmed_model": "方太 CXW-200", "base_price_cents": 20000, "scope_note": "型号一致"}),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "确认范围失败: {v}");

    let (s, v) = post_empty(app, &format!("/api/orders/{id}/start")).await;
    assert_eq!(s, StatusCode::OK, "开工失败: {v}");
    id
}

/// 补全完工所需材料（照片、测量、零件登记+复装），但不提交完工
async fn fill_completion_materials(app: &Router, id: i64) {
    assert_eq!(upload_photo(app, id, "before").await, StatusCode::CREATED);
    assert_eq!(upload_photo(app, id, "after").await, StatusCode::CREATED);
    let (s, v) = post(app, &format!("/api/orders/{id}/measurements/capture"), json!({"phase": "before"})).await;
    assert_eq!(s, StatusCode::CREATED, "测量 before 失败: {v}");
    let (s, v) = post(app, &format!("/api/orders/{id}/measurements/capture"), json!({"phase": "after"})).await;
    assert_eq!(s, StatusCode::CREATED, "测量 after 失败: {v}");
    let (s, v) = post(app, &format!("/api/orders/{id}/parts"), json!({"name": "油网", "quantity": 1})).await;
    assert_eq!(s, StatusCode::CREATED, "登记零件失败: {v}");
    let part_id = v["parts"][0]["id"].as_i64().unwrap();
    let (s, v) = post_empty(app, &format!("/api/orders/{id}/parts/{part_id}/reinstall")).await;
    assert_eq!(s, StatusCode::OK, "复装失败: {v}");
}

/// 推进到「待验收」
async fn make_awaiting_order(app: &Router) -> i64 {
    let id = make_in_progress_order(app).await;
    fill_completion_materials(app, id).await;
    let (s, v) = post_empty(app, &format!("/api/orders/{id}/complete")).await;
    assert_eq!(s, StatusCode::OK, "完工失败: {v}");
    id
}

/// 推进到「已验收」，返回 (订单id, 验收key)
async fn make_accepted_order(app: &Router) -> (i64, String) {
    let id = make_awaiting_order(app).await;
    let key = format!("acc-{id}-{}", uuid::Uuid::new_v4());
    let (s, v) = post(app, &format!("/api/orders/{id}/accept"), json!({"idempotency_key": key})).await;
    assert_eq!(s, StatusCode::OK, "验收失败: {v}");
    (id, key)
}

// ---------------------------------------------------------------------------
// 主流程
// ---------------------------------------------------------------------------

#[tokio::test]
async fn happy_path_full_flow() {
    let (app, _pool) = setup().await;
    let id = make_awaiting_order(&app).await;

    let (s, detail) = get(&app, &format!("/api/orders/{id}")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(detail["status"], "awaiting_acceptance");
    assert_eq!(detail["acceptance_blockers"].as_array().unwrap().len(), 0);
    assert_eq!(detail["bill"]["total_cents"], 20000);

    let (s, v) = post(&app, &format!("/api/orders/{id}/accept"), json!({"idempotency_key": "k-acc-1"})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["status"], "accepted");
    assert_eq!(v["acceptance"]["total_cents"], 20000);

    let (s, v) = post(&app, &format!("/api/orders/{id}/pay"), json!({"idempotency_key": "k-pay-1"})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["status"], "closed");
    assert_eq!(v["payment"]["amount_cents"], 20000);
}

// ---------------------------------------------------------------------------
// 不变量 1：未经确认的加价不生效
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unconfirmed_extras_never_billed() {
    let (app, _pool) = setup().await;
    let id = make_awaiting_order(&app).await;

    // 三个加价项目：一个同意、一个拒绝、一个始终不处理（pending）
    let (s, v) = post(&app, &format!("/api/orders/{id}/extras"), json!({"description": "更换止回阀", "price_cents": 8000})).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let approved_id = v["extras"][0]["id"].as_i64().unwrap();
    let (s, v) = post(&app, &format!("/api/orders/{id}/extras"), json!({"description": "加装防烟宝", "price_cents": 5000})).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let rejected_id = v["extras"][1]["id"].as_i64().unwrap();
    let (s, v) = post(&app, &format!("/api/orders/{id}/extras"), json!({"description": "深度除味", "price_cents": 3000})).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let pending_id = v["extras"][2]["id"].as_i64().unwrap();

    let (s, _) = post_empty(&app, &format!("/api/orders/{id}/extras/{approved_id}/approve")).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = post_empty(&app, &format!("/api/orders/{id}/extras/{rejected_id}/reject")).await;
    assert_eq!(s, StatusCode::OK);

    // 账单 = 基础价 20000 + 已同意 8000；拒绝的 5000 和未确认的 3000 不计入
    let (_, detail) = get(&app, &format!("/api/orders/{id}")).await;
    assert_eq!(detail["bill"]["base_cents"], 20000);
    assert_eq!(detail["bill"]["approved_extras_cents"], 8000);
    assert_eq!(detail["bill"]["total_cents"], 28000);

    // 验收并结单，金额冻结为 28000
    let (s, v) = post(&app, &format!("/api/orders/{id}/accept"), json!({"idempotency_key": "acc-x1"})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["acceptance"]["total_cents"], 28000);

    let (s, v) = post(&app, &format!("/api/orders/{id}/pay"), json!({"idempotency_key": "pay-x1"})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["payment"]["amount_cents"], 28000);

    // 验收后：pending 项目再同意也不生效（状态冻结）
    let (s, _) = post_empty(&app, &format!("/api/orders/{id}/extras/{pending_id}/approve")).await;
    assert_eq!(s, StatusCode::CONFLICT);
    // 验收后也不能再新增加价
    let (s, _) = post(&app, &format!("/api/orders/{id}/extras"), json!({"description": "事后加价", "price_cents": 9900})).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 最终金额仍是 28000
    let (_, detail) = get(&app, &format!("/api/orders/{id}")).await;
    assert_eq!(detail["bill"]["total_cents"], 28000);
    assert_eq!(detail["payment"]["amount_cents"], 28000);
}

#[tokio::test]
async fn rejected_extras_keep_original_scope() {
    let (app, _pool) = setup().await;
    let id = make_awaiting_order(&app).await;

    let (_, v) = post(&app, &format!("/api/orders/{id}/extras"), json!({"description": "型号不符需换配件", "price_cents": 12000})).await;
    let eid = v["extras"][0]["id"].as_i64().unwrap();
    let (s, _) = post_empty(&app, &format!("/api/orders/{id}/extras/{eid}/reject")).await;
    assert_eq!(s, StatusCode::OK);

    // 客户拒绝后按原范围执行：账单只有基础价
    let (_, detail) = get(&app, &format!("/api/orders/{id}")).await;
    assert_eq!(detail["bill"]["total_cents"], 20000);

    let (s, v) = post(&app, &format!("/api/orders/{id}/accept"), json!({"idempotency_key": "acc-r1"})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["acceptance"]["total_cents"], 20000);
}

// ---------------------------------------------------------------------------
// 不变量 2：缺件阻止验收
// ---------------------------------------------------------------------------

#[tokio::test]
async fn missing_parts_block_acceptance() {
    let (app, _pool) = setup().await;
    let id = make_in_progress_order(&app).await;

    assert_eq!(upload_photo(&app, id, "before").await, StatusCode::CREATED);
    assert_eq!(upload_photo(&app, id, "after").await, StatusCode::CREATED);
    post(&app, &format!("/api/orders/{id}/measurements/capture"), json!({"phase": "before"})).await;
    post(&app, &format!("/api/orders/{id}/measurements/capture"), json!({"phase": "after"})).await;

    // 登记两个零件，只复装一个
    let (_, v) = post(&app, &format!("/api/orders/{id}/parts"), json!({"name": "油网"})).await;
    let p1 = v["parts"][0]["id"].as_i64().unwrap();
    post(&app, &format!("/api/orders/{id}/parts"), json!({"name": "风轮"})).await;
    post_empty(&app, &format!("/api/orders/{id}/parts/{p1}/reinstall")).await;

    // 完工可以提交，但验收必须被缺件阻止
    let (s, v) = post_empty(&app, &format!("/api/orders/{id}/complete")).await;
    assert_eq!(s, StatusCode::OK, "{v}");

    let (s, v) = post(&app, &format!("/api/orders/{id}/accept"), json!({"idempotency_key": "acc-m1"})).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(v["error"].as_str().unwrap().contains("未复装"), "错误信息应提示缺件: {v}");

    // 状态仍是待验收
    let (_, detail) = get(&app, &format!("/api/orders/{id}")).await;
    assert_eq!(detail["status"], "awaiting_acceptance");
    assert!(detail["acceptance_blockers"].as_array().unwrap().iter().any(|b| b.as_str().unwrap().contains("未复装")));

    // 复装剩余零件后验收通过
    let p2 = detail["parts"][1]["id"].as_i64().unwrap();
    let (s, _) = post_empty(&app, &format!("/api/orders/{id}/parts/{p2}/reinstall")).await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = post(&app, &format!("/api/orders/{id}/accept"), json!({"idempotency_key": "acc-m1"})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["status"], "accepted");
}

#[tokio::test]
async fn completion_requires_parts_and_materials() {
    let (app, _pool) = setup().await;
    let id = make_in_progress_order(&app).await;

    // 什么都没准备就完工 → 拒绝
    let (s, v) = post_empty(&app, &format!("/api/orders/{id}/complete")).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(v["error"].as_str().unwrap().contains("无法完工"));

    // 只登记零件但缺照片/测量 → 仍拒绝
    post(&app, &format!("/api/orders/{id}/parts"), json!({"name": "油网"})).await;
    let (s, _) = post_empty(&app, &format!("/api/orders/{id}/complete")).await;
    assert_eq!(s, StatusCode::CONFLICT);
}

// ---------------------------------------------------------------------------
// 不变量 3：重复提交只生效一次
// ---------------------------------------------------------------------------

#[tokio::test]
async fn duplicate_acceptance_is_idempotent() {
    let (app, pool) = setup().await;
    let id = make_awaiting_order(&app).await;

    // 同一幂等键重复提交 → 都返回成功，但只有一条验收记录
    let (s1, v1) = post(&app, &format!("/api/orders/{id}/accept"), json!({"idempotency_key": "dup-acc"})).await;
    let (s2, v2) = post(&app, &format!("/api/orders/{id}/accept"), json!({"idempotency_key": "dup-acc"})).await;
    assert_eq!(s1, StatusCode::OK, "{v1}");
    assert_eq!(s2, StatusCode::OK, "{v2}");
    assert_eq!(v1["acceptance"]["id"], v2["acceptance"]["id"]);

    // 不同幂等键重复验收 → 409
    let (s3, _) = post(&app, &format!("/api/orders/{id}/accept"), json!({"idempotency_key": "another-key"})).await;
    assert_eq!(s3, StatusCode::CONFLICT);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM acceptances WHERE order_id = ?")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "验收记录只能有一条");
}

#[tokio::test]
async fn duplicate_payment_never_double_charges() {
    let (app, pool) = setup().await;
    let (id, _acc_key) = make_accepted_order(&app).await;

    // 同一幂等键重复收款 → 同一笔收款记录
    let (s1, v1) = post(&app, &format!("/api/orders/{id}/pay"), json!({"idempotency_key": "dup-pay"})).await;
    let (s2, v2) = post(&app, &format!("/api/orders/{id}/pay"), json!({"idempotency_key": "dup-pay"})).await;
    assert_eq!(s1, StatusCode::OK, "{v1}");
    assert_eq!(s2, StatusCode::OK, "{v2}");
    assert_eq!(v1["payment"]["id"], v2["payment"]["id"]);
    assert_eq!(v1["payment"]["amount_cents"], 20000);

    // 不同幂等键重复收款 → 409，不会生成第二笔
    let (s3, _) = post(&app, &format!("/api/orders/{id}/pay"), json!({"idempotency_key": "pay-again"})).await;
    assert_eq!(s3, StatusCode::CONFLICT);

    let (count, total): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(amount_cents), 0) FROM payments WHERE order_id = ?",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "收款记录只能有一条");
    assert_eq!(total, 20000, "重复提交不能重复收费");
}

#[tokio::test]
async fn cannot_pay_before_acceptance() {
    let (app, _pool) = setup().await;
    let id = make_awaiting_order(&app).await;
    let (s, v) = post(&app, &format!("/api/orders/{id}/pay"), json!({"idempotency_key": "early-pay"})).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(v["error"].as_str().unwrap().contains("验收"));
}

#[tokio::test]
async fn cannot_accept_before_completion() {
    let (app, _pool) = setup().await;
    let id = make_in_progress_order(&app).await;
    let (s, _) = post(&app, &format!("/api/orders/{id}/accept"), json!({"idempotency_key": "early-acc"})).await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[tokio::test]
async fn idempotency_key_cannot_be_reused_across_orders() {
    let (app, _pool) = setup().await;
    let id1 = make_awaiting_order(&app).await;
    let id2 = make_awaiting_order(&app).await;

    let (s, _) = post(&app, &format!("/api/orders/{id1}/accept"), json!({"idempotency_key": "shared-key"})).await;
    assert_eq!(s, StatusCode::OK);
    // 同一个 key 用到另一单上 → 拒绝
    let (s, _) = post(&app, &format!("/api/orders/{id2}/accept"), json!({"idempotency_key": "shared-key"})).await;
    assert_eq!(s, StatusCode::CONFLICT);
}
