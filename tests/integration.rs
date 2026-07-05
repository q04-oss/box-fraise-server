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
    crypto::{argon2_hash, new_nonce, verify_p256_signature},
    db::{self, AdminRlsTransaction, RlsTransaction},
    domain::{
        consultations::{
            service as consultations_service,
            types::{CompleteConsultationRequest, ReplaceCardRequest, RevokeCardRequest},
        },
        events::{service as events_service, types::CreateEventRequest},
        modeling::{
            service as modeling_service,
            types::{CreateModelRequestRequest, HairProfileInput, UpdateOwnHairProfileRequest},
        },
        oauth::{
            jwt as oauth_jwt, keys as oauth_keys, service as oauth_service,
            types::TokenRequest as OauthTokenRequest,
        },
        onboarding::{
            service as onboarding_service,
            types::{RegisterRequest, VerifyRequest},
        },
        schedule::{
            service as schedule_service,
            types::{CreatePersonalItemRequest, UpdatePersonalItemRequest},
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
            address: "123 Test St, Montreal".into(),
            latitude: 45.5,
            longitude: -73.5,
            starts_at: now,
            ends_at: now + ChronoDuration::hours(4),
            published: true,
        },
    )
    .await
    .unwrap()
    .id
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

/// (14) Personal items are strictly owner-scoped. User A creates one,
/// User B queries their own list and cannot see it — RLS is the safety
/// net even without user-level filtering.
#[tokio::test]
async fn personal_items_are_owner_scoped() {
    let pool = test_pool().await;
    let (user_a, _) = register_with_keypair(&pool).await;
    let (user_b, _) = register_with_keypair(&pool).await;

    let now = Utc::now();
    let created = schedule_service::create_personal(
        &pool,
        user_a,
        CreatePersonalItemRequest {
            title: "Owner scope test".into(),
            notes: Some("private".into()),
            starts_at: now,
            ends_at: now + ChronoDuration::hours(1),
            is_all_day: false,
            location: Some("everywhere".into()),
        },
    )
    .await
    .unwrap();

    let a_items = schedule_service::list_personal(&pool, user_a)
        .await
        .unwrap();
    assert!(a_items.iter().any(|i| i.id == created.id));

    let b_items = schedule_service::list_personal(&pool, user_b)
        .await
        .unwrap();
    assert!(
        b_items.iter().all(|i| i.id != created.id),
        "user B saw user A's private item — RLS leak"
    );
}

