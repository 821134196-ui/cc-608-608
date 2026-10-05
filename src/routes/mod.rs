mod acceptance;
mod addons;
mod measurements;
mod orders;
mod parts;
mod photos;

use axum::{routing::{get, post}, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // 订单
        .route("/orders", post(orders::create_order).get(orders::list_orders))
        .route("/orders/{id}", get(orders::order_detail))
        .route("/orders/{id}/arrive", post(orders::arrive_confirm))
        .route("/orders/{id}/start-work", post(orders::start_work))
        .route("/orders/{id}/complete-work", post(orders::complete_work))
        // 加价 / 改方案
        .route("/orders/{id}/addons", post(addons::propose_addon).get(addons::list))
        .route(
            "/orders/{id}/addons/{addon_id}/decision",
            post(addons::decide),
        )
        .route(
            "/orders/{id}/addons/{addon_id}/apply",
            post(addons::mark_applied),
        )
        // 零件拆装
        .route("/orders/{id}/parts", post(parts::add_part).get(parts::list))
        .route("/orders/{id}/parts/{part_id}/reinstall", post(parts::reinstall))
        // 照片
        .route("/orders/{id}/photos", post(photos::upload).get(photos::list))
        // 风量（模拟测量仪）
        .route("/orders/{id}/meter/{phase}", get(measurements::read_meter))
        .route("/orders/{id}/measurements", post(measurements::record))
        // 验收与结单
        .route("/orders/{id}/acceptance", post(acceptance::submit))
        .route("/orders/{id}/payment", post(acceptance::pay))
}
