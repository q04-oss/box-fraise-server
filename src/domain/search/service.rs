use std::time::Duration;

use crate::{
    config::Config,
    domain::search::types::*,
    error::{AppError, AppResult},
};

const BRAVE_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESULTS: usize = 10;

pub async fn search(cfg: &Config, query: &str) -> AppResult<SearchResponse> {
    let api_key = cfg.brave_search_api_key.as_deref().ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "search not configured (set BRAVE_SEARCH_API_KEY)"
        ))
    })?;

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent("box-fraise/0.1 (+https://fraise.box)")
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("reqwest client: {e}")))?;

    let response = client
        .get(BRAVE_URL)
        .header("X-Subscription-Token", api_key)
        .header("Accept", "application/json")
        .query(&[
            ("q", query),
            ("count", &MAX_RESULTS.to_string()),
            ("safesearch", "moderate"),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("brave request: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        // Body may contain the API key echoed back in error messages —
        // do NOT surface it to the caller. Log server-side only.
        let body_snippet = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(500)
            .collect::<String>();
        tracing::error!(
            status = %status,
            body = %body_snippet,
            "brave search upstream error"
        );
        return Err(AppError::Internal(anyhow::anyhow!(
            "upstream search error: {status}"
        )));
    }

    let brave: BraveWebResponse = response
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("brave decode: {e}")))?;

    let results = brave
        .web
        .map(|w| w.results)
        .unwrap_or_default()
        .into_iter()
        .map(|r| SearchResult {
            title: r.title,
            url: r.url,
            description: r.description,
        })
        .collect();

    Ok(SearchResponse {
        query: query.to_string(),
        results,
    })
}
