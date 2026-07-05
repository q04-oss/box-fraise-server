// Search domain — a thin, server-side proxy in front of the Brave
// Search API. No database, no repository: this is a pure integration.
//
// Design intent:
//   - The Brave API key never leaves the server. The marketing page's
//     search bar calls /v1/search on the same origin.
//   - Query text is NOT logged (nor written to audit_events). That
//     preserves the "private search" property the marketing page
//     promises. We log latency and status codes, nothing else.
//   - Brave's terms require attribution somewhere the user sees. The
//     marketing page renders "Search by Brave" under the results.

pub mod routes;
pub mod service;
pub mod types;
