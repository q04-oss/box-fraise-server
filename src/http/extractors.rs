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

use crate::error::AppError;

#[derive(Clone, Copy, Debug)]
pub struct AuthedUser(pub Uuid);

#[derive(Clone, Copy, Debug)]
pub struct AuthedAdmin(pub Uuid);

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
