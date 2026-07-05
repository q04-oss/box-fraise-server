// Schedule domain.
//
// Phase 1: personal_items only — the user's private calendar. Owner-
// scoped RLS is strict: even admins cannot read personal items.
//
// Phase 2 will add salon_appointments (staff-created walk-ins).
// Phase 3 will add consultation_requests (business-owner intake).
// The schema for all four kinds already exists (migration 0003); the
// service and route layers land one phase at a time.

pub mod repository;
pub mod routes;
pub mod service;
pub mod types;
