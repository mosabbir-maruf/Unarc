//! # Unarc
//!
//! A production-grade, security-focused archive utility.
//!
//! Designed with zero-trust extraction defaults, targeting macOS Apple Silicon
//! and Linux environments with reproducible containerized development.

pub mod archive;
pub mod cli;
pub mod core;
pub mod error;
pub mod platform;
pub mod security;

pub use cli::run;
pub use error::{Result, UnarcError};
