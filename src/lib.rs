// box-fraise as a library: the binary in src/main.rs is a thin shim
// around these modules so integration tests in tests/ can exercise
// the domain services and primitives directly.

pub mod app;
pub mod audit;
pub mod config;
pub mod crypto;
pub mod db;
pub mod domain;
pub mod error;
pub mod http;
pub mod maintenance;
