# Pairing — spec

How two people who met in person get a chat channel, and why it takes
three days.

Status: **design, not built.** Nothing in this document exists in code
yet. Open questions are collected at the end.

---

## Why this exists

`box-fraise-chat` is a libsignal relay. It resolves identity by asking
this server (`GET /v1/me`) and stores prekeys and ciphertext. What it
does **not** do is decide who is allowed to talk to whom:
`messages::service::send` validates sender ≠ recipient, envelope type,
and ciphertext length, and nothing else. Any authenticated user who
learns another user's UUID can message them.

Pairing is that missing gate. It is the reason chat can be switched on
safely.

It is also the smallest possible statement of the platform's thesis: a
channel exists because two people met, in person, and both said so —
separately, and later.

---

## The shape

1. **They meet at an event.** Both are already verified users, so both
   have a Secure Enclave key and a device the server can challenge.
2. **They exchange codes in person.** One shows a QR, the other scans
   it and signs it. That produces cryptographic evidence that two
   specific devices were in the same place at the same moment.
3. **Nothing else happens.** No request is sent, no decision is
   offered, no channel opens. There is nothing either person can be
   pressured into agreeing to on the spot.
4. **Three days pass.**
5. **Each is asked, separately: still want to talk?** Independently,
   alone, away from whoever is standing in front of them.
6. **If both say yes, chat opens.** If either doesn't, nothing opens,
   and neither is told which of the two it was.

Step 3 is the point. The cooling-off period is not a delay on a
decision made at the event; it is the reason no decision is made at
the event at all.

---

## The rule that everything else serves

**A declined pairing and an ignored pairing must be indistinguishable
to the other party — including in timing.**

If the two are distinguishable, three things break at once. The wait
becomes a countdown to a verdict. The person who declined becomes
identifiable as having declined. And "no" acquires a social cost,
which is precisely the cost the delay was introduced to remove.

Concretely, this constrains the implementation in ways that are easy
to violate by accident:

- No endpoint returns the other party's decision. Not as a boolean,
  not as a status string, not as a timestamp.
- An explicit "no" **does not** delete the row or change anything the
  other party can observe. The row lapses at `expires_at` exactly as
  an ignored one would. Deleting early is a side channel: the other
  side would watch it disappear ahead of schedule.
- No notification is emitted on decline.
- The audit trail records *that* a decision was made and by whom, not
  which way. There is no operational need for the value, and the
  audit table is one more place the property could leak.

The only thing either party ever learns is whether the channel opened.

---

## Schema sketch (migration 0015)

### `pairing_nonces`

Mirrors the existing `challenges` table: short-lived, single-use.

```sql
CREATE TABLE pairing_nonces (
    nonce         TEXT PRIMARY KEY,
    initiator_id  UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    event_id      UUID REFERENCES events (id) ON DELETE SET NULL,
    expires_at    TIMESTAMPTZ NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

The QR carries **only the nonce** — never the initiator's UUID.
Anyone photographing someone's screen across a room should learn
nothing. The server knows who the nonce belongs to because it issued
it to an authenticated caller.

### `pairings`

```sql
CREATE TABLE pairings (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Ordered pair. See note below.
    lower_user_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    upper_user_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,

    event_id          UUID REFERENCES events (id) ON DELETE SET NULL,

    met_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    opens_at          TIMESTAMPTZ NOT NULL,   -- met_at + cooling period
    expires_at        TIMESTAMPTZ NOT NULL,   -- opens_at + decision window

    lower_decision    TEXT,                   -- NULL | 'yes' | 'no'
    upper_decision    TEXT,
    lower_decided_at  TIMESTAMPTZ,
    upper_decided_at  TIMESTAMPTZ,

    opened_at         TIMESTAMPTZ,
    closed_at         TIMESTAMPTZ,
    closed_by         UUID REFERENCES users (id) ON DELETE SET NULL,

    CONSTRAINT pairings_ordered CHECK (lower_user_id < upper_user_id),
    CONSTRAINT pairings_decisions_valid CHECK (
        (lower_decision IS NULL OR lower_decision IN ('yes','no')) AND
        (upper_decision IS NULL OR upper_decision IN ('yes','no'))
    )
);

CREATE UNIQUE INDEX pairings_unique_pair
    ON pairings (lower_user_id, upper_user_id);
```

**The ordered pair is doing real work.** Storing the two user ids
sorted, with a unique index, makes "one pairing per unordered pair" a
database guarantee rather than an application convention. Without it
you get duplicate rows the moment both people scan each other, and a
reciprocal-row bug that only shows up under a race.

**Status is computed, not stored**, so it cannot drift from the
timestamps:

| condition                                    | status     |
|----------------------------------------------|------------|
| `closed_at` set                              | `closed`   |
| `opened_at` set                              | `open`     |
| `now() < opens_at`                           | `waiting`  |
| `now() >= expires_at`                        | `lapsed`   |
| otherwise                                    | `deciding` |

### RLS

```sql
CREATE POLICY pairings_own_select ON pairings
    FOR SELECT
    USING (
        lower_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR upper_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
    );
