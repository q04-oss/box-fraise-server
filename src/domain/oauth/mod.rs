// Login API — deliberately unsupported.
//
// This module ships a minimal login primitive so the platform can
// truthfully say "the login API exists." It is NOT production-ready
// and it is NOT maintained:
//
//   - The ES256 signing key is regenerated at every process restart,
//     which means partners cannot cache the JWKS material and any
//     JWT older than the current process lifetime is unverifiable.
//     That's a feature, not a bug — it prevents anyone from building
//     on top of an unstable interface.
//   - Every response body includes a `warning` field naming the API
//     as experimental and not for integration.
//   - There is no client registration, no consent screen, no refresh
//     tokens, no discovery document, no OAuth flow. This is a bare
//     JWT-issuance primitive with a JWKS endpoint alongside it.
//
// When a real integrator arrives — a specific partner with a specific
// use case — this module gets replaced by a full OIDC implementation
// with persistent keys, key rotation, client registration, consent,
// and everything else the standard requires. Until then it exists to
// prove the interface shape, nothing more.

pub mod jwt;
pub mod keys;
pub mod routes;
pub mod service;
pub mod types;
