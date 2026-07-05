use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    db::{Pool, RlsTransaction},
    domain::consultations::repository as consultations_repo,
    error::{AppError, AppResult},
};

use super::{
    jwt, keys,
    types::{Jwk, Jwks, TokenRequest, TokenResponse, UserInfo, WARNING},
};

const TOKEN_LIFETIME_SECS: i64 = 900; // 15 minutes
const ISSUER: &str = "https://fraise.box";

pub async fn issue_token(
    pool: &Pool,
    user_id: Uuid,
    req: TokenRequest,
) -> AppResult<TokenResponse> {
    let audience = req.audience.trim();
    if audience.is_empty() {
        return Err(AppError::bad_request("audience required"));
    }

    let (tier, verified) = user_tier(pool, user_id).await?;

    let now = Utc::now().timestamp();
    let exp = now + TOKEN_LIFETIME_SECS;

    let claims = json!({
        "iss":      ISSUER,
        "sub":      user_id.to_string(),
        "aud":      audience,
        "iat":      now,
        "exp":      exp,
        "tier":     tier,
        "verified": verified,
    });

    let token = jwt::sign(&claims, keys::signing_key(), &keys::key_id());

    Ok(TokenResponse {
        warning: WARNING,
        access_token: token,
        token_type: "Bearer",
        expires_in: TOKEN_LIFETIME_SECS,
        issued_for_audience: audience.to_string(),
    })
}

pub fn jwks() -> Jwks {
    let vk = keys::verifying_key();
    let ep = vk.to_encoded_point(false);
    let bytes = ep.as_bytes();
    // SEC1 uncompressed: 0x04 || X(32) || Y(32).
    let x = URL_SAFE_NO_PAD.encode(&bytes[1..33]);
    let y = URL_SAFE_NO_PAD.encode(&bytes[33..65]);

    Jwks {
        warning: WARNING,
        keys: vec![Jwk {
            kty: "EC",
            crv: "P-256",
            kid: keys::key_id(),
            use_: "sig",
            alg: "ES256",
            x,
            y,
        }],
    }
}

pub async fn userinfo(pool: &Pool, token: &str) -> AppResult<UserInfo> {
    let claims = jwt::verify(token, &keys::verifying_key()).ok_or(AppError::Unauthorized)?;

    let sub_str = claims
        .get("sub")
        .and_then(|v| v.as_str())
        .ok_or(AppError::Unauthorized)?;
    let sub = Uuid::parse_str(sub_str).map_err(|_| AppError::Unauthorized)?;

    let exp = claims
        .get("exp")
        .and_then(|v| v.as_i64())
        .ok_or(AppError::Unauthorized)?;
    if exp < Utc::now().timestamp() {
        return Err(AppError::Unauthorized);
    }

    let (tier, verified) = user_tier(pool, sub).await?;

    Ok(UserInfo {
        warning: WARNING,
        iss: ISSUER,
        sub,
        tier,
        verified,
    })
}

async fn user_tier(pool: &Pool, user_id: Uuid) -> AppResult<(u8, bool)> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let verification = consultations_repo::latest_verification_for(tx.conn(), user_id).await?;
    tx.commit().await?;
    let tier = if verification.is_some() { 2 } else { 1 };
    Ok((tier, verification.is_some()))
}
