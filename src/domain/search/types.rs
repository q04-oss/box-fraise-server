use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub description: String,
}

// ── Brave upstream response (only the fields we surface) ────────────

#[derive(Debug, Deserialize)]
pub(super) struct BraveWebResponse {
    pub web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BraveWebResults {
    pub results: Vec<BraveWebResult>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BraveWebResult {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub description: String,
}
