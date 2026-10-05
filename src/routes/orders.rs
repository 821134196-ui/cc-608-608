use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{
    billable_addons, get_accepted_at, get_order, get_payment, list_addons, list_measurements,
    list_parts, list_photos,
};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreateOrderReq {
    pub model: String,
    pub customer_name: String,
    pub customer_phone: String,
    pub appointment_at: String,
    #[serde(default)]
    pub base_fee_cents: Option<i64>,
    #[serde(default)]
    pub scope_note: String,
}

pub async fn create_order(
    State(st): State<AppState>,
    Json(req): Json<CreateOrderReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if req.model.trim().is_empty()
        || req.customer_name.trim().is_empty()
        || req.customer_phone.trim().is_empty()
        || req.appointment_at.trim().is_empty()
    {
        return Err(ApiError::bad_request("型号、联系人、电话和预约时间均不能为空"));
    }
    let fee = req.base_fee_cents.unwrap_or(19800); // 默认清洗套餐 198.00 元
    if fee < 0 {
        return Err(ApiError::bad_request("套餐价格不能为负"));
    }
    let id = Uuid::new_v4().simple().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO orders
            (id, model, customer_name, customer_phone, appointment_at, base_fee_cents, scope_note, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
    )
    .bind(&id)
    .bind(req.model.trim())
    .bind(req.customer_name.trim())
    .bind(req.customer_phone.trim())
    .bind(&req.appointment_at)
    .bind(fee)
    .bind(&req.scope_note)
    .bind(&now)
    .execute(&st.pool)
    .await?;

    let order = get_order(&st.pool, &id).await?;
    Ok(Json(json!({ "order": order })))
}

pub async fn list_orders(State(st): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let orders =
        sqlx::query_as::<_, crate::models::OrderRow>("SELECT * FROM orders ORDER BY created_at DESC")
            .fetch_all(&st.pool)
            .await?;
    Ok(Json(json!({ "orders": orders })))
}

