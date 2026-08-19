// Integration tests. Each test is independent — random UUIDs for any
// seeded state — so cargo test's parallel execution is safe.
//
// Prerequisites:
//   - DATABASE_URL pointing at a Postgres with migrations applied,
//     connecting as the `bf_app` runtime role (NOT the owner — that
//     would bypass FORCE ROW LEVEL SECURITY and silently green-light
//     RLS-isolation tests).
//   - `docker compose up -d` + `sqlx migrate run` covers the local
//     case; CI does the same in the workflow file.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use box_fraise::{
    crypto::{argon2_hash, new_nonce, sha256_hex, verify_p256_signature},
    db::{self, AdminRlsTransaction, RlsTransaction},
    domain::{
        consultations::{
            service as consultations_service,
            types::{CompleteConsultationRequest, ReplaceCardRequest, RevokeCardRequest},
        },
        events::{service as events_service, types::CreateEventRequest},
        members::{
            service as members_service,
            types::{CreateMemberRequest, RecordAttendanceRequest, ReissueRequest},
        },
        messages::{service as messages_service, types::SendMessageRequest},
        onboarding::{
            service as onboarding_service,
            types::{RegisterRequest, VerifyRequest},
        },
        pairings::{
            service as pairings_service,
            types::{ClaimInPersonRequest, ClaimRequest, DecisionRequest, SetDisplayNameRequest},
        },
        submissions::{
            service as submissions_service,
            types::{Prompt, SubmissionUpload},
        },
    },
    error::AppError,
};
use chrono::{Duration as ChronoDuration, Utc};
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use sqlx::PgPool;
use uuid::Uuid;

const TEST_PASSWORD: &str = "test-pw-XYZ123!";

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://bf_app:bf_app@localhost:5432/box_fraise".into())
}

async fn test_pool() -> PgPool {
    db::connect(&database_url())
        .await
        .expect("connect test pool")
}

fn random_label() -> String {
    Uuid::new_v4().to_string()
}

