// AuthedUser / AuthedAdmin extractors.
//
// The auth middleware (resolve_bearer) injects one of these into the
// request extensions when the Bearer token resolves. Handlers grab the
// extension via the extractor; a missing extension produces 401 — the
// extractor is the access-control gate, not the middleware.
//
// This split keeps public endpoints (POST /v1/onboard/register,
// POST /v1/admin/login, GET /v1/events) reachable without auth while
// every other route opts into auth by naming the extractor.

use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::{error::AppError, http::cookies};

#[derive(Clone, Copy, Debug)]
pub struct AuthedUser(pub Uuid);

#[derive(Clone, Copy, Debug)]
pub struct AuthedAdmin(pub Uuid);

/// Which session answered this request — the sha256 of the token, the
/// same value the middleware looked up by. Present alongside AuthedUser
/// and only there.
///
/// Exists so that signing out can name the session it means. A member
/// only ever holds one, so today that is the same set either way — but
/// "the token that asked" is the honest scope, and it stays correct if
/// that ever stops being true.
#[derive(Clone, Debug)]
pub struct SessionHash(pub String);

/// The raw token this request authenticated with, present alongside
/// AuthedUser and only there.
///
/// Exists for exactly one handler: the exchange that takes a credential
/// out of a QR code and puts it into an HttpOnly cookie. That handler
/// has to write the token into a Set-Cookie header, so it needs the
/// token and not its hash.
///
/// It is no more exposed here than it already was in the Authorization
/// header of the same request. It is never logged and never stored —
/// `user_sessions` holds only the hash, and that has not changed.
#[derive(Clone, Debug)]
pub struct SessionToken(pub String);

/// Whether the request reached the platform over TLS, so a cookie can
/// be marked `Secure` in production and not in local development, where
/// marking it would mean the browser silently discards it.
#[derive(Clone, Copy, Debug)]
pub struct SecureRequest(pub bool);

impl<S> FromRequestParts<S> for AuthedUser
where
    S: Send + Sync,
{
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthedUser>()
            .copied()
            .ok_or(AppError::Unauthorized)
    }
}

impl<S> FromRequestParts<S> for AuthedAdmin
where
    S: Send + Sync,
{
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthedAdmin>()
            .copied()
            .ok_or(AppError::Unauthorized)
    }
}

impl<S> FromRequestParts<S> for SessionHash
where
    S: Send + Sync,
{
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<SessionHash>()
            .cloned()
            .ok_or(AppError::Unauthorized)
    }
}

impl<S> FromRequestParts<S> for SessionToken
where
    S: Send + Sync,
{
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<SessionToken>()
            .cloned()
            .ok_or(AppError::Unauthorized)
    }
}

impl<S> FromRequestParts<S> for SecureRequest
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(SecureRequest(cookies::is_secure(&parts.headers)))
    }
}