/// (15) Update lifecycle: title change, all-day toggle, notes clear on
/// empty string. Confirms COALESCE-based partial update logic.
#[tokio::test]
async fn personal_items_update_lifecycle() {
    let pool = test_pool().await;
    let (user_id, _) = register_with_keypair(&pool).await;

    let now = Utc::now();
    let item = schedule_service::create_personal(
        &pool,
        user_id,
        CreatePersonalItemRequest {
            title: "Original".into(),
            notes: Some("initial notes".into()),
            starts_at: now,
            ends_at: now + ChronoDuration::hours(1),
            is_all_day: false,
            location: Some("here".into()),
        },
    )
    .await
    .unwrap();

    // Update just the title.
    let after_title = schedule_service::update_personal(
        &pool,
        user_id,
        item.id,
        UpdatePersonalItemRequest {
            title: Some("Renamed".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(after_title.title, "Renamed");
    assert_eq!(after_title.notes.as_deref(), Some("initial notes"));
    assert!(!after_title.is_all_day);

    // Toggle all-day + clear notes via empty string.
    let after_toggle = schedule_service::update_personal(
        &pool,
        user_id,
        item.id,
        UpdatePersonalItemRequest {
            is_all_day: Some(true),
            notes: Some(String::new()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(after_toggle.is_all_day);
    assert!(after_toggle.notes.is_none(), "empty notes should clear");
}

/// (16) Delete lifecycle: user can delete their own; a second delete
/// returns NotFound.
#[tokio::test]
async fn personal_items_delete_and_missing() {
    let pool = test_pool().await;
    let (user_id, _) = register_with_keypair(&pool).await;

    let now = Utc::now();
    let item = schedule_service::create_personal(
        &pool,
        user_id,
        CreatePersonalItemRequest {
            title: "To delete".into(),
            notes: None,
            starts_at: now,
            ends_at: now + ChronoDuration::minutes(30),
            is_all_day: false,
            location: None,
        },
    )
    .await
    .unwrap();

    schedule_service::delete_personal(&pool, user_id, item.id)
        .await
        .unwrap();

    // A second delete finds nothing → NotFound.
    let r = schedule_service::delete_personal(&pool, user_id, item.id).await;
    assert!(
        matches!(r, Err(AppError::NotFound)),
        "second delete should 404: {r:?}"
    );
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
            hair_profile: None,
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
            hair_profile: None,
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
            hair_profile: None,
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
            hair_profile: None,
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

/// (21) OAuth: token issued for a user is a valid ES256 JWT we can
/// verify with our own JWKS key, carrying the expected claims.
#[tokio::test]
async fn oauth_token_round_trips() {
    let pool = test_pool().await;
    let (user_id, _) = register_with_keypair(&pool).await;

    let resp = oauth_service::issue_token(
        &pool,
        user_id,
        OauthTokenRequest {
            audience: "https://partner.example".into(),
        },
    )
    .await
    .unwrap();

    // Verify with our public key.
    let vk = oauth_keys::verifying_key();
    let claims = oauth_jwt::verify(&resp.access_token, &vk)
        .expect("our own token must verify with our own public key");
    assert_eq!(claims["sub"].as_str().unwrap(), user_id.to_string());
    assert_eq!(claims["aud"].as_str().unwrap(), "https://partner.example");
    assert_eq!(claims["iss"].as_str().unwrap(), "https://fraise.box");
    assert_eq!(claims["tier"].as_i64().unwrap(), 1);
    assert!(!claims["verified"].as_bool().unwrap());

    // userinfo endpoint accepts the token.
    let info = oauth_service::userinfo(&pool, &resp.access_token)
        .await
        .unwrap();
    assert_eq!(info.sub, user_id);
    assert_eq!(info.tier, 1);
    assert!(!info.verified);
}

/// (22) Tampered token fails verification.
#[tokio::test]
async fn oauth_tampered_token_rejected() {
    let pool = test_pool().await;
    let (user_id, _) = register_with_keypair(&pool).await;

    let resp = oauth_service::issue_token(
        &pool,
        user_id,
        OauthTokenRequest {
            audience: "https://partner.example".into(),
        },
    )
    .await
    .unwrap();

    // Flip one character in the signature (last part after final '.').
    let mut parts: Vec<&str> = resp.access_token.split('.').collect();
    let mut sig = parts[2].to_string();
    let first_char = sig.chars().next().unwrap();
    let flipped = if first_char == 'A' { 'B' } else { 'A' };
    sig.replace_range(0..1, &flipped.to_string());
    parts[2] = &sig;
    let tampered = parts.join(".");

    let r = oauth_service::userinfo(&pool, &tampered).await;
    assert!(
        matches!(r, Err(AppError::Unauthorized)),
        "tampered sig should reject: {r:?}"
    );
}

/// (23) Model request lifecycle: student posts a request, the server
/// fans out invitations to matching willing-to-model users, the first
/// accept fills the request, further accepts race and lose.
#[tokio::test]
async fn model_request_matches_and_fills() {
    let pool = test_pool().await;

    let (consultant_id, _) = register_with_keypair(&pool).await;
    seed_trained_consultant(&pool, consultant_id).await;

    // A unique hair_type per test run so other tests' fixtures don't
    // accidentally match this test's fan-out filter.
    let unique_type = format!("test-{}", Uuid::new_v4());

    let (student_id, _) = register_with_keypair(&pool).await;
    consultations_service::complete_consultation(
        &pool,
        consultant_id,
        CompleteConsultationRequest {
            user_id: student_id,
            salon_id: None,
            consultation_notes: None,
            consent_snapshot: serde_json::Value::Null,
            design_version: "v1".into(),
            hair_profile: Some(HairProfileInput {
                hair_length: Some("long".into()),
                hair_texture: Some("straight".into()),
                hair_type: Some(unique_type.clone()),
                hair_thickness: None,
                natural_color: Some("brown".into()),
                current_color: None,
                chemically_treated: false,
                willing_services: None,
                willing_to_model: false,
                is_hair_student: true,
                hair_notes: None,
            }),
        },
    )
    .await
    .unwrap();

    // Two matching willing-to-model users (share the unique hair_type)
    // and one non-matcher that has the same type but wrong length.
    let (model_a, _) = register_with_keypair(&pool).await;
    let (model_b, _) = register_with_keypair(&pool).await;
    let (model_c, _) = register_with_keypair(&pool).await;
    for (uid, length) in [(model_a, "long"), (model_b, "long"), (model_c, "short")] {
        consultations_service::complete_consultation(
            &pool,
            consultant_id,
            CompleteConsultationRequest {
                user_id: uid,
                salon_id: None,
                consultation_notes: None,
                consent_snapshot: serde_json::Value::Null,
                design_version: "v1".into(),
                hair_profile: Some(HairProfileInput {
                    hair_length: Some(length.into()),
                    hair_texture: Some("straight".into()),
                    hair_type: Some(unique_type.clone()),
                    hair_thickness: None,
                    natural_color: Some("brown".into()),
                    current_color: None,
                    chemically_treated: false,
                    willing_services: None,
                    willing_to_model: true,
                    is_hair_student: false,
                    hair_notes: None,
                }),
            },
        )
        .await
        .unwrap();
    }

    let now = Utc::now();
    let created = modeling_service::create_model_request(
        &pool,
        student_id,
        CreateModelRequestRequest {
            service: "colour practice".into(),
            starts_at: now + ChronoDuration::days(3),
            ends_at: now + ChronoDuration::days(3) + ChronoDuration::hours(2),
            location: "Cosmetology school, Edmonton".into(),
            location_lat: None,
            location_lng: None,
            filter_length: vec!["long".into()],
            filter_texture: vec![],
            filter_type: vec![unique_type.clone()],
            filter_color: vec![],
            additional_notes: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(created.invitations_sent, 2, "should fan out to A + B only");

    let a_inv = modeling_service::list_own_invitations(&pool, model_a)
        .await
        .unwrap();
    assert_eq!(a_inv.len(), 1);
    let a_invitation_id = a_inv[0].invitation.id;

    let c_inv = modeling_service::list_own_invitations(&pool, model_c)
        .await
        .unwrap();
    assert!(c_inv.is_empty(), "model_c hair does not match");

    modeling_service::respond_to_invitation(&pool, model_a, a_invitation_id, true)
        .await
        .unwrap();

    let b_inv = modeling_service::list_own_invitations(&pool, model_b)
        .await
        .unwrap();
    let b_invitation_id = b_inv[0].invitation.id;
    let r = modeling_service::respond_to_invitation(&pool, model_b, b_invitation_id, true).await;
    assert!(
        matches!(r, Err(AppError::Conflict)),
        "second accept should conflict: {r:?}"
    );
}

/// (24) A non-student cannot create a model request.
#[tokio::test]
async fn non_student_cannot_post_model_request() {
    let pool = test_pool().await;
    let (consultant_id, _) = register_with_keypair(&pool).await;
    seed_trained_consultant(&pool, consultant_id).await;

    let (user_id, _) = register_with_keypair(&pool).await;
    consultations_service::complete_consultation(
        &pool,
        consultant_id,
        CompleteConsultationRequest {
            user_id,
            salon_id: None,
            consultation_notes: None,
            consent_snapshot: serde_json::Value::Null,
            design_version: "v1".into(),
            hair_profile: Some(HairProfileInput {
                hair_length: None,
                hair_texture: None,
                hair_type: None,
                hair_thickness: None,
                natural_color: None,
                current_color: None,
                chemically_treated: false,
                willing_services: None,
                willing_to_model: false,
                is_hair_student: false,
                hair_notes: None,
            }),
        },
    )
    .await
    .unwrap();

    let now = Utc::now();
    let r = modeling_service::create_model_request(
        &pool,
        user_id,
        CreateModelRequestRequest {
            service: "cut".into(),
            starts_at: now + ChronoDuration::hours(1),
            ends_at: now + ChronoDuration::hours(2),
            location: "somewhere".into(),
            location_lat: None,
            location_lng: None,
            filter_length: vec![],
            filter_texture: vec![],
            filter_type: vec![],
            filter_color: vec![],
            additional_notes: None,
        },
    )
    .await;
    assert!(
        matches!(r, Err(AppError::Forbidden)),
        "non-student should be Forbidden: {r:?}"
    );
}

/// (25) User can toggle their own willing_to_model flag from /me.
#[tokio::test]
async fn user_can_toggle_willing_to_model() {
    let pool = test_pool().await;
    let (consultant_id, _) = register_with_keypair(&pool).await;
    seed_trained_consultant(&pool, consultant_id).await;

    let (user_id, _) = register_with_keypair(&pool).await;
    consultations_service::complete_consultation(
        &pool,
        consultant_id,
        CompleteConsultationRequest {
            user_id,
            salon_id: None,
            consultation_notes: None,
            consent_snapshot: serde_json::Value::Null,
            design_version: "v1".into(),
            hair_profile: Some(HairProfileInput {
                hair_length: Some("medium".into()),
                hair_texture: Some("wavy".into()),
                hair_type: None,
                hair_thickness: None,
                natural_color: None,
                current_color: None,
                chemically_treated: false,
                willing_services: None,
                willing_to_model: true,
                is_hair_student: false,
                hair_notes: None,
            }),
        },
    )
    .await
    .unwrap();

    let updated = modeling_service::update_own_willing_to_model(
        &pool,
        user_id,
        UpdateOwnHairProfileRequest {
            willing_to_model: false,
        },
    )
    .await
    .unwrap();
    assert!(!updated.willing_to_model);
    assert_eq!(updated.hair_length.as_deref(), Some("medium"));

    let updated = modeling_service::update_own_willing_to_model(
        &pool,
        user_id,
        UpdateOwnHairProfileRequest {
            willing_to_model: true,
        },
    )
    .await
    .unwrap();
    assert!(updated.willing_to_model);
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
