use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ScoreUpload {
    pub initials: String,
    pub metres: i32,
}

#[derive(Serialize, FromRow)]
pub struct BoardRow {
    pub id: Uuid,
    pub initials: String,
    pub metres: i32,
    pub achieved_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ScorePosted {
    /// Where it landed, or None if it did not make the board.
    pub rank: Option<i64>,
}
