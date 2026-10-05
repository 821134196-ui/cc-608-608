//! 客户验收与结单收款。
//!
//! 规则：
//! - 只有师傅提交完工（awaiting_acceptance）后客户才能验收；
//! - 验收时再次校验零件全部复装（缺件一票否决），且要求前后照片/风量齐全；
//! - 验收每单最多办理一次（唯一索引 + 状态双重兜底），重复提交不会二次办理；
//! - 只有已验收的订单才能结单收款；
//! - 收款金额 = 基础费 + 已确认(approved)且已施工(applied) 的加价；
//!   待确认/已拒绝的项目一律不计入；
//! - 收款在事务内完成：先查是否已收款，再写 payment 与模拟扣款流水，
//!   重复提交绝不可能产生第二笔扣款。

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{billable_addons, get_order, list_measurements, list_parts, list_photos};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct AcceptanceReq {
    /// 客户必须显式 accepted=true 才通过；false 表示验收驳回（退回施工中整改）
    pub accepted: bool,
    #[serde(default)]
    pub note: String,
}

pub async fn submit(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AcceptanceReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let order = get_order(&st.pool, &id).await?;

    if !req.accepted {
        // 客户驳回：退回施工中，要求师傅整改后重新提交完工
        if order.status != "awaiting_acceptance" {
            return Err(ApiError::conflict(format!(
                "当前状态为 {}，无法驳回",
                order.status
            )));
        }
        sqlx::query("UPDATE orders SET status='in_progress' WHERE id=?1")
            .bind(&id)
            .execute(&st.pool)
            .await?;
        return Ok(Json(json!({
            "ok": true, "accepted": false, "status": "in_progress",
            "notice": "客户驳回验收，订单退回施工整改",
        })));
    }

    // 已验收 → 重复提交直接拒绝办理（只有第一次生效）
    if order.status == "accepted" || order.status == "completed" {
        return Err(ApiError::conflict(
            "该订单已完成客户验收，验收只能办理一次，请勿重复提交",
        ));
    }
    if order.status != "awaiting_acceptance" {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，师傅尚未提交完工，客户还不能验收",
            order.status
        )));
    }

    // —— 验收闸门：缺件一票否决（服务端复核，不信任前端）——
    let parts = list_parts(&st.pool, &id).await?;
    let missing: Vec<&str> = parts
        .iter()
        .filter(|p| p.reinstalled_at.is_none())
        .map(|p| p.name.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(ApiError::conflict(format!(
            "验收不通过：以下 {} 个拆下零件尚未复装：{}。零件完整复装前不能验收",
            missing.len(),
            missing.join("、")
        )));
    }

    let photos = list_photos(&st.pool, &id).await?;
    if !photos.iter().any(|p| p.phase == "before") || !photos.iter().any(|p| p.phase == "after") {
        return Err(ApiError::bad_request("验收材料不完整：缺少清洗前/后对比照片"));
    }
    let measurements = list_measurements(&st.pool, &id).await?;
    let before = measurements.iter().find(|m| m.phase == "before");
    let after = measurements.iter().find(|m| m.phase == "after");
    match (before, after) {
        (Some(b), Some(a)) if a.airflow_m3h > b.airflow_m3h => {}
        (Some(_), Some(_)) => {
            return Err(ApiError::conflict("洗后风量未高于洗前，复装可能有问题，不能验收"));
        }
        _ => return Err(ApiError::bad_request("验收材料不完整：缺少清洗前/后风量读数")),
    }

    let now = Utc::now().to_rfc3339();
    let event_id = Uuid::new_v4().simple().to_string();

    // 落验收记录 + 改状态。唯一索引 idx_acceptance_one_accepted 兜底，
    // 即使两个请求并发，也只有一个 accepted 事件能写入。
    let mut tx = st.pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO acceptance_events (id, order_id, accepted, note, created_at)
         VALUES (?1, ?2, 1, ?3, ?4)",
    )
    .bind(&event_id)
    .bind(&id)
    .bind(&req.note)
    .bind(&now)
    .execute(&mut *tx)
    .await;

    if let Err(e) = inserted {
        tx.rollback().await?;
        return Err(e.into()); // 唯一冲突 → ApiError::Conflict（重复验收）
    }

    sqlx::query("UPDATE orders SET status='accepted' WHERE id=?1 AND status='awaiting_acceptance'")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Json(json!({
        "ok": true, "accepted": true, "status": "accepted",
        "accepted_at": now,
        "notice": "客户验收通过，可以结单收款",
    })))
}

#[derive(Deserialize)]
pub struct PaymentReq {
    /// 可选：支付方式备注，仅展示用，不影响金额
    #[serde(default)]
    pub method: String,
}

pub async fn pay(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PaymentReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let order = get_order(&st.pool, &id).await?;

    if order.status == "completed" {
        return Err(ApiError::conflict(
            "该订单已结单收款，收款只能办理一次，请勿重复提交",
        ));
    }
    if order.status != "accepted" {
        return Err(ApiError::conflict(format!(
            "客户尚未完成验收（当前状态：{}），只有确认验收后才能结单收款",
            order.status
        )));
    }

    // 金额由服务端按实际生效项目重算，前端传多少都不作数
    let billable = billable_addons(&st.pool, &id).await?;
    let mut addon_fee = 0i64;
    let mut lines = vec![json!({
        "kind": "base", "title": "标准清洗套餐", "price_cents": order.base_fee_cents,
    })];
    for a in &billable {
        addon_fee += a.price_cents;
        lines.push(json!({
            "kind": "addon", "addon_id": a.id, "code": a.code,
            "title": a.title, "price_cents": a.price_cents,
        }));
    }
    let total = order.base_fee_cents + addon_fee;

    let now = Utc::now().to_rfc3339();
    let payment_id = Uuid::new_v4().simple().to_string();
    let charge_id = Uuid::new_v4().simple().to_string();

    // 事务 BEGIN IMMEDIATE：进入事务即拿写锁，把并发结单串行化，
    // 重复提交时第二个事务必然看到已存在的 payment（而不是双双 SELECT 为空）。
    let mut tx = st
        .pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(ApiError::from)?;
    let existed: Option<(String,)> =
        sqlx::query_as("SELECT id FROM payments WHERE order_id = ?1")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?;
    if existed.is_some() {
        tx.rollback().await?;
        return Err(ApiError::conflict("收款记录已存在，重复提交不会重复扣款"));
    }

    sqlx::query(
        "INSERT INTO payments (id, order_id, amount_cents, detail_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&payment_id)
    .bind(&id)
    .bind(total)
    .bind(serde_json::to_string(&lines).unwrap_or_else(|_| "[]".into()))
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // 模拟支付网关扣款流水——只有真正结单时写一次
    sqlx::query(
        "INSERT INTO charge_events (id, order_id, amount_cents, created_at)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&charge_id)
    .bind(&id)
    .bind(total)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE orders SET status='completed', completed_at=?1 WHERE id=?2 AND status='accepted'")
        .bind(&now)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Json(json!({
        "ok": true,
        "status": "completed",
        "payment_id": payment_id,
        "charge_id": charge_id,
        "method": req.method,
        "amount_cents": total,
        "breakdown": {
            "base_fee_cents": order.base_fee_cents,
            "addon_fee_cents": addon_fee,
            "lines": lines,
        },
        "paid_at": now,
    })))
}
