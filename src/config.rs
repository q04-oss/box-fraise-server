use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub cors_allowed_origins: Vec<String>,
    pub admin_session_ttl: chrono::Duration,
    pub challenge_ttl: chrono::Duration,
    /// The gap between meeting someone and being asked whether you
    /// still want to talk. Three days by default — long enough that
    /// the answer is given somewhere other than the room you met in.
    pub pairing_cooling: chrono::Duration,
    /// How long you then have to answer before the pairing lapses.
    pub pairing_window: chrono::Duration,
    pub seed_admin_email: Option<String>,
    pub seed_admin_password: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url =
            env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;
        // Prefer BIND_ADDR when explicitly set. Otherwise pick up the
        // PORT env var that Railway / Heroku / Fly / most PaaS inject,
        // and bind on 0.0.0.0 so the platform's router can reach us.
        let bind_addr = env::var("BIND_ADDR")
            .ok()
            .or_else(|| env::var("PORT").ok().map(|p| format!("0.0.0.0:{p}")))
            .unwrap_or_else(|| "0.0.0.0:3000".into());
        let cors_allowed_origins = env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:3000".into())
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        let admin_session_ttl =
            chrono::Duration::seconds(parse_env_or("ADMIN_SESSION_TTL_SECS", 43_200)?);
        let challenge_ttl = chrono::Duration::seconds(parse_env_or("CHALLENGE_TTL_SECS", 120)?);
        let pairing_cooling =
            chrono::Duration::seconds(parse_env_or("PAIRING_COOLING_SECS", 3 * 24 * 60 * 60)?);
        let pairing_window =
            chrono::Duration::seconds(parse_env_or("PAIRING_WINDOW_SECS", 7 * 24 * 60 * 60)?);
        let seed_admin_email = env::var("SEED_ADMIN_EMAIL").ok().filter(|s| !s.is_empty());
        let seed_admin_password = env::var("SEED_ADMIN_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty());
        Ok(Self {
            database_url,
            bind_addr,
            cors_allowed_origins,
            admin_session_ttl,
            challenge_ttl,
            pairing_cooling,
            pairing_window,
            seed_admin_email,
            seed_admin_password,
        })
    }
}

fn parse_env_or(key: &str, default: i64) -> anyhow::Result<i64> {
    match env::var(key) {
        Ok(v) => v
            .parse::<i64>()
            .map_err(|e| anyhow::anyhow!("{key} invalid: {e}")),
        Err(_) => Ok(default),
    }
}
