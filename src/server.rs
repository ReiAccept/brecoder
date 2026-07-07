use crate::monitor::AppState;
use axum::{extract::State, routing::get, Json, Router};
use tower_http::cors::{Any, CorsLayer};

/// Build the axum router
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/status", get(status_json))
        .layer(cors)
        .with_state(state)
}

/// JSON endpoint returning current stream statuses
async fn status_json(State(state): State<AppState>) -> Json<serde_json::Value> {
    let statuses = state.statuses.read().await;
    Json(serde_json::json!({
        "count": statuses.len(),
        "streams": &*statuses,
    }))
}