#[derive(Serialize)]
pub(crate) struct OrderDetail {
    #[serde(flatten)]
    order: crate::models::OrderRow,
    addons: Vec<crate::models::AddonRow>,
    parts: Vec<crate::models::PartRow>,
    photos: Vec<crate::models::PhotoRow>,
    measurements: Vec<crate::models::MeasurementRow>,
    payment: Option<crate::models::PaymentRow>,
    accepted_at: Option<String>,
    /// 可结算金额构成：基础费 + 已确认且已施工的加价；待确认/已拒绝项目不计入
    bill: Bill,
    /// 缺件名称清单（已拆下但未复装）
    missing_parts: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct Bill {
    base_fee_cents: i64,
    addon_fee_cents: i64,
    total_cents: i64,
    lines: Vec<serde_json::Value>,
}

pub async fn order_detail(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<OrderDetail>> {
    let order = get_order(&st.pool, &id).await?;
    let addons = list_addons(&st.pool, &id).await?;
    let parts = list_parts(&st.pool, &id).await?;
    let photos = list_photos(&st.pool, &id).await?;
    let measurements = list_measurements(&st.pool, &id).await?;
    let payment = get_payment(&st.pool, &id).await?;
    let accepted_at = get_accepted_at(&st.pool, &id).await?;

    let billable = billable_addons(&st.pool, &id).await?;
    let mut lines = vec![json!({
        "kind": "base", "title": "标准清洗套餐", "price_cents": order.base_fee_cents,
    })];
    let mut addon_fee = 0i64;
    for a in &billable {
        addon_fee += a.price_cents;
        lines.push(json!({
            "kind": "addon", "addon_id": a.id, "code": a.code,
            "title": a.title, "price_cents": a.price_cents,
        }));
    }
    let total = order.base_fee_cents + addon_fee;
    let base_fee = order.base_fee_cents;

    let missing_parts = parts
        .iter()
        .filter(|p| p.reinstalled_at.is_none())
        .map(|p| p.name.clone())
        .collect();

    Ok(Json(OrderDetail {
        order,
        addons,
        parts,
        photos,
        measurements,
        payment,
        accepted_at,
        bill: Bill {
            base_fee_cents: base_fee,
            addon_fee_cents: addon_fee,
            total_cents: total,
            lines,
        },
        missing_parts,
    }))
}

#[derive(Deserialize)]
pub struct ArriveReq {
    /// 师傅现场核对后的型号；与预约一致时可直接确认范围开工
    pub actual_model: String,
    #[serde(default)]
    pub scope_note: String,
}

/// 师傅到场：确认型号与施工范围，进入施工中。
/// 型号不符时不得擅自开工——应先创建“型号不符”加价方案，等客户在手机上确认。
pub async fn arrive_confirm(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ArriveReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let order = get_order(&st.pool, &id).await?;
    if order.status != "pending" {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，不能重复到场确认",
            order.status
        )));
    }

    let mismatch = req.actual_model.trim() != order.model.trim();
    let now = Utc::now().to_rfc3339();

    if mismatch {
        // 已存在待决定的型号不符方案时，不允许用第二次“到场确认”覆盖它
        let existing = list_addons(&st.pool, &id).await?;
        if let Some(a) = existing
            .iter()
            .find(|a| a.code == "MODEL_MISMATCH" && a.status == "pending")
        {
            return Err(ApiError::conflict(format!(
                "已生成型号不符方案（{}），正等待客户确认；请等客户决定后再开工，勿重复提交",
                a.title
            )));
        }

        // 型号不符：保持 pending，并自动生成一条待客户确认的“型号不符方案”
        let addon_id = Uuid::new_v4().simple().to_string();
        sqlx::query(
            "INSERT INTO addons (id, order_id, code, title, description, price_cents, extra_json, created_at)
             VALUES (?1,?2,'MODEL_MISMATCH',?3,?4,?5,?6,?7)",
        )
        .bind(&addon_id)
        .bind(&id)
        .bind("现场型号不符，需调整清洗方案")
        .bind(format!(
            "预约型号「{}」与现场型号「{}」不符，请客户确认新方案与差价。客户拒绝则仍按原范围执行。",
            order.model,
            req.actual_model.trim()
        ))
        .bind(0)
        .bind(serde_json::json!({
            "appointment_model": order.model,
            "actual_model": req.actual_model.trim(),
        }).to_string())
        .bind(&now)
        .execute(&st.pool)
        .await?;

        return Ok(Json(json!({
            "ok": true,
            "started": false,
            "warning": "现场型号与预约不符，已生成待确认方案；客户确认前不得按新方案施工或加价",
            "addon_id": addon_id,
        })));
    }

    sqlx::query(
        "UPDATE orders SET status='in_progress', actual_model=?1, scope_note=?2, scope_confirmed_at=?3 WHERE id=?4",
    )
    .bind(req.actual_model.trim())
    .bind(if req.scope_note.trim().is_empty() { order.scope_note.clone() } else { req.scope_note })
    .bind(&now)
    .bind(&id)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true, "started": true, "status": "in_progress" })))
}