```

RLS is row-level, so it gets a caller to their own rows and no
further. **It cannot hide the other party's decision column** — that
boundary is in the application, and it is the one from the rule above.
The repository must never select the counterparty's `*_decision` into
any type that reaches a handler. Enforce it by shape: the read model
carries `my_decision` and `status`, and has nowhere to put the other
value.

---

## Endpoints — `box-fraise-server`

| Endpoint                                  | Auth | Notes                                                     |
|-------------------------------------------|------|-----------------------------------------------------------|
| `POST /v1/pairings/nonce`                 | user | Issues a short-lived nonce; optional `event_id` binding    |
| `POST /v1/pairings/claim`                 | user | `{nonce, signature_b64}` → creates the pairing             |
| `GET  /v1/pairings`                       | user | Caller's pairings, safe fields only                        |
| `POST /v1/pairings/{id}/decision`         | user | `{decision}`; only inside `[opens_at, expires_at)`          |
| `POST /v1/pairings/{id}/block`            | user | Closes permanently                                         |
| `GET  /v1/pairings/authorized?peer={id}`  | user | `{authorized: bool}` — for the chat service                 |

### `POST /v1/pairings/claim`

The scanner posts the nonce with a signature over it from their own
Secure Enclave key. The server:

1. Resolves the nonce → initiator, checks it is unexpired and unused.
2. Verifies the signature against the scanner's registered device
   keys, using the existing P-256 / DER / SHA-256 / low-S-normalised
   path in `crypto::verify_p256_signature`.
3. Rejects `initiator == scanner`.
4. Inserts the pairing with the ids sorted, `opens_at = now() +
   cooling`, `expires_at = opens_at + window`.
5. Burns the nonce.

A unique-violation on `pairings_unique_pair` means these two are
already paired → **409**, not a second row.

What this proves: the scanner's device was close enough to read the
initiator's screen within the nonce's lifetime, and the initiator
asked for that code while authenticated. Both parties acted. That is
the in-person part, and it reuses the primitive the event verification
flow already relies on.

### `POST /v1/pairings/{id}/decision`

Rejected before `opens_at` — **there is no way to answer early**, which
is what makes the cooling-off real rather than cosmetic. Rejected
after `expires_at`. Writes the caller's side only.

If, after the write, both sides are `'yes'`, set `opened_at`. Use the
same race-close idiom as the verify flip so two simultaneous
confirmations produce exactly one open:

```sql
UPDATE pairings SET opened_at = now()
 WHERE id = $1 AND opened_at IS NULL
   AND lower_decision = 'yes' AND upper_decision = 'yes'
 RETURNING opened_at;
```

### `GET /v1/pairings/authorized`

`true` only when `opened_at IS NOT NULL AND closed_at IS NULL`.

---

## Changes in `box-fraise-chat`

Chat gains a `PairingGate` alongside the existing `Verifier`, using the
same shape: call this server, cache briefly, fail closed.

Gate **both** of these:

- `messages::service::send` — the obvious one.
- **Prekey bundle fetch.** Less obvious and just as important: being
  able to fetch someone's bundle confirms they exist and are
  reachable. Leaving it open turns chat into a user-enumeration
  oracle even with sending locked down.

Cache TTL is a revocation-latency tradeoff. `Verifier` caches `/v1/me`
already; a block should take effect faster than a token cache needs
to, so keep this TTL short (≈60s) or invalidate on block. **Blocking
that takes five minutes to bite is not blocking.**

Encryption is untouched. This is authorization metadata only — the
server continues to hold ciphertext it cannot read, and nothing here
needs message content.

---

## Blocking

`closed` is terminal. The pairing is not deleted, so the pair cannot be
re-created by scanning again — the unique index sees the existing row.

Unlike declining, blocking **is** observable: the other party's
messages stop being accepted. That asymmetry is deliberate. A decline
happens before any relationship exists and should cost nothing; a
block ends one that does, and someone whose messages are going nowhere
is entitled to know the conversation is closed. They are not told why,
and not told by whom — in a two-person channel that is inferable, and
pretending otherwise would be theatre.

---

## Maintenance

Extend the existing hourly prune:

- `pairing_nonces` past `expires_at`.
- `pairings` past `expires_at` that never opened — the lapsed ones.
  Without this you accumulate a permanent record of every near-miss
  connection anyone ever made, which is exactly the kind of social
  graph this project has no reason to keep.

Open pairings are never pruned on a timer.

---

## Audit

New actions: `pairing.nonce_issued`, `pairing.created`,
`pairing.decided`, `pairing.opened`, `pairing.blocked`.

`pairing.decided` records the actor and the pairing, **not the value**.
See the rule above.

---

## Abuse

- **Cap pending pairings per user per event.** Someone working a room
  scanning everyone. Both-must-confirm bounds the harm, but a cap
  keeps the queue and the table sane. Suggested: 10 per event.
- Nonce TTL should be short — 120s, matching `CHALLENGE_TTL_SECS`.
  A code that stays live for an hour can be photographed and used
  from somewhere else entirely.
- One nonce, one use.

---

## Open questions

1. **Cooling period and decision window.** Proposal: 72h cooling, then
   a 7-day window to decide, so a pairing lives at most 10 days
   undecided. Both should be config, not constants.
2. **Display names do not exist.** `users` has device keys and a
   status; there is no name, handle, or photo. Pairing needs *some*
   identity to show, and adding it is a prerequisite, not a detail.
3. **What is shown during the wait.** Proposal: the event and the
   date, and nothing else — "someone you met at Think Out Loud, 14
   March" — with anything more revealed only on opening. The less
   shown before both agree, the less a scan is worth to someone
   collecting them.
4. **Re-pairing after a block.** The spec above makes it impossible
   forever. That is the safe default; it is also unforgiving of a
   mistaken tap. An "unblock" would need to be reachable only by the
   person who blocked.
5. **No push infrastructure.** Nothing can tell someone their three
   days are up. Until that exists, the flow depends on people opening
   the app and looking, which they will mostly not do. This is the
   biggest practical risk to the feature working at all, and it is not
   solved by anything in this document.