async fn seed_test_admin(pool: &PgPool) -> Uuid {
    let email = format!("admin-{}@test.local", Uuid::new_v4());
    let hash = argon2_hash(TEST_PASSWORD).unwrap();
    let mut tx = AdminRlsTransaction::begin(pool).await.unwrap();
    let (id,): (Uuid,) =
        sqlx::query_as("INSERT INTO admins (email, password_hash) VALUES ($1, $2) RETURNING id")
            .bind(&email)
            .bind(&hash)
            .fetch_one(tx.conn())
            .await
            .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn seed_test_event(pool: &PgPool, admin_id: Uuid) -> Uuid {
    let now = Utc::now();
    events_service::create(
        pool,
        admin_id,
        CreateEventRequest {
            name: format!("Test Event {}", random_label()),
            host_name: "Test Host".into(),
            description: None,
            questions: vec![],
            poster_url: None,
            address: "123 Test St, Edmonton".into(),
            latitude: 53.5,
            longitude: -113.5,
            starts_at: now,
            ends_at: now + ChronoDuration::hours(4),
            published: true,
        },
    )
    .await
    .unwrap()
    .id
}

/// Delete a submission for real.
///
/// `submissions_admin_delete` requires app.is_admin, so a raw query on
/// the bare pool matches no rows and silently deletes nothing — which
/// is what several tests here used to do, leaving accepted rows behind
/// in the feed on every run.
async fn scrub_submission(pool: &PgPool, id: Uuid) {
    let mut tx = AdminRlsTransaction::begin(pool).await.unwrap();
    sqlx::query("DELETE FROM submissions WHERE id = $1")
        .bind(id)
        .execute(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// A member, made the only way there is: an admin signs somebody up
/// at a run. Returns the user id to post as.
async fn seed_test_member(pool: &PgPool) -> Uuid {
    let admin_id = seed_test_admin(pool).await;
    let event_id = seed_test_event(pool, admin_id).await;
    members_service::create(pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap()
        .user_id
}

fn fresh_keypair() -> (SigningKey, Vec<u8>) {
    let sk = SigningKey::random(&mut rand::rngs::OsRng);
    let vk = sk.verifying_key();
    let pk_sec1 = vk.to_encoded_point(false).as_bytes().to_vec();
    debug_assert_eq!(pk_sec1.len(), 65);
    (sk, pk_sec1)
}

fn sign_der(sk: &SigningKey, msg: &str) -> Vec<u8> {
    let sig: Signature = sk.sign(msg.as_bytes());
    sig.to_der().as_bytes().to_vec()
}

async fn register_with_keypair(pool: &PgPool) -> (Uuid, SigningKey) {
    let (sk, sec1) = fresh_keypair();
    let b64 = URL_SAFE_NO_PAD.encode(&sec1);
    let r = onboarding_service::register(
        pool,
        RegisterRequest {
            public_key: b64,
            key_id: "test-device".into(),
        },
    )
    .await
    .unwrap();
    (r.user_id, sk)
}

// ── Tests ────────────────────────────────────────────────────────────

/// (1) RlsTransaction sets app.user_id transaction-locally and does not
/// leak it across commits. This is the keystone invariant of the entire
/// RLS model — if it fails, every other guarantee is suspect.
#[tokio::test]
async fn rls_user_id_is_transaction_local() {
    let pool = test_pool().await;
    let user_id = Uuid::new_v4();

    let mut tx = RlsTransaction::begin(&pool, user_id).await.unwrap();
    let inside: Option<String> = sqlx::query_scalar("SELECT current_setting('app.user_id', true)")
        .fetch_one(tx.conn())
        .await
        .unwrap();
    assert_eq!(inside.as_deref(), Some(user_id.to_string().as_str()));
    tx.commit().await.unwrap();

    // After commit, a fresh acquire from the pool must not see the GUC.
    // Even if the pool returns the same connection, LOCAL semantics
    // ensure the value was discarded at COMMIT.
    let after: Option<String> = sqlx::query_scalar("SELECT current_setting('app.user_id', true)")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        after.as_deref().map(str::is_empty).unwrap_or(true),
        "app.user_id leaked across tx boundary: {after:?}"
    );
}

/// (2) bf_app is the runtime role and is subject to RLS. A user who
/// exists should be invisible under no context. This is the test that
/// would have caught the "owner role bypasses RLS" historical bug.
#[tokio::test]
async fn bf_app_no_context_yields_zero_user_rows() {
    let pool = test_pool().await;
    let (user_id, _sk) = register_with_keypair(&pool).await;

    let rows: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(rows.is_empty(), "RLS leaked under no context");
}

/// (3) audit_events is append-only at the DB level — both the missing
/// UPDATE/DELETE grant and the trigger should bite. Either failing is
/// fine; both failing is the property we want.
#[tokio::test]
async fn audit_events_is_append_only() {
    let pool = test_pool().await;
    let action = format!("test.append.only.{}", random_label());
    sqlx::query("INSERT INTO audit_events (actor_type, action) VALUES ('system', $1)")
        .bind(&action)
        .execute(&pool)
        .await
        .unwrap();

    let update_err = sqlx::query("UPDATE audit_events SET action='hacked' WHERE action=$1")
        .bind(&action)
        .execute(&pool)
        .await;
    assert!(update_err.is_err(), "audit_events UPDATE must fail");

    let delete_err = sqlx::query("DELETE FROM audit_events WHERE action=$1")
        .bind(&action)
        .execute(&pool)
        .await;
    assert!(delete_err.is_err(), "audit_events DELETE must fail");
}

/// (4) Happy path: register → challenge → verify flips the user to verified.
#[tokio::test]
async fn onboarding_happy_path_verifies_user() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;

    let (user_id, sk) = register_with_keypair(&pool).await;
    let chal = onboarding_service::issue_challenge(&pool, ChronoDuration::seconds(120), user_id)
        .await
        .unwrap();
    let sig_b64 = URL_SAFE_NO_PAD.encode(sign_der(&sk, &chal.nonce));

    let v = onboarding_service::verify(
        &pool,
        admin_id,
        VerifyRequest {
            nonce: chal.nonce,
            signature_b64: sig_b64,
            event_id,
        },
    )
    .await
    .unwrap();
    assert_eq!(v.user_id, user_id);
    assert_eq!(v.status, "verified");
    assert_eq!(v.verified_at_event_id, event_id);
}

/// (5) A challenge cannot be used twice. The second verify call
/// against the same nonce must return Conflict (HTTP 409).
#[tokio::test]
async fn challenge_replay_is_rejected() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let (user_id, sk) = register_with_keypair(&pool).await;

    let chal = onboarding_service::issue_challenge(&pool, ChronoDuration::seconds(120), user_id)
        .await
        .unwrap();
    let sig = URL_SAFE_NO_PAD.encode(sign_der(&sk, &chal.nonce));

    onboarding_service::verify(
        &pool,
        admin_id,
        VerifyRequest {
            nonce: chal.nonce.clone(),
            signature_b64: sig.clone(),
            event_id,
        },
    )
    .await
    .unwrap();

    let replay = onboarding_service::verify(
        &pool,
        admin_id,
        VerifyRequest {
            nonce: chal.nonce,
            signature_b64: sig,
            event_id,
        },
    )
    .await;
    assert!(
        matches!(replay, Err(AppError::Conflict)),
        "replay should 409: {replay:?}"
    );
}

/// (6) An expired challenge cannot be redeemed.
#[tokio::test]
async fn expired_challenge_is_rejected() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let (user_id, sk) = register_with_keypair(&pool).await;

    // Seed an already-expired challenge directly.
    let nonce = new_nonce();
    let expires_at = Utc::now() - ChronoDuration::seconds(10);
    let mut tx = RlsTransaction::begin(&pool, user_id).await.unwrap();
    sqlx::query("INSERT INTO challenges (nonce, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(&nonce)
        .bind(user_id)
        .bind(expires_at)
        .execute(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let sig = URL_SAFE_NO_PAD.encode(sign_der(&sk, &nonce));
    let r = onboarding_service::verify(
        &pool,
        admin_id,
        VerifyRequest {
            nonce,
            signature_b64: sig,
            event_id,
        },
    )
    .await;
    assert!(
        matches!(r, Err(AppError::BadRequest(_))),
        "expired should 400: {r:?}"
    );
}

/// (7) A signature that does not verify against the user's device key
/// must be rejected with the dedicated InvalidSignature variant (401).
#[tokio::test]
async fn tampered_signature_is_rejected() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let (user_id, sk) = register_with_keypair(&pool).await;

    let chal = onboarding_service::issue_challenge(&pool, ChronoDuration::seconds(120), user_id)
        .await
        .unwrap();
    let mut sig_bytes = sign_der(&sk, &chal.nonce);
    // Flip a payload byte — DER may still decode, but verify must fail.
    let last = sig_bytes.len() - 1;
    sig_bytes[last] ^= 0x01;

    let r = onboarding_service::verify(
        &pool,
        admin_id,
        VerifyRequest {
            nonce: chal.nonce,
            signature_b64: URL_SAFE_NO_PAD.encode(sig_bytes),
            event_id,
        },
    )
    .await;
    assert!(
        matches!(r, Err(AppError::InvalidSignature)),
        "tampered sig should reject: {r:?}"
    );
}

/// (8) Cross-user isolation: user A's RlsTransaction cannot read user
/// B's row. This is the exact bug pattern that prompted the FORCE ROW
/// LEVEL SECURITY + transaction-local GUC discipline.
#[tokio::test]
async fn user_a_cannot_read_user_b_under_rls() {
    let pool = test_pool().await;
    let (user_a, _) = register_with_keypair(&pool).await;
    let (user_b, _) = register_with_keypair(&pool).await;
    assert_ne!(user_a, user_b);

    // Sanity: user A sees their own row.
    let me_a = onboarding_service::me(&pool, user_a).await.unwrap();
    assert_eq!(me_a.id, user_a);

    // The isolation property: user A's context cannot read user B.
    let mut tx = RlsTransaction::begin(&pool, user_a).await.unwrap();
    let rows: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE id = $1")
        .bind(user_b)
        .fetch_all(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(rows.is_empty(), "user A read user B's row under RLS");
}

/// (9) Non-admin context cannot insert an event. The events_admin_insert
/// WITH CHECK requires app.is_admin = 'true' — a user-scoped tx does
/// not satisfy it.
#[tokio::test]
async fn non_admin_cannot_insert_event() {
    let pool = test_pool().await;
    let (user_id, _) = register_with_keypair(&pool).await;
    let admin_id = seed_test_admin(&pool).await; // a real admin to FK against
    let now = Utc::now();

    let mut tx = RlsTransaction::begin(&pool, user_id).await.unwrap();
    let insert = sqlx::query(
        "INSERT INTO events
            (name, host_name, latitude, longitude, address, starts_at, ends_at,
             published, created_by_admin_id)
         VALUES ('x','y',0,0,'z',$1,$2,true,$3)",
    )
    .bind(now)
    .bind(now + ChronoDuration::hours(1))
    .bind(admin_id)
    .execute(tx.conn())
    .await;
    assert!(
        insert.is_err(),
        "non-admin event INSERT must be denied by RLS"
    );
}

/// (10) Atomic flip — two concurrent verify calls for the same user
/// must result in exactly one success. The other races on the
/// UPDATE ... WHERE status='pending' guard and returns Conflict.
#[tokio::test]
async fn concurrent_verify_only_one_succeeds() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let (user_id, sk) = register_with_keypair(&pool).await;

    let chal_a = onboarding_service::issue_challenge(&pool, ChronoDuration::seconds(120), user_id)
        .await
        .unwrap();
    let chal_b = onboarding_service::issue_challenge(&pool, ChronoDuration::seconds(120), user_id)
        .await
        .unwrap();
    let sig_a = URL_SAFE_NO_PAD.encode(sign_der(&sk, &chal_a.nonce));
    let sig_b = URL_SAFE_NO_PAD.encode(sign_der(&sk, &chal_b.nonce));

    let pool_a = pool.clone();
    let pool_b = pool.clone();
    let t_a = tokio::spawn(async move {
        onboarding_service::verify(
            &pool_a,
            admin_id,
            VerifyRequest {
                nonce: chal_a.nonce,
                signature_b64: sig_a,
                event_id,
            },
        )
        .await
    });
    let t_b = tokio::spawn(async move {
        onboarding_service::verify(
            &pool_b,
            admin_id,
            VerifyRequest {
                nonce: chal_b.nonce,
                signature_b64: sig_b,
                event_id,
            },
        )
        .await
    });
    let (ra, rb) = tokio::join!(t_a, t_b);
    let ra = ra.unwrap();
    let rb = rb.unwrap();

    let oks = [&ra, &rb].iter().filter(|r| r.is_ok()).count();
    let conflicts = [&ra, &rb]
        .iter()
        .filter(|r| matches!(r, Err(AppError::Conflict)))
        .count();
    assert_eq!(oks, 1, "exactly one verify must succeed: {ra:?} / {rb:?}");
    assert_eq!(conflicts, 1, "the loser must be Conflict: {ra:?} / {rb:?}");
}

/// (11) Crypto round-trip: a valid (pk, msg, sig) triple verifies.
/// Sanity check on verify_p256_signature itself.
#[tokio::test]
async fn verify_round_trips_in_process() {
    let (sk, pk_sec1) = fresh_keypair();
    let msg = "round-trip-test-message";
    let sig: Signature = sk.sign(msg.as_bytes());
    let der = sig.to_der().as_bytes().to_vec();
    verify_p256_signature(&pk_sec1, msg, &der).expect("round trip must verify");
}

/// (12) iOS interop fixture — placeholder. The intent: drop in a
/// (pk_sec1, nonce, sig_der) triple captured from a real iPhone via
/// `SecKeyCreateSignature(.., .ecdsaSignatureMessageX962SHA256, ..)`
/// and confirm verify_p256_signature accepts it. While that fixture is
/// not present, the test is in-process and #[ignore]d so it does not
/// run by default. Swap the body and remove #[ignore] when a real
/// capture is available.
/// (13) /v1/me embeds the verified event after a successful verify, so
/// the iOS client gets `{name, host_name, starts_at, address}` in one
/// round-trip instead of having to follow up with /v1/events/{id}.
#[tokio::test]
async fn me_embeds_verified_event_after_verify() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let (user_id, sk) = register_with_keypair(&pool).await;

    // Pre-verify: status pending, event embedded as None.
    let pre = onboarding_service::me(&pool, user_id).await.unwrap();
    assert_eq!(pre.status, "pending");
    assert!(
        pre.event.is_none(),
        "pending user should have no embedded event"
    );

    let chal = onboarding_service::issue_challenge(&pool, ChronoDuration::seconds(120), user_id)
        .await
        .unwrap();
    let sig_b64 = URL_SAFE_NO_PAD.encode(sign_der(&sk, &chal.nonce));
    onboarding_service::verify(
        &pool,
        admin_id,
        VerifyRequest {
            nonce: chal.nonce,
            signature_b64: sig_b64,
            event_id,
        },
    )
    .await
    .unwrap();

    let post = onboarding_service::me(&pool, user_id).await.unwrap();
    assert_eq!(post.status, "verified");
    let event = post
        .event
        .expect("verified user should have embedded event");
    assert_eq!(event.id, event_id);
    assert!(!event.name.is_empty());
    assert!(!event.host_name.is_empty());
    assert!(!event.address.is_empty());
}

