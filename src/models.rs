use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

use crate::error::{ApiError, ApiResult};

#[derive(Debug, FromRow, Serialize)]
pub struct OrderRow {
    pub id: String,
    pub model: String,
    pub customer_name: String,
    pub customer_phone: String,
    pub appointment_at: String,
    pub base_fee_cents: i64,
    pub actual_model: Option<String>,
    pub status: String,
    pub scope_note: String,
    pub created_at: String,
    pub scope_confirmed_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, FromRow, Serialize)]
pub struct AddonRow {
    pub id: String,
    pub order_id: String,
    pub code: String,
    pub title: String,
    pub description: String,
    pub price_cents: i64,
    pub extra_json: String,
    pub status: String, // pending / approved / rejected
    pub applied: i64,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub applied_at: Option<String>,
}

#[derive(Debug, FromRow, Serialize)]
pub struct PartRow {
    pub id: String,
    pub order_id: String,
    pub name: String,
    pub removed_at: String,
    pub reinstalled_at: Option<String>,
    pub note: String,
}

#[derive(Debug, FromRow, Serialize)]
pub struct PhotoRow {
    pub id: String,
    pub order_id: String,
    pub phase: String,
    pub label: String,
    pub rel_path: String,
    pub content_type: String,
    pub created_at: String,
}

#[derive(Debug, FromRow, Serialize)]
pub struct MeasurementRow {
    pub id: String,
    pub order_id: String,
    pub phase: String,
    pub airflow_m3h: f64,
    pub source: String,
    pub note: String,
    pub recorded_at: String,
}

#[derive(Debug, FromRow, Serialize)]
pub struct PaymentRow {
    pub id: String,
    pub order_id: String,
    pub amount_cents: i64,
    pub detail_json: String,
    pub created_at: String,
}

pub async fn get_order(pool: &SqlitePool, id: &str) -> ApiResult<OrderRow> {
    sqlx::query_as::<_, OrderRow>("SELECT * FROM orders WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("订单 {id} 不存在")))
}

pub async fn list_addons(pool: &SqlitePool, order_id: &str) -> ApiResult<Vec<AddonRow>> {
    Ok(
        sqlx::query_as::<_, AddonRow>("SELECT * FROM addons WHERE order_id = ?1 ORDER BY created_at")
            .bind(order_id)
            .fetch_all(pool)
            .await?,
    )
}

pub async fn list_parts(pool: &SqlitePool, order_id: &str) -> ApiResult<Vec<PartRow>> {
    Ok(
        sqlx::query_as::<_, PartRow>("SELECT * FROM parts WHERE order_id = ?1 ORDER BY removed_at")
            .bind(order_id)
            .fetch_all(pool)
            .await?,
    )
}

pub async fn list_photos(pool: &SqlitePool, order_id: &str) -> ApiResult<Vec<PhotoRow>> {
    Ok(
        sqlx::query_as::<_, PhotoRow>("SELECT * FROM photos WHERE order_id = ?1 ORDER BY created_at")
            .bind(order_id)
            .fetch_all(pool)
            .await?,
    )
}

pub async fn list_measurements(pool: &SqlitePool, order_id: &str) -> ApiResult<Vec<MeasurementRow>> {
    Ok(sqlx::query_as::<_, MeasurementRow>(
        "SELECT * FROM measurements WHERE order_id = ?1 ORDER BY phase",
    )
    .bind(order_id)
    .fetch_all(pool)
    .await?)
}

pub async fn get_payment(pool: &SqlitePool, order_id: &str) -> ApiResult<Option<PaymentRow>> {
    Ok(sqlx::query_as::<_, PaymentRow>("SELECT * FROM payments WHERE order_id = ?1")
        .bind(order_id)
        .fetch_optional(pool)
        .await?)
}

/// 已获客户确认、且师傅已按方案实施的加价项目 —— 只有这些才计入收款金额。
/// pending（未确认）与 rejected（客户拒绝）一律不计费、不生效。
pub async fn billable_addons(pool: &SqlitePool, order_id: &str) -> ApiResult<Vec<AddonRow>> {
    Ok(sqlx::query_as::<_, AddonRow>(
        "SELECT * FROM addons WHERE order_id = ?1 AND status = 'approved' AND applied = 1",
    )
    .bind(order_id)
    .fetch_all(pool)
    .await?)
}

pub async fn get_accepted_at(pool: &SqlitePool, order_id: &str) -> ApiResult<Option<String>> {
    let ts: Option<(String,)> =
        sqlx::query_as("SELECT created_at FROM acceptance_events WHERE order_id = ?1 AND accepted = 1")
            .bind(order_id)
            .fetch_optional(pool)
            .await?;
    Ok(ts.map(|t| t.0))
}
