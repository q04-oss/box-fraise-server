use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    audit,
    crypto::{argon2_verify, new_session_token},
    db::{AdminRlsTransaction, Pool},
    error::{AppError, AppResult},
};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

pub async fn login(
    pool: &Pool,
    ttl: chrono::Duration,
    req: LoginRequest,
) -> AppResult<LoginResponse> {
    // Read the admin row under an admin-scoped transaction (the only
    // context that satisfies the admins SELECT policy). This is NOT
    // the auth boundary — Argon2 below is. The RLS transaction is just
    // how we get permission to read the table at all.
    let email = req.email.trim().to_lowercase();
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, password_hash FROM admins WHERE email = $1")
            .bind(&email)
            .fetch_optional(tx.conn())
            .await?;
    tx.commit().await?;

    let Some((admin_id, password_hash)) = row else {
        // Don't leak whether the email is registered; same 401 either way.
        return Err(AppError::Unauthorized);
    };
    if !argon2_verify(&req.password, &password_hash) {
        return Err(AppError::Unauthorized);
    }

    let (token, token_hash) = new_session_token();
    let expires_at = Utc::now() + ttl;
    sqlx::query(
        "INSERT INTO admin_sessions (token_hash, admin_id, expires_at)
         VALUES ($1, $2, $3)",
    )
    .bind(&token_hash)
    .bind(admin_id)
    .bind(expires_at)
    .execute(pool)
    .await?;

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "admin.login",
        Some(&admin_id.to_string()),
        json!({}),
    )
    .await;

    Ok(LoginResponse { token, expires_at })
}
