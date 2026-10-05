use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::{meter, AppState};

// ---------------------------------------------------------------------------
// 数据模型
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct OrderRow {
    pub id: i64,
    pub customer_name: String,
    pub phone: String,
    pub address: String,
    pub hood_model: String,
    pub confirmed_model: Option<String>,
    pub appointment_time: String,
    pub status: String,
    pub base_price_cents: i64,
    pub scope_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct PhotoRow {
    pub id: i64,
    pub kind: String,
    pub file_path: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct PhotoJson {
    pub id: i64,
    pub kind: String,
    pub url: String,
    pub created_at: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct PartRow {
    pub id: i64,
    pub name: String,
    pub quantity: i64,
    pub reinstalled: bool,
    pub removed_at: String,
    pub reinstalled_at: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ExtraRow {
    pub id: i64,
    pub description: String,
    pub price_cents: i64,
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct MeasurementRow {
    pub id: i64,
    pub phase: String,
    pub airflow_m3_min: f64,
    pub noise_db: f64,
    pub measured_at: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AcceptanceRow {
    pub id: i64,
    pub idempotency_key: String,
    pub total_cents: i64,
    pub confirmed_at: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct PaymentRow {
    pub id: i64,
    pub idempotency_key: String,
    pub amount_cents: i64,
    pub paid_at: String,
}

#[derive(Serialize)]
pub struct BillJson {
    pub base_cents: i64,
    pub approved_extras_cents: i64,
    pub total_cents: i64,
    /// 验收后账单冻结，之后任何加价变更都不再影响应付金额
    pub frozen: bool,
}

#[derive(Serialize)]
pub struct OrderDetail {
    #[serde(flatten)]
    pub order: OrderRow,
    pub photos: Vec<PhotoJson>,
    pub parts: Vec<PartRow>,
    pub extras: Vec<ExtraRow>,
    pub measurements: Vec<MeasurementRow>,
    pub acceptance: Option<AcceptanceRow>,
    pub payment: Option<PaymentRow>,
    pub bill: BillJson,
    /// 当前阻碍验收的事项（空数组 = 可以验收）
    pub acceptance_blockers: Vec<String>,
}

// ---------------------------------------------------------------------------
// 查询辅助
// ---------------------------------------------------------------------------

async fn fetch_order(pool: &SqlitePool, id: i64) -> Result<OrderRow, AppError> {
    sqlx::query_as::<_, OrderRow>("SELECT * FROM orders WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::not_found(format!("订单 {id} 不存在")))
}

async fn fetch_photos(pool: &SqlitePool, id: i64) -> Result<Vec<PhotoRow>, AppError> {
    Ok(sqlx::query_as::<_, PhotoRow>(
        "SELECT id, kind, file_path, created_at FROM photos WHERE order_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?)
}

async fn fetch_parts(pool: &SqlitePool, id: i64) -> Result<Vec<PartRow>, AppError> {
    Ok(sqlx::query_as::<_, PartRow>(
        "SELECT id, name, quantity, reinstalled, removed_at, reinstalled_at FROM parts WHERE order_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?)
}

async fn fetch_extras(pool: &SqlitePool, id: i64) -> Result<Vec<ExtraRow>, AppError> {
    Ok(sqlx::query_as::<_, ExtraRow>(
        "SELECT id, description, price_cents, status, created_at, decided_at FROM extra_items WHERE order_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?)
}

async fn fetch_measurements(pool: &SqlitePool, id: i64) -> Result<Vec<MeasurementRow>, AppError> {
    Ok(sqlx::query_as::<_, MeasurementRow>(
        "SELECT id, phase, airflow_m3_min, noise_db, measured_at FROM measurements WHERE order_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?)
}

async fn fetch_acceptance(pool: &SqlitePool, id: i64) -> Result<Option<AcceptanceRow>, AppError> {
    Ok(sqlx::query_as::<_, AcceptanceRow>(
        "SELECT id, idempotency_key, total_cents, confirmed_at FROM acceptances WHERE order_id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

async fn fetch_payment(pool: &SqlitePool, id: i64) -> Result<Option<PaymentRow>, AppError> {
    Ok(sqlx::query_as::<_, PaymentRow>(
        "SELECT id, idempotency_key, amount_cents, paid_at FROM payments WHERE order_id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

/// 完工前置检查：照片、测量、零件登记
fn completion_blockers(photos: &[PhotoRow], parts: &[PartRow], measurements: &[MeasurementRow]) -> Vec<String> {
    let mut b = Vec::new();
    if !photos.iter().any(|p| p.kind == "before") {
        b.push("缺少清洗前照片".to_string());
    }
    if !photos.iter().any(|p| p.kind == "after") {
        b.push("缺少清洗后照片".to_string());
    }
    if !measurements.iter().any(|m| m.phase == "before") {
        b.push("缺少清洗前风量读数".to_string());
    }
    if !measurements.iter().any(|m| m.phase == "after") {
        b.push("缺少清洗后风量读数".to_string());
    }
    if parts.is_empty() {
        b.push("尚未登记任何拆下零件".to_string());
    }
    b
}

/// 验收检查 = 完工检查 + 所有零件已复装
fn acceptance_blockers(photos: &[PhotoRow], parts: &[PartRow], measurements: &[MeasurementRow]) -> Vec<String> {
    let mut b = completion_blockers(photos, parts, measurements);
    let missing: i64 = parts.iter().filter(|p| !p.reinstalled).map(|p| p.quantity).sum();
    if missing > 0 {
        b.push(format!("还有 {missing} 件零件未复装，不能通过验收"));
    }
    b
}

async fn load_detail(pool: &SqlitePool, id: i64) -> Result<OrderDetail, AppError> {
    let order = fetch_order(pool, id).await?;
    let photos = fetch_photos(pool, id).await?;
    let parts = fetch_parts(pool, id).await?;
    let extras = fetch_extras(pool, id).await?;
    let measurements = fetch_measurements(pool, id).await?;
    let acceptance = fetch_acceptance(pool, id).await?;
    let payment = fetch_payment(pool, id).await?;

    let approved_extras_cents: i64 = extras
        .iter()
        .filter(|e| e.status == "approved")
        .map(|e| e.price_cents)
        .sum();
    let frozen = acceptance.is_some();
    // 验收后金额以验收记录中冻结的总额为准
    let total_cents = acceptance
        .as_ref()
        .map(|a| a.total_cents)
        .unwrap_or(order.base_price_cents + approved_extras_cents);

    let blockers = acceptance_blockers(&photos, &parts, &measurements);
    let base_cents = order.base_price_cents;

    Ok(OrderDetail {
        order,
        photos: photos
            .into_iter()
            .map(|p| PhotoJson {
                id: p.id,
                kind: p.kind,
                url: format!("/uploads/{}", p.file_path),
                created_at: p.created_at,
            })
            .collect(),
        parts,
        extras,
        measurements,
        acceptance,
        payment,
        bill: BillJson {
            base_cents,
            approved_extras_cents,
            total_cents,
            frozen,
        },
        acceptance_blockers: blockers,
    })
}

fn ensure_status(order: &OrderRow, allowed: &[&str], action: &str) -> Result<(), AppError> {
    if allowed.contains(&order.status.as_str()) {
        Ok(())
    } else {
        Err(AppError::conflict(format!(
            "订单当前状态为「{}」，不能{action}",
            status_label(&order.status)
        )))
    }
}

pub fn status_label(s: &str) -> &'static str {
    match s {
        "booked" => "已预约",
        "confirmed" => "已确认范围",
        "in_progress" => "清洗中",
        "awaiting_acceptance" => "待验收",
        "accepted" => "已验收",
        "closed" => "已结单",
        _ => "未知",
    }
}

fn validate_key(key: &str) -> Result<(), AppError> {
    let k = key.trim();
    if k.is_empty() || k.len() > 128 {
        return Err(AppError::bad_request("idempotency_key 不能为空且不超过 128 字符"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 路由处理
// ---------------------------------------------------------------------------

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

/// 模拟测量仪接口：现场“测量仪”每次调用返回一条读数
pub async fn mock_meter_reading() -> Json<meter::MeterReading> {
    Json(meter::take_reading())
}

#[derive(Deserialize)]
pub struct CreateOrder {
    pub customer_name: String,
    pub phone: String,
    pub address: String,
    pub hood_model: String,
    pub appointment_time: String,
    pub base_price_cents: Option<i64>,
}

pub async fn create_order(
    State(state): State<AppState>,
    Json(body): Json<CreateOrder>,
) -> Result<(StatusCode, Json<OrderDetail>), AppError> {
    for (v, name) in [
        (&body.customer_name, "客户姓名"),
        (&body.phone, "联系电话"),
        (&body.address, "服务地址"),
        (&body.hood_model, "油烟机型号"),
        (&body.appointment_time, "预约时间"),
    ] {
        if v.trim().is_empty() {
            return Err(AppError::bad_request(format!("{name}不能为空")));
        }
    }
    let base = body.base_price_cents.unwrap_or(0);
    if base < 0 {
        return Err(AppError::bad_request("基础价格不能为负"));
    }
    let res = sqlx::query(
        "INSERT INTO orders (customer_name, phone, address, hood_model, appointment_time, base_price_cents)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(body.customer_name.trim())
    .bind(body.phone.trim())
    .bind(body.address.trim())
    .bind(body.hood_model.trim())
    .bind(body.appointment_time.trim())
    .bind(base)
    .execute(&state.pool)
    .await?;
    let id = res.last_insert_rowid();
    Ok((StatusCode::CREATED, Json(load_detail(&state.pool, id).await?)))
}

#[derive(Serialize)]
pub struct OrderSummary {
    pub id: i64,
    pub customer_name: String,
    pub hood_model: String,
    pub status: String,
    pub status_label: String,
    pub appointment_time: String,
    pub created_at: String,
}

pub async fn list_orders(State(state): State<AppState>) -> Result<Json<Vec<OrderSummary>>, AppError> {
    let rows = sqlx::query_as::<_, OrderRow>("SELECT * FROM orders ORDER BY id DESC")
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|o| OrderSummary {
                status_label: status_label(&o.status).to_string(),
                id: o.id,
                customer_name: o.customer_name,
                hood_model: o.hood_model,
                status: o.status,
                appointment_time: o.appointment_time,
                created_at: o.created_at,
            })
            .collect(),
    ))
}

pub async fn get_order(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<OrderDetail>, AppError> {
    Ok(Json(load_detail(&state.pool, id).await?))
}

#[derive(Deserialize)]
pub struct PhotoQuery {
    pub kind: String,
}

const MAX_PHOTO_BYTES: usize = 10 * 1024 * 1024;

pub async fn upload_photo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<PhotoQuery>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<OrderDetail>), AppError> {
    let order = fetch_order(&state.pool, id).await?;
    if !matches!(q.kind.as_str(), "booking" | "before" | "after") {
        return Err(AppError::bad_request("kind 必须是 booking / before / after"));
    }
    if matches!(order.status.as_str(), "accepted" | "closed") {
        return Err(AppError::conflict("订单已验收，不能再上传照片"));
    }

    let mut saved: Option<(String, String)> = None; // (ext, bytes as saved path)
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("multipart 解析失败: {e}")))?
    {
        let content_type = field.content_type().unwrap_or("").to_string();
        let ext = match content_type.as_str() {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            "image/gif" => "gif",
            _ => continue, // 跳过非图片字段
        };
        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::bad_request(format!("读取上传内容失败: {e}")))?;
        if data.is_empty() {
            return Err(AppError::bad_request("上传文件为空"));
        }
        if data.len() > MAX_PHOTO_BYTES {
            return Err(AppError::bad_request("图片不能超过 10MB"));
        }
        let dir = state.uploads_dir.join(id.to_string());
        tokio::fs::create_dir_all(&dir).await?;
        let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
        let path = dir.join(&filename);
        tokio::fs::write(&path, &data).await?;
        saved = Some((format!("{id}/{filename}"), q.kind.clone()));
        break; // 每次请求保存一张
    }

    let (file_path, kind) = saved.ok_or_else(|| AppError::bad_request("请求中没有可识别的图片文件（支持 JPG/PNG/WebP/GIF）"))?;
    sqlx::query("INSERT INTO photos (order_id, kind, file_path) VALUES (?, ?, ?)")
        .bind(id)
        .bind(kind)
        .bind(file_path)
        .execute(&state.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(load_detail(&state.pool, id).await?)))
}

#[derive(Deserialize)]
pub struct ConfirmScope {
    pub confirmed_model: String,
    pub base_price_cents: i64,
    pub scope_note: Option<String>,
}

/// 师傅到场确认施工范围；型号不符时如实记录 confirmed_model 与说明
pub async fn confirm_scope(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ConfirmScope>,
) -> Result<Json<OrderDetail>, AppError> {
    let order = fetch_order(&state.pool, id).await?;
    ensure_status(&order, &["booked"], "确认施工范围")?;
    if body.confirmed_model.trim().is_empty() {
        return Err(AppError::bad_request("确认型号不能为空"));
    }
    if body.base_price_cents < 0 {
        return Err(AppError::bad_request("基础价格不能为负"));
    }
    sqlx::query(
        "UPDATE orders SET confirmed_model = ?, base_price_cents = ?, scope_note = ?, status = 'confirmed', updated_at = datetime('now') WHERE id = ?",
    )
    .bind(body.confirmed_model.trim())
    .bind(body.base_price_cents)
    .bind(body.scope_note.as_deref().map(str::trim))
    .bind(id)
    .execute(&state.pool)
    .await?;
    Ok(Json(load_detail(&state.pool, id).await?))
}

pub async fn start_work(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<OrderDetail>, AppError> {
    let order = fetch_order(&state.pool, id).await?;
    ensure_status(&order, &["confirmed"], "开始施工")?;
    sqlx::query("UPDATE orders SET status = 'in_progress', updated_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Json(load_detail(&state.pool, id).await?))
}

#[derive(Deserialize)]
pub struct AddPart {
    pub name: String,
    pub quantity: Option<i64>,
}

pub async fn add_part(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<AddPart>,
) -> Result<(StatusCode, Json<OrderDetail>), AppError> {
    let order = fetch_order(&state.pool, id).await?;
    ensure_status(&order, &["confirmed", "in_progress"], "登记零件")?;
    if body.name.trim().is_empty() {
        return Err(AppError::bad_request("零件名称不能为空"));
    }
    let qty = body.quantity.unwrap_or(1);
    if qty < 1 {
        return Err(AppError::bad_request("零件数量至少为 1"));
    }
    sqlx::query("INSERT INTO parts (order_id, name, quantity) VALUES (?, ?, ?)")
        .bind(id)
        .bind(body.name.trim())
        .bind(qty)
        .execute(&state.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(load_detail(&state.pool, id).await?)))
}

pub async fn reinstall_part(
    State(state): State<AppState>,
    Path((id, pid)): Path<(i64, i64)>,
) -> Result<Json<OrderDetail>, AppError> {
    let order = fetch_order(&state.pool, id).await?;
    ensure_status(
        &order,
        &["confirmed", "in_progress", "awaiting_acceptance"],
        "登记复装",
    )?;
    let res = sqlx::query(
        "UPDATE parts SET reinstalled = 1, reinstalled_at = datetime('now') WHERE id = ? AND order_id = ? AND reinstalled = 0",
    )
    .bind(pid)
    .bind(id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        // 已复装或不存在 —— 区分一下给出准确提示
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM parts WHERE id = ? AND order_id = ?")
            .bind(pid)
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
        if exists.is_none() {
            return Err(AppError::not_found("零件不存在"));
        }
        // 已复装：幂等返回，不视为错误
    }
    Ok(Json(load_detail(&state.pool, id).await?))
}

pub async fn delete_part(
    State(state): State<AppState>,
    Path((id, pid)): Path<(i64, i64)>,
) -> Result<Json<OrderDetail>, AppError> {
    let order = fetch_order(&state.pool, id).await?;
    ensure_status(&order, &["confirmed", "in_progress"], "删除零件记录")?;
    let res = sqlx::query("DELETE FROM parts WHERE id = ? AND order_id = ?")
        .bind(pid)
        .bind(id)
        .execute(&state.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("零件不存在"));
    }
    Ok(Json(load_detail(&state.pool, id).await?))
}

#[derive(Deserialize)]
pub struct CaptureBody {
    pub phase: String,
}

/// 从模拟测量仪取一条读数并记录到订单
pub async fn capture_measurement(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<CaptureBody>,
) -> Result<(StatusCode, Json<OrderDetail>), AppError> {
    let order = fetch_order(&state.pool, id).await?;
    ensure_status(
        &order,
        &["confirmed", "in_progress", "awaiting_acceptance"],
        "记录测量数据",
    )?;
    if !matches!(body.phase.as_str(), "before" | "after") {
        return Err(AppError::bad_request("phase 必须是 before / after"));
    }
    let reading = meter::take_reading();
    sqlx::query(
        "INSERT INTO measurements (order_id, phase, airflow_m3_min, noise_db) VALUES (?, ?, ?, ?)",
    )
    .bind(id)
    .bind(body.phase)
    .bind(reading.airflow_m3_min)
    .bind(reading.noise_db)
    .execute(&state.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(load_detail(&state.pool, id).await?)))
}

#[derive(Deserialize)]
pub struct ProposeExtra {
    pub description: String,
    pub price_cents: i64,
}

/// 师傅提出加价项目（新型号不符、新增服务等）。此时只是“方案”，不计入账单。
pub async fn propose_extra(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ProposeExtra>,
) -> Result<(StatusCode, Json<OrderDetail>), AppError> {
    let order = fetch_order(&state.pool, id).await?;
    ensure_status(
        &order,
        &["confirmed", "in_progress", "awaiting_acceptance"],
        "提出加价项目",
    )?;
    if body.description.trim().is_empty() {
        return Err(AppError::bad_request("加价说明不能为空"));
    }
    if body.price_cents <= 0 {
        return Err(AppError::bad_request("加价金额必须大于 0"));
    }
    sqlx::query("INSERT INTO extra_items (order_id, description, price_cents) VALUES (?, ?, ?)")
        .bind(id)
        .bind(body.description.trim())
        .bind(body.price_cents)
        .execute(&state.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(load_detail(&state.pool, id).await?)))
}

async fn decide_extra(
    state: &AppState,
    id: i64,
    eid: i64,
    decision: &'static str,
) -> Result<Json<OrderDetail>, AppError> {
    let order = fetch_order(&state.pool, id).await?;
    // 验收后账单冻结，任何加价变更都不再生效
    ensure_status(
        &order,
        &["confirmed", "in_progress", "awaiting_acceptance"],
        "处理加价项目",
    )?;
    let res = sqlx::query(
        "UPDATE extra_items SET status = ?, decided_at = datetime('now') WHERE id = ? AND order_id = ? AND status = 'pending'",
    )
    .bind(decision)
    .bind(eid)
    .bind(id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM extra_items WHERE id = ? AND order_id = ?")
            .bind(eid)
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
        return Err(if exists.is_none() {
            AppError::not_found("加价项目不存在")
        } else {
            AppError::conflict("该加价项目已处理，不能重复决定")
        });
    }
    Ok(Json(load_detail(&state.pool, id).await?))
}

/// 客户同意加价 —— 只有经过这一步，项目才计入账单
pub async fn approve_extra(
    State(state): State<AppState>,
    Path((id, eid)): Path<(i64, i64)>,
) -> Result<Json<OrderDetail>, AppError> {
    decide_extra(&state, id, eid, "approved").await
}

/// 客户拒绝加价 —— 按原范围继续执行，该项目永不计费
pub async fn reject_extra(
    State(state): State<AppState>,
    Path((id, eid)): Path<(i64, i64)>,
) -> Result<Json<OrderDetail>, AppError> {
    decide_extra(&state, id, eid, "rejected").await
}

/// 师傅完工，进入待验收。要求照片、测量、零件登记齐全（复装情况在验收时校验）。
pub async fn complete_work(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<OrderDetail>, AppError> {
    let order = fetch_order(&state.pool, id).await?;
    ensure_status(&order, &["in_progress"], "完工提交")?;
    let photos = fetch_photos(&state.pool, id).await?;
    let parts = fetch_parts(&state.pool, id).await?;
    let measurements = fetch_measurements(&state.pool, id).await?;
    let blockers = completion_blockers(&photos, &parts, &measurements);
    if !blockers.is_empty() {
        return Err(AppError::conflict(format!("无法完工：{}", blockers.join("；"))));
    }
    sqlx::query("UPDATE orders SET status = 'awaiting_acceptance', updated_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Json(load_detail(&state.pool, id).await?))
}

#[derive(Deserialize)]
pub struct IdempotentBody {
    pub idempotency_key: String,
}

/// 客户确认验收。幂等：同一 idempotency_key 重放返回原结果；不同 key 重复验收返回 409。
pub async fn accept_order(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<IdempotentBody>,
) -> Result<Json<OrderDetail>, AppError> {
    validate_key(&body.idempotency_key)?;
    let key = body.idempotency_key.trim();
    let mut tx = state.pool.begin().await?;

    let order = sqlx::query_as::<_, OrderRow>("SELECT * FROM orders WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::not_found(format!("订单 {id} 不存在")))?;

    match order.status.as_str() {
        "accepted" | "closed" => {
            // 幂等重放：同一 key 直接返回成功；不同 key 说明是重复提交
            let acc = fetch_acceptance(&state.pool, id).await?;
            return match acc {
                Some(a) if a.idempotency_key == key => {
                    drop(tx);
                    Ok(Json(load_detail(&state.pool, id).await?))
                }
                _ => Err(AppError::conflict("订单已验收，请勿重复提交")),
            };
        }
        "awaiting_acceptance" => {}
        _ => return Err(AppError::conflict("订单尚未完工，不能验收")),
    }

    let photos = fetch_photos(&state.pool, id).await?;
    let parts = fetch_parts(&state.pool, id).await?;
    let measurements = fetch_measurements(&state.pool, id).await?;
    let blockers = acceptance_blockers(&photos, &parts, &measurements);
    if !blockers.is_empty() {
        return Err(AppError::conflict(format!("验收未通过：{}", blockers.join("；"))));
    }

    // 金额在验收这一刻冻结：基础价 + 已确认加价（pending/rejected 一律不计）
    let approved_sum: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(price_cents), 0) FROM extra_items WHERE order_id = ? AND status = 'approved'",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let total = order.base_price_cents + approved_sum;

    let res = sqlx::query(
        "INSERT OR IGNORE INTO acceptances (order_id, idempotency_key, total_cents) VALUES (?, ?, ?)",
    )
    .bind(id)
    .bind(key)
    .bind(total)
    .execute(&mut *tx)
    .await?;

    if res.rows_affected() == 0 {
        // 唯一约束冲突：同单已验收（并发）或 key 被其他订单占用
        let existing = sqlx::query_as::<_, AcceptanceRow>(
            "SELECT id, idempotency_key, total_cents, confirmed_at FROM acceptances WHERE order_id = ?",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        return match existing {
            Some(a) if a.idempotency_key == key => {
                tx.commit().await?;
                Ok(Json(load_detail(&state.pool, id).await?))
            }
            Some(_) => Err(AppError::conflict("订单已验收，请勿重复提交")),
            None => Err(AppError::conflict("该幂等键已被其他请求使用，请更换")),
        };
    }

    sqlx::query("UPDATE orders SET status = 'accepted', updated_at = datetime('now') WHERE id = ? AND status = 'awaiting_acceptance'")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(load_detail(&state.pool, id).await?))
}

/// 收款结单。金额取自验收时冻结的总额；幂等键保证重复提交只收一次。
pub async fn pay_order(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<IdempotentBody>,
) -> Result<Json<OrderDetail>, AppError> {
    validate_key(&body.idempotency_key)?;
    let key = body.idempotency_key.trim();
    let mut tx = state.pool.begin().await?;

    let order = sqlx::query_as::<_, OrderRow>("SELECT * FROM orders WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::not_found(format!("订单 {id} 不存在")))?;

    match order.status.as_str() {
        "closed" => {
            let pay = fetch_payment(&state.pool, id).await?;
            return match pay {
                Some(p) if p.idempotency_key == key => {
                    drop(tx);
                    Ok(Json(load_detail(&state.pool, id).await?))
                }
                _ => Err(AppError::conflict("订单已结单，请勿重复收款")),
            };
        }
        "accepted" => {}
        _ => return Err(AppError::conflict("请先完成客户验收，再办理结单")),
    }

    let acceptance = sqlx::query_as::<_, AcceptanceRow>(
        "SELECT id, idempotency_key, total_cents, confirmed_at FROM acceptances WHERE order_id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::conflict("缺少验收记录，不能结单"))?;
    let amount = acceptance.total_cents;

    let res = sqlx::query(
        "INSERT OR IGNORE INTO payments (order_id, idempotency_key, amount_cents) VALUES (?, ?, ?)",
    )
    .bind(id)
    .bind(key)
    .bind(amount)
    .execute(&mut *tx)
    .await?;

    if res.rows_affected() == 0 {
        let existing = sqlx::query_as::<_, PaymentRow>(
            "SELECT id, idempotency_key, amount_cents, paid_at FROM payments WHERE order_id = ?",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        return match existing {
            Some(p) if p.idempotency_key == key => {
                tx.commit().await?;
                Ok(Json(load_detail(&state.pool, id).await?))
            }
            Some(_) => Err(AppError::conflict("订单已结单，请勿重复收款")),
            None => Err(AppError::conflict("该幂等键已被其他请求使用，请更换")),
        };
    }

    sqlx::query("UPDATE orders SET status = 'closed', updated_at = datetime('now') WHERE id = ? AND status = 'accepted'")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(load_detail(&state.pool, id).await?))
}
