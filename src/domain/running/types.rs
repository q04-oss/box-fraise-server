use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct SignedIn {
    pub username: String,
    /// Returned once and never again — the server keeps only the hash.
    /// Browsers get it as an HttpOnly cookie and never see this field.
    #[serde(skip_serializing)]
    pub token: String,
}

#[derive(Deserialize)]
pub struct LogRun {
    pub distance_m: i32,
    pub duration_s: i32,
}

#[derive(Serialize, FromRow)]
pub struct Run {
    pub id: Uuid,
    pub distance_m: i32,
    pub duration_s: i32,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

/// A runner's own page: who they are, what they have logged, and the
/// score that puts them on the board.
#[derive(Serialize)]
pub struct Me {
    pub username: String,
    pub runs: Vec<Run>,
    pub score: f64,
    pub total_m: i64,
}

/// A row on the board.
///
/// `score` is the average of distance × speed across a runner's logged
/// runs — far and fast, divided by how many they have logged, so that
/// piling up short easy runs cannot climb it. The formula is printed on
/// the page, because a score nobody can check is a score nobody trusts.
#[derive(Serialize, FromRow)]
pub struct BoardRow {
    pub username: String,
    pub score: f64,
    pub runs: i64,
    pub total_m: i64,
}
