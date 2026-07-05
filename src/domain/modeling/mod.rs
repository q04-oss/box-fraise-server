// Modeling — hair profiles + student model requests.
//
// Captured at consultation time: the user's hair characteristics and
// whether they consent to being contacted as a practice model, plus
// whether they themselves are a hair student.
//
// When a student creates a model_request with a time / date / location
// and hair criteria, the server fans out invitations to every
// willing-to-model user whose hair matches. Each matched user can
// accept or decline; the first accept fills the request. Accepted
// invitations render alongside personal items on the model's /my
// calendar view — no cross-context write into personal_items
// required, which keeps the RLS story clean.

pub mod repository;
pub mod routes;
pub mod service;
pub mod types;
