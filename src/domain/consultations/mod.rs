// Consultations domain.
//
// Box Fraise's identity credential — the private, in-person
// consultation that verifies a user for the social layer and
// culminates in a physical identity card. Consultation records +
// card issuance travel together and are treated atomically at the
// service layer.
//
// See docs in migration 0005 for the underlying philosophy: identity
// originates inside Box Fraise, not downstream of any external
// document authority.

pub mod repository;
pub mod routes;
pub mod service;
pub mod types;