/// (17) Consultation lifecycle: a trained consultant completes a
/// consultation → the verification + card are issued atomically → the
/// public card lookup returns valid → revoke → lookup returns dead.
#[tokio::test]
async fn consultation_lifecycle_end_to_end() {
    let pool = test_pool().await;

    // Set up: a consultant (a user, promoted to staff with training).
    let (consultant_user_id, _) = register_with_keypair(&pool).await;
    seed_trained_consultant(&pool, consultant_user_id).await;

    // A user who will be verified.
    let (verified_user_id, _) = register_with_keypair(&pool).await;

    // Complete the consultation.
    let result = consultations_service::complete_consultation(
        &pool,
        consultant_user_id,
        CompleteConsultationRequest {
            user_id: verified_user_id,
            salon_id: None,
            consultation_notes: Some(
                "Careful conversation, comfortable with public profile.".into(),
            ),
            consent_snapshot: serde_json::json!({
                "advertising": true,
                "social_feed": true,
                "revenue_share": true,
            }),
            design_version: "v1".into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.verification.user_id, verified_user_id);
    assert_eq!(result.verification.consulted_by_user_id, consultant_user_id);
    assert_eq!(result.card.user_id, verified_user_id);
    assert_eq!(result.card.status, "active");
    assert_eq!(
        result.card.serial.len(),
        24,
        "serial should be 20 hex + 4 hyphens"
    );

    // Public lookup by serial.
    let lookup = consultations_service::lookup_card(&pool, &result.card.serial)
        .await
        .unwrap();
    assert!(lookup.is_valid);
    assert_eq!(lookup.status, "active");

    // Lookup with lowercase + no hyphens should canonicalise and still hit.
    let messy = result.card.serial.replace('-', "").to_lowercase();
    let lookup2 = consultations_service::lookup_card(&pool, &messy)
        .await
        .unwrap();
    assert!(lookup2.is_valid);

    // Revoke.
    consultations_service::revoke_card(
        &pool,
        consultant_user_id,
        result.card.id,
        RevokeCardRequest {
            reason: "test revoke".into(),
        },
    )
    .await
    .unwrap();

    let after = consultations_service::lookup_card(&pool, &result.card.serial)
        .await
        .unwrap();
    assert!(!after.is_valid);
    assert_eq!(after.status, "revoked");
}

/// (18) A consultant cannot self-verify.
#[tokio::test]
async fn consultant_cannot_verify_themselves() {
    let pool = test_pool().await;
    let (consultant_user_id, _) = register_with_keypair(&pool).await;
    seed_trained_consultant(&pool, consultant_user_id).await;

    let r = consultations_service::complete_consultation(
        &pool,
        consultant_user_id,
        CompleteConsultationRequest {
            user_id: consultant_user_id,
            salon_id: None,
            consultation_notes: None,
            consent_snapshot: serde_json::Value::Null,
            design_version: "v1".into(),
        },
    )
    .await;
    assert!(matches!(r, Err(AppError::BadRequest(_))));
}

/// (19) An untrained staff member cannot complete consultations.
#[tokio::test]
async fn untrained_staff_cannot_consult() {
    let pool = test_pool().await;
    let (untrained_id, _) = register_with_keypair(&pool).await;
    // Insert a staff row with NO consultation_training_completed_at.
    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO staff (user_id, role, can_verify_others)
         VALUES ($1, 'stylist', true)",
    )
    .bind(untrained_id)
    .execute(tx.conn())
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let (target_id, _) = register_with_keypair(&pool).await;
    let r = consultations_service::complete_consultation(
        &pool,
        untrained_id,
        CompleteConsultationRequest {
            user_id: target_id,
            salon_id: None,
            consultation_notes: None,
            consent_snapshot: serde_json::Value::Null,
            design_version: "v1".into(),
        },
    )
    .await;
    assert!(
        matches!(r, Err(AppError::Forbidden)),
        "expected Forbidden, got {r:?}"
    );
}

/// (20) Replace an active card → new card is active, old card marks
/// as replaced with pointer to the new id.
#[tokio::test]
async fn card_replacement_flow() {
    let pool = test_pool().await;
    let (consultant_id, _) = register_with_keypair(&pool).await;
    seed_trained_consultant(&pool, consultant_id).await;
    let (user_id, _) = register_with_keypair(&pool).await;

    let first = consultations_service::complete_consultation(
        &pool,
        consultant_id,
        CompleteConsultationRequest {
            user_id,
            salon_id: None,
            consultation_notes: None,
            consent_snapshot: serde_json::Value::Null,
            design_version: "v1".into(),
        },
    )
    .await
    .unwrap();

    let replacement = consultations_service::replace_card(
        &pool,
        consultant_id,
        first.card.id,
        ReplaceCardRequest {
            design_version: None,
        },
    )
    .await
    .unwrap();

    assert_ne!(replacement.serial, first.card.serial);
    assert_eq!(replacement.status, "active");

    // Old card should now be 'replaced'.
    let old = consultations_service::lookup_card(&pool, &first.card.serial)
        .await
        .unwrap();
    assert_eq!(old.status, "replaced");
    assert!(!old.is_valid);

    // New card is valid.
    let new = consultations_service::lookup_card(&pool, &replacement.serial)
        .await
        .unwrap();
    assert!(new.is_valid);
}

/// Helper: promote a user to a trained stylist consultant.
async fn seed_trained_consultant(pool: &PgPool, user_id: Uuid) {
    let mut tx = AdminRlsTransaction::begin(pool).await.unwrap();
    sqlx::query(
        "INSERT INTO staff (user_id, role, can_verify_others,
                             consultation_training_completed_at)
         VALUES ($1, 'stylist', true, now())",
    )
    .bind(user_id)
    .execute(tx.conn())
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// Events created with a questions[] round-trip through EventSummary
/// with the same ordered list. Guards against a silent drop or reorder
/// if either the INSERT RETURNING or the FromRow contract for the
/// column ever regresses.
#[tokio::test]
async fn event_questions_round_trip() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let now = Utc::now();
    let qs: Vec<String> = vec![
        "Is justice real?".into(),
        "Should Alberta separate from Canada?".into(),
        "Should the Water Not Coal question be on the referendum?".into(),
    ];

    let ev = events_service::create(
        &pool,
        admin_id,
        CreateEventRequest {
            name: format!("Q-test {}", random_label()),
            host_name: "Host".into(),
            description: None,
            questions: qs.clone(),
            poster_url: None,
            address: "10026 102 Street NW".into(),
            latitude: 53.5423,
            longitude: -113.4917,
            starts_at: now,
            ends_at: now + ChronoDuration::hours(2),
            published: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(ev.questions, qs, "questions must round-trip in order");

    // And they survive the list route too.
    let listed = events_service::list_public(&pool).await.unwrap();
    let mine = listed
        .into_iter()
        .find(|e| e.id == ev.id)
        .expect("event visible on public list");
    assert_eq!(mine.questions, qs);
}

/// The /v1/questions archive returns only published events with a
/// non-empty questions[] and never the ones without questions.
#[tokio::test]
async fn questions_archive_filters_and_lists() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let now = Utc::now();

    let with_qs = events_service::create(
        &pool,
        admin_id,
        CreateEventRequest {
            name: format!("QA with {}", random_label()),
            host_name: "Host".into(),
            description: None,
            questions: vec!["Only question".into()],
            poster_url: None,
            address: "10026 102 Street NW".into(),
            latitude: 53.5423,
            longitude: -113.4917,
            starts_at: now,
            ends_at: now + ChronoDuration::hours(1),
            published: true,
        },
    )
    .await
    .unwrap();

    let without_qs = events_service::create(
        &pool,
        admin_id,
        CreateEventRequest {
            name: format!("QA without {}", random_label()),
            host_name: "Host".into(),
            description: None,
            questions: vec![],
            poster_url: None,
            address: "10026 102 Street NW".into(),
            latitude: 53.5423,
            longitude: -113.4917,
            starts_at: now,
            ends_at: now + ChronoDuration::hours(1),
            published: true,
        },
    )
    .await
    .unwrap();

    let archive = events_service::list_all_questions(&pool).await.unwrap();
    assert!(
        archive.iter().any(|e| e.event_id == with_qs.id),
        "event with questions should appear in the archive"
    );
    assert!(
        archive.iter().all(|e| e.event_id != without_qs.id),
        "event without questions should not appear in the archive"
    );
}