/// 型号不符方案经客户决定后开工。
/// - approved：按现场型号（新方案）开工；
/// - rejected：仍按原预约型号与原范围开工（actual_model 记录原型号）；
/// - pending：拒绝开工——未确认的新方案不得实施。
pub async fn start_work(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let order = get_order(&st.pool, &id).await?;
    if order.status != "pending" {
        return Err(ApiError::conflict(format!("当前状态为 {}，无需重复开工", order.status)));
    }

    let addons = list_addons(&st.pool, &id).await?;
    let mismatch = addons.iter().find(|a| a.code == "MODEL_MISMATCH");
    let mismatch = match mismatch {
        Some(a) => a,
        None => {
            return Err(ApiError::bad_request(
                "不存在型号不符方案；型号一致时到场确认即自动开工",
            ));
        }
    };

    let now = Utc::now().to_rfc3339();
    match mismatch.status.as_str() {
        "pending" => Err(ApiError::conflict(
            "型号不符的新方案尚待客户确认，客户确认前不能按新方案施工",
        )),
        "rejected" => {
            // 客户拒绝 → 按原范围执行
            sqlx::query(
                "UPDATE orders SET status='in_progress', actual_model=?1, scope_confirmed_at=?2 WHERE id=?3",
            )
            .bind(&order.model)
            .bind(&now)
            .bind(&id)
            .execute(&st.pool)
            .await?;
            Ok(Json(json!({
                "ok": true, "status": "in_progress", "actual_model": order.model,
                "notice": "客户拒绝新方案，已按原型号、原范围开工",
            })))
        }
        "approved" => {
            let extra: serde_json::Value =
                serde_json::from_str(&mismatch.extra_json).unwrap_or(json!({}));
            let actual = extra
                .get("actual_model")
                .and_then(|v| v.as_str())
                .unwrap_or(&order.model)
                .to_string();
            sqlx::query(
                "UPDATE orders SET status='in_progress', actual_model=?1, scope_confirmed_at=?2 WHERE id=?3",
            )
            .bind(&actual)
            .bind(&now)
            .bind(&id)
            .execute(&st.pool)
            .await?;
            Ok(Json(json!({
                "ok": true, "status": "in_progress", "actual_model": actual,
                "notice": "客户已确认新方案，按现场型号开工",
            })))
        }
        other => Err(ApiError::Internal(anyhow::anyhow!("未知方案状态 {other}"))),
    }
}

#[derive(Deserialize)]
pub struct CompleteWorkReq {
    #[serde(default)]
    pub note: String,
}

/// 师傅完工自检：要求所有拆下零件已复装、洗后照片与洗后风量已记录，
/// 之后订单进入“待客户验收”。
pub async fn complete_work(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CompleteWorkReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let order = get_order(&st.pool, &id).await?;
    if order.status != "in_progress" {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，不能提交完工",
            order.status
        )));
    }

    // 有待客户决定的加价方案时不能完工——必须先得到明确的同意/拒绝，
    // 避免“先做了再说”或把未确认项目裹进完工页
    let addons = list_addons(&st.pool, &id).await?;
    let pending: Vec<&str> = addons
        .iter()
        .filter(|a| a.status == "pending")
        .map(|a| a.title.as_str())
        .collect();
    if !pending.is_empty() {
        return Err(ApiError::conflict(format!(
            "还有 {} 个加价方案待客户确认（{}），未确认的项目不能施工；客户拒绝后按原范围执行",
            pending.len(),
            pending.join("、")
        )));
    }

    let parts = list_parts(&st.pool, &id).await?;
    let missing: Vec<&str> = parts
        .iter()
        .filter(|p| p.reinstalled_at.is_none())
        .map(|p| p.name.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(ApiError::conflict(format!(
            "尚有 {} 个零件未复装（{}），零件完整复装前不能提交完工",
            missing.len(),
            missing.join("、")
        )));
    }
    if parts.is_empty() {
        return Err(ApiError::bad_request(
            "还未登记任何拆装零件，请先逐件登记拆下的零件并完成复装",
        ));
    }

    let photos = list_photos(&st.pool, &id).await?;
    let has_before = photos.iter().any(|p| p.phase == "before");
    let has_after = photos.iter().any(|p| p.phase == "after");
    if !has_before || !has_after {
        return Err(ApiError::bad_request(
            "缺少清洗前或清洗后的对比照片，不能提交完工",
        ));
    }

    let measurements = list_measurements(&st.pool, &id).await?;
    let before = measurements.iter().find(|m| m.phase == "before");
    let after = measurements.iter().find(|m| m.phase == "after");
    if before.is_none() || after.is_none() {
        return Err(ApiError::bad_request(
            "缺少清洗前/后的风量读数，请用测量仪完成测量",
        ));
    }
    let after = after.unwrap();
    let before = before.unwrap();
    if after.airflow_m3h <= before.airflow_m3h {
        return Err(ApiError::conflict(format!(
            "洗后风量 {:.1} 未高于洗前 {:.1}，请检查复装后再提交",
            after.airflow_m3h, before.airflow_m3h
        )));
    }

    sqlx::query("UPDATE orders SET status='awaiting_acceptance' WHERE id=?1")
        .bind(&id)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({
        "ok": true, "status": "awaiting_acceptance",
        "note": req.note,
    })))
}
