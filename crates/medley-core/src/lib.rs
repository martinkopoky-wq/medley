//! Core building blocks shared by every Medley crate.

pub mod layout;
pub mod matching;
pub mod model;
pub mod oauth;
pub mod provider;
pub mod store;

pub use model::*;
pub use provider::{Provider, StreamSource};

/// User-Agent for all outgoing requests.
pub const USER_AGENT: &str =
    concat!("Medley/", env!("CARGO_PKG_VERSION"), " (+https://github.com/martinkopoky-wq/medley)");