// ── Pairings ────────────────────────────────────────────────────────
//
// The property under test throughout: a declined pairing and an
// ignored one must be indistinguishable to the other party. Most of
// these exist to prove that one thing from different angles.

/// One second, so the waiting period elapses inside a test; and a
/// generous window so nothing lapses mid-assertion.
///
/// Passed as arguments rather than set in the environment. Env vars
/// are process-global and this suite runs in parallel, so a test that
/// mutated them would silently corrupt whichever other test happened
/// to be mid-flight.
const SHORT_COOLING: ChronoDuration = ChronoDuration::seconds(1);
const LONG_WINDOW: ChronoDuration = ChronoDuration::seconds(300);

/// Register two users and walk them through a scan, with the cooling
/// period turned down so the test does not take three days.
///
/// Returns (initiator, scanner, pairing_id).
async fn seed_pair(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let (initiator, _sk_a) = register_with_keypair(pool).await;
    let (scanner, sk_b) = register_with_keypair(pool).await;

    let nonce = pairings_service::issue_nonce(pool, initiator)
        .await
        .unwrap();
    let sig = URL_SAFE_NO_PAD.encode(sign_der(&sk_b, &nonce.nonce));
    let claim = pairings_service::claim(
        pool,
        scanner,
        SHORT_COOLING,
        LONG_WINDOW,
        ClaimRequest {
            nonce: nonce.nonce,
            signature_b64: sig,
        },
    )
    .await
    .unwrap();

    (initiator, scanner, claim.pairing_id)
}

async fn wait_for_window() {
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
}

#[tokio::test]
async fn claim_requires_a_valid_signature_from_the_scanner() {
    let pool = test_pool().await;
    let (initiator, _sk_a) = register_with_keypair(&pool).await;
    let (scanner, _sk_b) = register_with_keypair(&pool).await;

    let nonce = pairings_service::issue_nonce(&pool, initiator)
        .await
        .unwrap();

    // A signature from a key that is not the scanner's.
    let (stranger_sk, _pk) = fresh_keypair();
    let bad = URL_SAFE_NO_PAD.encode(sign_der(&stranger_sk, &nonce.nonce));
    assert!(
        matches!(
            pairings_service::claim(
                &pool,
                scanner,
                SHORT_COOLING,
                LONG_WINDOW,
                ClaimRequest {
                    nonce: nonce.nonce.clone(),
                    signature_b64: bad
                }
            )
            .await,
            Err(AppError::InvalidSignature)
        ),
        "a scan must be signed by the scanner's own device key"
    );
}

#[tokio::test]
async fn a_nonce_cannot_be_claimed_twice() {
    let pool = test_pool().await;
    let (_a, _b, _id) = seed_pair(&pool).await;
    // seed_pair burned the nonce; a second claim of the same nonce is
    // covered by the burn race-close. Here: a fresh third party trying
    // the same code.
    let (initiator, _sk) = register_with_keypair(&pool).await;
    let nonce = pairings_service::issue_nonce(&pool, initiator)
        .await
        .unwrap();

    let (first, sk_first) = register_with_keypair(&pool).await;
    let (second, sk_second) = register_with_keypair(&pool).await;

    pairings_service::claim(
        &pool,
        first,
        SHORT_COOLING,
        LONG_WINDOW,
        ClaimRequest {
            nonce: nonce.nonce.clone(),
            signature_b64: URL_SAFE_NO_PAD.encode(sign_der(&sk_first, &nonce.nonce)),
        },
    )
    .await
    .expect("first claim should win");

    assert!(
        matches!(
            pairings_service::claim(
                &pool,
                second,
                SHORT_COOLING,
                LONG_WINDOW,
                ClaimRequest {
                    nonce: nonce.nonce.clone(),
                    signature_b64: URL_SAFE_NO_PAD.encode(sign_der(&sk_second, &nonce.nonce)),
                }
            )
            .await,
            Err(AppError::Conflict)
        ),
        "a burned nonce must not be reusable"
    );
}

#[tokio::test]
async fn cannot_decide_before_the_waiting_period_is_over() {
    let pool = test_pool().await;

    let (initiator, _sk_a) = register_with_keypair(&pool).await;
    let (scanner, sk_b) = register_with_keypair(&pool).await;
    let nonce = pairings_service::issue_nonce(&pool, initiator)
        .await
        .unwrap();
    let claim = pairings_service::claim(
        &pool,
        scanner,
        // Long enough that the pairing is still in `waiting` when the
        // assertion below runs.
        ChronoDuration::seconds(600),
        LONG_WINDOW,
        ClaimRequest {
            nonce: nonce.nonce.clone(),
            signature_b64: URL_SAFE_NO_PAD.encode(sign_der(&sk_b, &nonce.nonce)),
        },
    )
    .await
    .unwrap();

    // This is the whole point of the design: no answer is possible at
    // the event, so nobody can be pressured into giving one.
    assert!(
        matches!(
            pairings_service::decide(
                &pool,
                scanner,
                claim.pairing_id,
                DecisionRequest {
                    decision: "yes".into()
                }
            )
            .await,
            Err(AppError::BadRequest(_))
        ),
        "deciding before opens_at must be refused"
    );
}

