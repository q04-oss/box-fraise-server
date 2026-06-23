// Append-only audit trail.
//
// Audit writes always go to `&PgPool` (a fresh connection), never to a
// request transaction. If the request rolls back, the audit row stays —
// that's exactly the property we want for security-relevant events
// like "admin verified user X at event Y".
//
// Failures are logged and swallowed: auditing must never fail the
// user-facing operation. If audit storage is broken, the on-call
// engineer fixes it; the user does not see a 500.

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn write(
    pool: &PgPool,
    actor_type: &str,
    actor_id: Option<Uuid>,
    action: &str,
    target: Option<&str>,
    metadata: Value,
) {
    let result = sqlx::query(
        "INSERT INTO audit_events (actor_type, actor_id, action, target, metadata)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(actor_type)
    .bind(actor_id)
    .bind(action)
    .bind(target)
    .bind(metadata)
    .execute(pool)
    .await;
    if let Err(e) = result {
        tracing::error!(error = ?e, %action, "audit write failed");
    }
}
