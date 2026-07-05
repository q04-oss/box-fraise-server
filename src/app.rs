use std::sync::Arc;

use axum::Router;
use sqlx::PgPool;

use crate::{config::Config, crypto, db};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cfg: Arc<Config>,
}

impl AppState {
    pub async fn init(cfg: Config) -> anyhow::Result<Self> {
        let pool = db::connect(&cfg.database_url).await?;
        Ok(Self {
            pool,
            cfg: Arc::new(cfg),
        })
    }

    /// Idempotent first-boot admin bootstrap. Skipped if either env
    /// var is missing. Inserts only when no admin with that email
    /// exists — safe to run on every boot. Email is normalised
    /// (trim + lowercase) to match the login lookup.
    pub async fn seed_admin_if_configured(&self) -> anyhow::Result<()> {
        let (Some(email), Some(password)) = (
            self.cfg.seed_admin_email.as_deref(),
            self.cfg.seed_admin_password.as_deref(),
        ) else {
            tracing::info!(
                "admin bootstrap skipped (SEED_ADMIN_EMAIL or SEED_ADMIN_PASSWORD unset)"
            );
            return Ok(());
        };
        let email = email.trim().to_lowercase();

        let mut tx = db::AdminRlsTransaction::begin(&self.pool).await?;
        let existing: Option<(uuid::Uuid,)> =
            sqlx::query_as("SELECT id FROM admins WHERE email = $1")
                .bind(&email)
                .fetch_optional(tx.conn())
                .await?;
        if existing.is_some() {
            tx.commit().await?;
            tracing::info!(%email, "admin bootstrap: already present");
            return Ok(());
        }
        let hash = crypto::argon2_hash(password)?;
        sqlx::query("INSERT INTO admins (email, password_hash) VALUES ($1, $2)")
            .bind(&email)
            .bind(hash)
            .execute(tx.conn())
            .await?;
        tx.commit().await?;
        tracing::info!(%email, "admin bootstrap: seeded");
        Ok(())
    }
}

pub fn build_router(state: AppState) -> Router {
    use axum::routing::get;
    use tower_http::services::ServeDir;
    use tower_http::trace::TraceLayer;

    let cors = build_cors(&state.cfg.cors_allowed_origins);

    // /v1 is the only versioned surface. /admin static page lives at
    // the root because it's a single-shot HTML the operator opens; it
    // has no API-versioning concern.
    let v1 = Router::new()
        .merge(crate::domain::admin::routes::router())
        .merge(crate::domain::onboarding::routes::router())
        .merge(crate::domain::events::routes::router())
        .merge(crate::domain::schedule::routes::router())
        .merge(crate::domain::search::routes::router())
        .merge(crate::domain::consultations::routes::router())
        .merge(crate::domain::oauth::routes::router())
        .merge(crate::celestial::routes::router())
        // Bearer-resolution runs on every /v1 request. It's a soft pass
        // — unrecognised tokens leave no marker; extractors enforce.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::http::middleware::resolve_bearer,
        ));

    // Static marketing site fallback. Order matters here — real routes
    // (/v1, /health, /admin) must be registered BEFORE the fallback,
    // otherwise ServeDir would try to 404 requests the router should be
    // handling. Axum's `.fallback_service` only fires when no route
    // matches, so the ordering below is safe by construction, but keep
    // the fallback last on the chain for clarity.
    Router::new()
        .nest("/v1", v1)
        .route("/health", get(health))
        .merge(crate::http::admin_assets::router())
        .fallback_service(ServeDir::new("web").append_index_html_on_directories(true))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Liveness endpoint. Kept dumb on purpose: returns 200 as long as the
/// process is up. If Railway's healthcheck needs to include DB
/// reachability, add a separate `/ready` that runs `SELECT 1` — a
/// hot-path readiness check on every heartbeat is not what we want.
async fn health() -> &'static str {
    "ok"
}

fn build_cors(origins: &[String]) -> tower_http::cors::CorsLayer {
    use axum::http::{header, Method};
    use tower_http::cors::{AllowOrigin, CorsLayer};
    let parsed: Vec<axum::http::HeaderValue> = origins
        .iter()
        .filter_map(|o| axum::http::HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(parsed))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
        .max_age(std::time::Duration::from_secs(3600))
}