#[tokio::test]
async fn mutual_yes_opens_the_channel() {
    let pool = test_pool().await;
    let (initiator, scanner, id) = seed_pair(&pool).await;
    wait_for_window().await;

    let after_first = pairings_service::decide(
        &pool,
        scanner,
        id,
        DecisionRequest {
            decision: "yes".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(after_first.status, "deciding", "one yes is not enough");
    assert!(
        !pairings_service::authorized(&pool, scanner, initiator)
            .await
            .unwrap()
            .authorized,
        "chat must not be authorized on one confirmation"
    );

    let after_second = pairings_service::decide(
        &pool,
        initiator,
        id,
        DecisionRequest {
            decision: "yes".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(after_second.status, "open");

    assert!(
        pairings_service::authorized(&pool, scanner, initiator)
            .await
            .unwrap()
            .authorized
    );
    assert!(
        pairings_service::authorized(&pool, initiator, scanner)
            .await
            .unwrap()
            .authorized
    );
}

/// **The load-bearing test.** After one side declines, the other side
/// must see exactly what they would have seen had that person simply
/// never answered.
#[tokio::test]
async fn a_decline_is_indistinguishable_from_silence() {
    let pool = test_pool().await;

    // Pair one: the scanner declines.
    let (init_a, scan_a, id_a) = seed_pair(&pool).await;
    // Pair two: the scanner never answers.
    let (init_b, _scan_b, id_b) = seed_pair(&pool).await;
    wait_for_window().await;

    pairings_service::decide(
        &pool,
        scan_a,
        id_a,
        DecisionRequest {
            decision: "no".into(),
        },
    )
    .await
    .unwrap();

    let declined = pairings_service::list(&pool, init_a)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.id == id_a)
        .expect("the declined pairing must still be listed");
    let ignored = pairings_service::list(&pool, init_b)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.id == id_b)
        .expect("the ignored pairing must still be listed");

    // Same status, same absence of any signal.
    assert_eq!(declined.status, ignored.status);
    assert_eq!(declined.status, "deciding");
    assert_eq!(declined.my_decision, None);
    assert_eq!(ignored.my_decision, None);
    assert_eq!(declined.peer_id, None, "no peer id before opening");

    // And the row is still there — an early delete would let the other
    // side watch it vanish ahead of schedule, which is a side channel.
    assert!(
        !pairings_service::authorized(&pool, init_a, scan_a)
            .await
            .unwrap()
            .authorized,
        "a declined pairing must never authorize chat"
    );

    // Serialized, the two are byte-identical apart from ids and times.
    let d = serde_json::to_value(&declined).unwrap();
    let i = serde_json::to_value(&ignored).unwrap();
    assert_eq!(d["status"], i["status"]);
    assert_eq!(d["my_decision"], i["my_decision"]);
    assert_eq!(d["peer_id"], i["peer_id"]);
    assert!(
        d.get("their_decision").is_none() && d.get("peer_decision").is_none(),
        "the view must have no field for the other side's answer"
    );
}

#[tokio::test]
async fn one_no_never_opens_even_with_a_yes() {
    let pool = test_pool().await;
    let (initiator, scanner, id) = seed_pair(&pool).await;
    wait_for_window().await;

    pairings_service::decide(
        &pool,
        scanner,
        id,
        DecisionRequest {
            decision: "yes".into(),
        },
    )
    .await
    .unwrap();
    let after = pairings_service::decide(
        &pool,
        initiator,
        id,
        DecisionRequest {
            decision: "no".into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(after.status, "deciding", "a no must not open the channel");
    assert!(
        !pairings_service::authorized(&pool, initiator, scanner)
            .await
            .unwrap()
            .authorized
    );
}

#[tokio::test]
async fn blocking_closes_the_channel_for_both() {
    let pool = test_pool().await;
    let (initiator, scanner, id) = seed_pair(&pool).await;
    wait_for_window().await;

    pairings_service::decide(
        &pool,
        scanner,
        id,
        DecisionRequest {
            decision: "yes".into(),
        },
    )
    .await
    .unwrap();
    pairings_service::decide(
        &pool,
        initiator,
        id,
        DecisionRequest {
            decision: "yes".into(),
        },
    )
    .await
    .unwrap();
    assert!(
        pairings_service::authorized(&pool, initiator, scanner)
            .await
            .unwrap()
            .authorized
    );

    pairings_service::block(&pool, initiator, id).await.unwrap();

    assert!(
        !pairings_service::authorized(&pool, initiator, scanner)
            .await
            .unwrap()
            .authorized,
        "blocking must revoke authorization for the blocker"
    );
    assert!(
        !pairings_service::authorized(&pool, scanner, initiator)
            .await
            .unwrap()
            .authorized,
        "and for the other side too"
    );
    // Terminal: a second block is a conflict, not a silent no-op.
    assert!(matches!(
        pairings_service::block(&pool, scanner, id).await,
        Err(AppError::Conflict)
    ));
}

#[tokio::test]
async fn the_same_two_people_cannot_pair_twice() {
    let pool = test_pool().await;
    let (initiator, scanner, _id) = seed_pair(&pool).await;

    // Scan again — the unique index on the ordered pair should refuse.
    let nonce = pairings_service::issue_nonce(&pool, initiator)
        .await
        .unwrap();
    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    let (_pk,): (Vec<u8>,) =
        sqlx::query_as("SELECT public_key FROM device_keys WHERE user_id = $1")
            .bind(scanner)
            .fetch_one(tx.conn())
            .await
            .unwrap();
    tx.commit().await.unwrap();

    // Signature will be wrong (we don't hold the scanner's key here),
    // so assert on the earlier failure mode instead: a nonce claimed
    // by an already-paired user. Re-register the pair relationship via
    // a direct insert to isolate the unique index.
    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    let (lower, upper) = if initiator < scanner {
        (initiator, scanner)
    } else {
        (scanner, initiator)
    };
    let dup = sqlx::query(
        "INSERT INTO pairings (lower_user_id, upper_user_id, opens_at, expires_at)
         VALUES ($1, $2, now() + interval '1 hour', now() + interval '2 hours')",
    )
    .bind(lower)
    .bind(upper)
    .execute(tx.conn())
    .await;
    tx.rollback().await.ok();

    assert!(
        dup.is_err(),
        "a second pairing between the same two must be refused"
    );
    drop(nonce);
}

#[tokio::test]
async fn display_name_is_what_the_other_person_sees() {
    let pool = test_pool().await;
    let (initiator, scanner, _id) = seed_pair(&pool).await;

    pairings_service::set_display_name(
        &pool,
        scanner,
        SetDisplayNameRequest {
            display_name: "  Sam  ".into(),
        },
    )
    .await
    .unwrap();

    let seen = pairings_service::list(&pool, initiator).await.unwrap();
    let entry = seen.first().expect("initiator should see the pairing");
    assert_eq!(
        entry.peer_name.as_deref(),
        Some("Sam"),
        "name is trimmed and shown"
    );

    assert!(matches!(
        pairings_service::set_display_name(
            &pool,
            scanner,
            SetDisplayNameRequest {
                display_name: "x".repeat(41)
            }
        )
        .await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn a_third_party_sees_nothing_of_someone_elses_pairing() {
    let pool = test_pool().await;
    let (initiator, scanner, _id) = seed_pair(&pool).await;
    let (stranger, _sk) = register_with_keypair(&pool).await;

    let theirs = pairings_service::list(&pool, stranger).await.unwrap();
    assert!(theirs.is_empty(), "a stranger must see no pairings");
    assert!(
        !pairings_service::authorized(&pool, stranger, initiator)
            .await
            .unwrap()
            .authorized,
        "a stranger must not be authorized to message a paired user"
    );
    assert!(
        !pairings_service::authorized(&pool, stranger, scanner)
            .await
            .unwrap()
            .authorized
    );
}

#[tokio::test]
#[ignore = "real iOS-emitted fixture goes here"]
async fn ios_signature_fixture_round_trips() {
    let (sk, pk_sec1) = fresh_keypair();
    let msg = "ios-fixture-message-bytes";
    let sig: Signature = sk.sign(msg.as_bytes());
    let der = sig.to_der().as_bytes().to_vec();
    verify_p256_signature(&pk_sec1, msg, &der).expect("ios fixture must verify");
}

// ── Submissions ─────────────────────────────────────────────────────

/// A 1x1 JPEG. Real magic bytes, so it passes the sniffer.
fn tiny_jpeg() -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0];
    // Padded past MIN_IMAGE_BYTES; the sniffer only reads the head.
    v.extend(std::iter::repeat_n(0x20, 1024));
    v
}

fn column(text: &str) -> SubmissionUpload {
    SubmissionUpload {
        prompt: Prompt::RUN_COUNTRY.into(),
        title: None,
        body: Some(text.into()),
        image_bytes: None,
    }
}

/// The public write lands, and lands as 'pending' — never as anything
/// the sender chose. `status` is hardcoded in the repository and the
/// RLS WITH CHECK enforces it independently.
#[tokio::test]
async fn submission_is_accepted_as_pending() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;
    let r = submissions_service::submit(
        &pool,
        member,
        column("A column about the railyard, long enough to count as one."),
    )
    .await
    .unwrap();
    assert_eq!(r.status, "pending");

    let admin_id = seed_test_admin(&pool).await;
    let queue = submissions_service::list_pending(&pool).await.unwrap();
    assert!(queue.iter().any(|s| s.id == r.id));
    submissions_service::reject(&pool, admin_id, r.id)
        .await
        .unwrap();
}

/// A pending submission is invisible to every non-admin read path.
/// Unlike the old sightings table there is no 'approved' state that
/// opens it up either — `submissions` has no non-admin SELECT policy at
/// all, so a plain transaction sees nothing in any state.
#[tokio::test]
async fn submissions_are_invisible_without_admin() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;
    let r = submissions_service::submit(
        &pool,
        member,
        column("Another column, once again long enough to be a column."),
    )
    .await
    .unwrap();

    // No GUCs set: exactly what an unauthenticated request looks like.
    let mut tx = pool.begin().await.unwrap();
    let seen: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM submissions WHERE id = $1")
        .bind(r.id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(seen, 0, "a submission must not be readable without admin");

    let admin_id = seed_test_admin(&pool).await;
    submissions_service::reject(&pool, admin_id, r.id)
        .await
        .unwrap();
}

/// Nothing at all is not a submission, and a title is not a column.
#[tokio::test]
async fn empty_submission_is_refused() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;
    let empty = SubmissionUpload {
        prompt: Prompt::RUN_COUNTRY.into(),
        title: None,
        body: None,
        image_bytes: None,
    };
    assert!(matches!(
        submissions_service::submit(&pool, member, empty).await,
        Err(AppError::BadRequest(_))
    ));

    // Too short to be a column.
    assert!(matches!(
        submissions_service::submit(&pool, member, column("hi")).await,
        Err(AppError::BadRequest(_))
    ));
}

/// The content type is decided by the bytes, never by what the caller
/// claims. HTML dressed as a JPEG is the stored-XSS-via-upload shape.
#[tokio::test]
async fn non_image_bytes_are_refused() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;
    let html = SubmissionUpload {
        prompt: Prompt::RUN_COUNTRY.into(),
        title: None,
        body: None,
        image_bytes: Some(
            b"<html><script>alert(1)</script></html>"
                .repeat(40)
                .to_vec(),
        ),
    };
    assert!(matches!(
        submissions_service::submit(&pool, member, html).await,
        Err(AppError::BadRequest(_))
    ));
}

/// A photograph with no writing is a whole submission.
#[tokio::test]
async fn photograph_alone_is_a_submission() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;
    let r = submissions_service::submit(
        &pool,
        member,
        SubmissionUpload {
        prompt: Prompt::RUN_COUNTRY.into(),
            title: None,
            body: None,
            image_bytes: Some(tiny_jpeg()),
        },
    )
    .await
    .unwrap();

    let admin_id = seed_test_admin(&pool).await;
    let img = submissions_service::image_admin(&pool, r.id).await.unwrap();
    assert_eq!(img.content_type, "image/jpeg");
    submissions_service::reject(&pool, admin_id, r.id)
        .await
        .unwrap();
}

/// Two editors accepting the same submission: exactly one UPDATE
/// touches a row, the loser gets a Conflict rather than a silent
/// double-accept.
#[tokio::test]
async fn accepting_twice_conflicts() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;
    let admin_id = seed_test_admin(&pool).await;
    let r = submissions_service::submit(
        &pool,
        member,
        column("A column that will be accepted exactly once, no matter what."),
    )
    .await
    .unwrap();

    submissions_service::accept(&pool, admin_id, r.id)
        .await
        .unwrap();
    assert!(matches!(
        submissions_service::accept(&pool, admin_id, r.id).await,
        Err(AppError::Conflict)
    ));

    // Clean up in SQL rather than through the service: reject() only
    // touches pending rows, so once a submission is accepted there is
    // no service path that removes it. Without this the row survives
    // every run and the editor's table fills with test columns.
    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    sqlx::query("DELETE FROM submissions WHERE id = $1")
        .bind(r.id)
        .execute(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// Rejection deletes the row and its bytes. The audit entry is the only
/// remaining record that the submission existed.
#[tokio::test]
async fn rejection_deletes_the_submission() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;
    let admin_id = seed_test_admin(&pool).await;
    let r = submissions_service::submit(
        &pool,
        member,
        SubmissionUpload {
        prompt: Prompt::RUN_COUNTRY.into(),
            title: Some("Doomed".into()),
            body: Some("A column that is about to be turned down by the editor.".into()),
            image_bytes: Some(tiny_jpeg()),
        },
    )
    .await
    .unwrap();

    submissions_service::reject(&pool, admin_id, r.id)
        .await
        .unwrap();

    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM submissions WHERE id = $1")
        .bind(r.id)
        .fetch_one(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(left, 0, "a rejected submission must be gone, bytes and all");

    // audit_events is admin-select-only, so this read needs the admin
    // GUC — under no context RLS hides the row and the assertion would
    // fail for the wrong reason.
    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    let audited: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE action = 'submission.rejected' AND target = $1",
    )
    .bind(r.id.to_string())
    .fetch_one(tx.conn())
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(audited, 1, "the rejection must leave a record behind");
}

/// A post records which of the three it answers, and the strawberry
/// draws from one of them only.
///
/// 0028 replaced the editor-written taste_lines pool with member
/// answers, so what a sticker in the street returns is now somebody's
/// writing rather than an editor's line.
#[tokio::test]
async fn a_post_belongs_to_a_prompt_and_the_sticker_draws_one() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;
    let admin_id = seed_test_admin(&pool).await;

    let mk = |prompt: &str, text: &str| SubmissionUpload {
        prompt: prompt.into(),
        title: None,
        body: Some(text.into()),
        image_bytes: None,
    };

    assert!(
        submissions_service::submit(
            &pool,
            member,
            mk("run_the_bath", "A prompt nobody ever wrote on the side of a sticker."),
        )
        .await
        .is_err(),
        "a prompt outside the three is refused before it reaches the CHECK"
    );

    let taste = submissions_service::submit(
        &pool,
        member,
        mk(Prompt::BETTER_TASTE, "Pineapple is a rumour. Stop running the day before."),
    )
    .await
    .unwrap();
    let away = submissions_service::submit(
        &pool,
        member,
        mk(Prompt::RUN_AWAY, "Because the alternative is staying where somebody put me."),
    )
    .await
    .unwrap();

    for id in [taste.id, away.id] {
        submissions_service::accept(&pool, admin_id, id).await.unwrap();
    }

    let feed = submissions_service::list_published(&pool).await.unwrap();
    assert_eq!(
        feed.iter().find(|p| p.id == away.id).map(|p| p.prompt.as_str()),
        Some(Prompt::RUN_AWAY),
        "the feed says which one a post answers"
    );

    // The sticker only ever returns better_taste, however many other
    // posts are accepted.
    for _ in 0..8 {
        let drawn = submissions_service::draw_taste(&pool).await.unwrap();
        assert_eq!(
            drawn.prompt, Prompt::BETTER_TASTE,
            "a strawberry must never return an answer to a different question"
        );
    }

    for id in [taste.id, away.id] {
        scrub_submission(&pool, id).await;
    }
}

