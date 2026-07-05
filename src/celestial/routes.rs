use axum::{extract::Query, routing::get, Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::{app::AppState, celestial::Sky, error::AppResult};

pub fn router() -> Router<AppState> {
    Router::new().route("/sky", get(sky_handler))
}

#[derive(Debug, Deserialize)]
struct SkyQuery {
    /// Optional ISO-8601 timestamp; defaults to "now". Lets a client
    /// look up the sky at any past or future moment.
    at: Option<DateTime<Utc>>,
}

async fn sky_handler(Query(params): Query<SkyQuery>) -> AppResult<Json<Sky>> {
    let t = params.at.unwrap_or_else(Utc::now);
    Ok(Json(Sky::at(t)))
}
