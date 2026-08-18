// Cookies, by hand.
//
// A member's credential used to live in localStorage. That was the
// wrong place for it. Safari's tracking prevention deletes
// script-writable storage — localStorage and IndexedDB both — after
// seven days without a visit to the site, and this platform asks
// members to turn up once a *month*. An iPhone member who did nothing
// wrong could open the site after a fortnight and find their
// membership gone, with no way back except queueing at a run for a new
// number.
//
// A cookie set by the server, over a first-party response, is not
// script-writable and survives that sweep. It is also unreadable by
// script at all, which is worth having on its own.
//
// Written by hand rather than by pulling in a cookie crate: we set two
// cookies and read one, all of them ours, none of them needing quoting
// or attribute parsing. The whole surface is below.

use axum::http::{HeaderMap, HeaderValue};

/// The credential. HttpOnly — nothing in the page ever needs to read
/// it, and the iOS app uses the Authorization header instead.
pub const SESSION: &str = "bf_session";

/// The member number, readable by script. Not a credential: it is the
/// public byline on everything they post, printed on the page next to
/// it. It exists so the page can tell whether somebody is signed in,
/// and who, without a round trip on every load.
pub const MEMBER: &str = "bf_member";

/// Two years. There is no shorter honest number: a membership is meant
/// to be kept, and the thing that lapses is attendance, not this.
const MAX_AGE: i64 = 60 * 60 * 24 * 730;

/// Read one of our cookies out of the request headers.
///
/// Deliberately tolerant of the shapes browsers actually send — pairs
/// separated by "; ", possibly with stray whitespace — and deliberately
/// not a general cookie parser. We never set a value needing quotes or
/// percent-decoding, so anything that would need it is not ours.
pub fn get(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| k.trim() == name)
        .map(|(_, v)| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Whether this request reached us over TLS.
///
/// Behind Railway's router the connection into this process is plain
/// HTTP, so the scheme has to come from the proxy's header. Absent in
/// local development, which is exactly when we must not set `Secure` —
/// a Secure cookie over http://localhost is silently dropped and the
/// sign-in appears to do nothing.
pub fn is_secure(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').next().unwrap_or("").trim().eq_ignore_ascii_case("https"))
        .unwrap_or(false)
}

/// The pair of Set-Cookie values that sign somebody in.
///
/// SameSite=Lax rather than Strict: a member following a link to
/// /gurgle from somewhere else should arrive signed in. Lax still
/// withholds the cookie from cross-site POSTs, which is the CSRF case
/// that matters now that a cookie rather than a header carries the
/// credential.
pub fn sign_in(token: &str, member_no: i32, secure: bool) -> Vec<HeaderValue> {
    vec![
        set(SESSION, token, MAX_AGE, true, secure),
        set(MEMBER, &member_no.to_string(), MAX_AGE, false, secure),
    ]
}

/// The pair that signs somebody out. Same name, same path, empty value,
/// Max-Age=0 — a cookie is only replaceable by one matching it.
pub fn sign_out(secure: bool) -> Vec<HeaderValue> {
    vec![
        set(SESSION, "", 0, true, secure),
        set(MEMBER, "", 0, false, secure),
    ]
}

fn set(name: &str, value: &str, max_age: i64, http_only: bool, secure: bool) -> HeaderValue {
    let mut v = format!("{name}={value}; Path=/; Max-Age={max_age}; SameSite=Lax");
    if http_only {
        v.push_str("; HttpOnly");
    }
    if secure {
        v.push_str("; Secure");
    }
    // Every part of this string is either a literal above or a token
    // we minted ourselves (base64url) or an integer, so it cannot
    // contain a byte a header value rejects.
    HeaderValue::from_str(&v).expect("cookie built from safe parts")
}