/// The feed carries published posts and nothing else — a pending
/// submission must never appear in it, however it is asked for.
#[tokio::test]
async fn the_feed_shows_only_published_posts() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;
    let admin_id = seed_test_admin(&pool).await;

    let mk = |text: &str| SubmissionUpload {
        prompt: Prompt::RUN_COUNTRY.into(),
        title: None,
        body: Some(text.into()),
        image_bytes: None,
    };

    let waiting = submissions_service::submit(
        &pool,
        member,
        mk("A post that is still waiting on the editor to make up their mind."),
    )
    .await
    .unwrap();
    let up = submissions_service::submit(
        &pool,
        member,
        mk("A post that the editor has already read and decided to put up."),
    )
    .await
    .unwrap();
    submissions_service::accept(&pool, admin_id, up.id)
        .await
        .unwrap();

    let feed = submissions_service::list_published(&pool).await.unwrap();
    assert!(
        feed.iter().any(|p| p.id == up.id),
        "an accepted post should be in the feed"
    );
    assert!(
        !feed.iter().any(|p| p.id == waiting.id),
        "a pending post must never reach the feed"
    );

    // And its image is private until it is not.
    assert!(matches!(
        submissions_service::published_image(&pool, waiting.id).await,
        Err(AppError::NotFound)
    ));

    submissions_service::reject(&pool, admin_id, waiting.id)
        .await
        .unwrap();
    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    sqlx::query("DELETE FROM submissions WHERE id = $1")
        .bind(up.id)
        .execute(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// The feed exposes exactly the fields it means to.
///
/// 0020 makes accepted rows publicly readable, and RLS is row-level —
/// so any column added to `submissions` later is public the moment a
/// post is accepted, unless the feed query and PublishedSubmission
/// leave it out. This pins the shape, so adding a column and widening
/// the query fails here rather than in the open.
#[tokio::test]
async fn the_feed_exposes_only_the_fields_it_means_to() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;
    let admin_id = seed_test_admin(&pool).await;

    let r = submissions_service::submit(
        &pool,
        member,
        SubmissionUpload {
        prompt: Prompt::RUN_COUNTRY.into(),
            title: Some("A title".into()),
            body: Some("A post used to pin down exactly what the feed gives out.".into()),
            image_bytes: None,
        },
    )
    .await
    .unwrap();
    submissions_service::accept(&pool, admin_id, r.id)
        .await
        .unwrap();

    let feed = submissions_service::list_published(&pool).await.unwrap();
    let post = feed
        .iter()
        .find(|p| p.id == r.id)
        .expect("the post should be in the feed");
    let value = serde_json::to_value(post).unwrap();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "body",
            "has_image",
            "id",
            "member_no",
            "prompt",
            "published_at",
            "title"
        ],
        "the feed's shape changed — check nothing private came with it"
    );

    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    sqlx::query("DELETE FROM submissions WHERE id = $1")
        .bind(r.id)
        .execute(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// Posting is members-only, and the database is what says so. This
/// goes straight at the RLS policy rather than through the handler,
/// because the handler is not the boundary — 0023 is.
#[tokio::test]
async fn a_post_needs_a_membership() {
    let pool = test_pool().await;
    let member = seed_test_member(&pool).await;

    // No user context at all: what an unauthenticated request looks
    // like once the extractor has been bypassed or forgotten.
    let orphan = sqlx::query(
        "INSERT INTO submissions (body, user_id, status)
         VALUES ('a post with nobody behind it, long enough to be a column.', $1, 'pending')",
    )
    .bind(member)
    .execute(&pool)
    .await;
    assert!(
        orphan.is_err(),
        "an insert with no app.user_id must be refused by the policy"
    );

    // And one member may not post as another.
    let other = seed_test_member(&pool).await;
    let mut tx = RlsTransaction::begin(&pool, member).await.unwrap();
    let impersonation = sqlx::query(
        "INSERT INTO submissions (body, user_id, status)
         VALUES ('a post attributed to somebody else entirely, at length.', $1, 'pending')",
    )
    .bind(other)
    .execute(tx.conn())
    .await;
    assert!(
        impersonation.is_err(),
        "posting as another member must be refused by the policy"
    );
}

/// The byline on a post is the member's number, taken from the
/// account rather than from anything the request said.
#[tokio::test]
async fn a_post_carries_the_members_number() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let created = members_service::create(&pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap();

    let r = submissions_service::submit(
        &pool,
        created.user_id,
        SubmissionUpload {
        prompt: Prompt::RUN_COUNTRY.into(),
            title: None,
            body: Some("A post that should carry its author's name without being told it.".into()),
            image_bytes: None,
        },
    )
    .await
    .unwrap();
    submissions_service::accept(&pool, admin_id, r.id)
        .await
        .unwrap();

    let feed = submissions_service::list_published(&pool).await.unwrap();
    let post = feed.iter().find(|p| p.id == r.id).unwrap();
    assert_eq!(post.member_no, created.member_no);
    assert!(created.member_no > 0, "a member is numbered from one");

    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    sqlx::query("DELETE FROM submissions WHERE id = $1")
        .bind(r.id)
        .execute(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// A membership is only representable as verified-at-an-event-by-an-
/// admin, so the credential it hands over is evidence somebody was
/// there.
#[tokio::test]
async fn a_membership_records_where_it_was_granted() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let created = members_service::create(&pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap();
    assert!(!created.token.is_empty(), "a membership hands over a token");

    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    let (status, at_event, by_admin): (String, Uuid, Uuid) = sqlx::query_as(
        "SELECT status, verified_at_event_id, verified_by_admin_id FROM users WHERE id = $1",
    )
    .bind(created.user_id)
    .fetch_one(tx.conn())
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(status, "verified");
    assert_eq!(at_event, event_id);
    assert_eq!(by_admin, admin_id);

    // Only the hash is kept, so the token cannot be recovered later.
    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_sessions WHERE user_id = $1 AND token_hash = $2",
    )
    .bind(created.user_id)
    .bind(box_fraise::crypto::sha256_hex(created.token.as_bytes()))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, 1);
}

/// A membership is kept, not got. Turning up opens the account;
/// turning up every month is what keeps it able to post.
#[tokio::test]
async fn posting_lapses_without_attendance() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let member = members_service::create(&pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap();

    let post = |body: &str| SubmissionUpload {
        prompt: Prompt::RUN_COUNTRY.into(),
        title: None,
        body: Some(body.into()),
        image_bytes: None,
    };

    // Signing up is turning up, so they can post straight away.
    let first = submissions_service::submit(
        &pool,
        member.user_id,
        post("A post from somebody who turned up this morning, at length."),
    )
    .await
    .unwrap();

    // Wind their attendance back past the window. Replaced rather than
    // edited: bf_app has no UPDATE on attendances, because when
    // somebody was somewhere is a fact rather than a field.
    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    sqlx::query("DELETE FROM attendances WHERE user_id = $1")
        .bind(member.user_id)
        .execute(tx.conn())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO attendances (user_id, event_id, recorded_at)
         VALUES ($1, $2, now() - interval '40 days')",
    )
    .bind(member.user_id)
    .bind(event_id)
    .execute(tx.conn())
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let standing = members_service::standing(&pool, member.user_id)
        .await
        .unwrap();
    assert!(!standing.current, "forty days without a run is not current");

    assert!(
        submissions_service::submit(
            &pool,
            member.user_id,
            post("A post from somebody who stopped coming to the run club."),
        )
        .await
        .is_err(),
        "a lapsed member must not be able to post"
    );

    // Nothing was taken away: the account is intact and so is the work.
    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    let still_there: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM submissions WHERE id = $1")
        .bind(first.id)
        .fetch_one(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(still_there, 1, "a lapse must not remove what they wrote");

    // Coming back restores it — at the next run, not the same one. A
    // run can only be attended once, so coming back necessarily means
    // a different event.
    let next_run = seed_test_event(&pool, admin_id).await;
    members_service::record_attendance(
        &pool,
        admin_id,
        RecordAttendanceRequest {
            event_id: next_run,
            member_no: member.member_no,
        },
    )
    .await
    .unwrap();
    assert!(
        members_service::standing(&pool, member.user_id)
            .await
            .unwrap()
            .current,
        "turning up again makes them current"
    );
    assert!(submissions_service::submit(
        &pool,
        member.user_id,
        post("A post from somebody who came back to the run club after a while.")
    )
    .await
    .is_ok());
}

/// Marking somebody twice at the same run is an admin pressing a
/// button again, not a second attendance.
#[tokio::test]
async fn attendance_at_one_run_counts_once() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let member = members_service::create(&pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap();

    // Signing up already recorded this run, so marking it is a no-op.
    let again = members_service::record_attendance(
        &pool,
        admin_id,
        RecordAttendanceRequest {
            event_id,
            member_no: member.member_no,
        },
    )
    .await
    .unwrap();
    assert!(!again.was_new, "the same run cannot be attended twice");

    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attendances WHERE user_id = $1")
        .bind(member.user_id)
        .fetch_one(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(rows, 1);
}

/// A member cannot mark themselves present.
#[tokio::test]
async fn a_member_cannot_record_their_own_attendance() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let member = members_service::create(&pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap();

    let mut tx = RlsTransaction::begin(&pool, member.user_id).await.unwrap();
    let attempt = sqlx::query("INSERT INTO attendances (user_id, event_id) VALUES ($1, $2)")
        .bind(member.user_id)
        .bind(event_id)
        .execute(tx.conn())
        .await;
    assert!(
        attempt.is_err(),
        "attendance is something an admin says about you, not something you say"
    );
}

/// Resolve a raw token the way `resolve_bearer` does — by hash, and by
/// nothing else. Tests call services directly, so this stands in for
/// the middleware when what is under test is whether a credential still
/// opens anything.
async fn session_owner(pool: &PgPool, token: &str) -> Option<Uuid> {
    sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM user_sessions WHERE token_hash = $1")
        .bind(sha256_hex(token.as_bytes()))
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// A replacement credential is the same membership on a different
/// phone. This is the whole reason the endpoint exists: losing a device
/// used to cost somebody their number, and their posts stayed under a
/// byline they could no longer write as.
#[tokio::test]
async fn a_replacement_credential_keeps_the_member() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let member = members_service::create(&pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap();

    let replaced = members_service::reissue(
        &pool,
        admin_id,
        ReissueRequest {
            member_no: member.member_no,
        },
    )
    .await
    .unwrap();

    assert_eq!(replaced.user_id, member.user_id, "the same account");
    assert_eq!(
        replaced.member_no, member.member_no,
        "a lost phone does not cost somebody their number"
    );
    assert_ne!(
        replaced.token, member.token,
        "the server kept only a hash and could not reissue the old one if it wanted to"
    );
    assert_eq!(replaced.sessions_ended, 1);
    assert!(
        replaced.current,
        "signing up counted as turning up, and replacing a phone does not undo that"
    );

    assert_eq!(
        session_owner(&pool, &member.token).await,
        None,
        "whoever found the old phone is not this member any more"
    );
    assert_eq!(
        session_owner(&pool, &replaced.token).await,
        Some(member.user_id),
        "and the new code opens the same account"
    );
}

/// Signing out ends one session. Not the account, and not anybody
/// else's — 0027 scopes the delete by user_id, so naming a stranger's
/// session exactly still matches nothing.
#[tokio::test]
async fn signing_out_ends_that_session_and_no_other() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let a = members_service::create(&pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap();
    let b = members_service::create(&pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap();

    // The policy is the enforcement, so this is allowed to succeed —
    // it simply deletes nothing.
    members_service::sign_out(&pool, a.user_id, &sha256_hex(b.token.as_bytes()))
        .await
        .unwrap();
    assert_eq!(
        session_owner(&pool, &b.token).await,
        Some(b.user_id),
        "one member cannot log another one out"
    );

    members_service::sign_out(&pool, a.user_id, &sha256_hex(a.token.as_bytes()))
        .await
        .unwrap();
    assert_eq!(session_owner(&pool, &a.token).await, None);

    // Signing out is not leaving. The number, the posts and the
    // attendance are all still there for the next credential.
    let standing = members_service::standing(&pool, a.user_id).await.unwrap();
    assert_eq!(standing.member_no, a.member_no);
    assert!(standing.current);
}

/// A channel exists only while the pairing is open. Waiting,
/// deciding, lapsed or closed all carry nothing — the policies in
/// 0026 say so, not a handler.
#[tokio::test]
async fn a_message_needs_an_open_channel() {
    let pool = test_pool().await;
    let (a, b, pairing_id) = seed_pair(&pool).await;

    let say = |body: &str| SendMessageRequest {
        ciphertext: URL_SAFE_NO_PAD.encode(body.repeat(4).as_bytes()),
        iv: URL_SAFE_NO_PAD.encode([0u8; 12]),
    };

    // Nobody has decided yet, so there is nothing to talk through.
    assert!(
        messages_service::send(&pool, a, pairing_id, say("too soon"))
            .await
            .is_err(),
        "a pairing that has not opened cannot carry a message"
    );

    // Both say yes, once the cooling period has passed.
    wait_for_window().await;
    for who in [a, b] {
        pairings_service::decide(
            &pool,
            who,
            pairing_id,
            DecisionRequest {
                decision: "yes".into(),
            },
        )
        .await
        .unwrap();
    }

    messages_service::send(&pool, a, pairing_id, say("hello there"))
        .await
        .unwrap();
    assert_eq!(
        messages_service::list(&pool, b, pairing_id)
            .await
            .unwrap()
            .len(),
        1,
        "the other half of the channel can read it"
    );

    // A stranger sees nothing and cannot write.
    let (outsider, _sk) = register_with_keypair(&pool).await;
    assert!(messages_service::list(&pool, outsider, pairing_id)
        .await
        .unwrap()
        .is_empty());
    assert!(
        messages_service::send(&pool, outsider, pairing_id, say("let me in"))
            .await
            .is_err()
    );

    // Blocking takes the channel away from both, rather than leaving
    // one side holding a transcript of somebody who left.
    pairings_service::block(&pool, b, pairing_id).await.unwrap();

    assert!(messages_service::list(&pool, a, pairing_id)
        .await
        .unwrap()
        .is_empty());
    assert!(
        messages_service::send(&pool, a, pairing_id, say("still there?"))
            .await
            .is_err()
    );
}

/// The server stores what it was given and cannot do otherwise: no
/// column holds plaintext, and nothing may edit or unsay a message.
#[tokio::test]
async fn messages_are_opaque_and_final() {
    let pool = test_pool().await;

    let mut tx = AdminRlsTransaction::begin(&pool).await.unwrap();
    let columns: Vec<(String,)> = sqlx::query_as(
        "SELECT column_name FROM information_schema.columns WHERE table_name = 'messages'",
    )
    .fetch_all(tx.conn())
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let names: Vec<&str> = columns.iter().map(|c| c.0.as_str()).collect();
    assert!(
        !names.contains(&"body") && !names.contains(&"text") && !names.contains(&"plaintext"),
        "messages must carry no readable column: {names:?}"
    );

    // bf_app has SELECT and INSERT and nothing else.
    for verb in ["UPDATE", "DELETE"] {
        let granted: bool =
            sqlx::query_scalar("SELECT has_table_privilege('bf_app', 'messages', $1)")
                .bind(verb)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!granted, "bf_app must not be able to {verb} a message");
    }
}

/// Two members can pair with nothing but their memberships. This is
/// the path that matters: a member has no device key, because their
/// credential was handed to them on a phone at a run, so requiring a
/// signature would mean nobody can ever start a channel.
#[tokio::test]
async fn two_members_can_pair_without_device_keys() {
    let pool = test_pool().await;
    let admin_id = seed_test_admin(&pool).await;
    let event_id = seed_test_event(&pool, admin_id).await;
    let a = members_service::create(&pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap()
        .user_id;
    let b = members_service::create(&pool, admin_id, CreateMemberRequest { event_id })
        .await
        .unwrap()
        .user_id;

    // Neither has one, which is the whole point.
    let keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_keys WHERE user_id = ANY($1)")
        .bind(vec![a, b])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(keys, 0);

    let nonce = pairings_service::issue_nonce(&pool, a).await.unwrap().nonce;
    let claimed = pairings_service::claim_in_person(
        &pool,
        b,
        SHORT_COOLING,
        LONG_WINDOW,
        ClaimInPersonRequest {
            nonce: nonce.clone(),
        },
    )
    .await
    .unwrap();

    // One code, one pairing: it burns on use.
    assert!(matches!(
        pairings_service::claim_in_person(
            &pool,
            b,
            SHORT_COOLING,
            LONG_WINDOW,
            ClaimInPersonRequest { nonce },
        )
        .await,
        Err(AppError::Conflict)
    ));

    // Your own code is not a way to pair with yourself.
    let own = pairings_service::issue_nonce(&pool, a).await.unwrap().nonce;
    assert!(matches!(
        pairings_service::claim_in_person(
            &pool,
            a,
            SHORT_COOLING,
            LONG_WINDOW,
            ClaimInPersonRequest { nonce: own },
        )
        .await,
        Err(AppError::BadRequest(_))
    ));

    // And it is a real pairing: both decide, and a channel opens.
    wait_for_window().await;
    for who in [a, b] {
        pairings_service::decide(
            &pool,
            who,
            claimed.pairing_id,
            DecisionRequest {
                decision: "yes".into(),
            },
        )
        .await
        .unwrap();
    }
    messages_service::send(
        &pool,
        a,
        claimed.pairing_id,
        SendMessageRequest {
            ciphertext: URL_SAFE_NO_PAD.encode("something long enough to pass".as_bytes()),
            iv: URL_SAFE_NO_PAD.encode([0u8; 12]),
        },
    )
    .await
    .unwrap();
}
