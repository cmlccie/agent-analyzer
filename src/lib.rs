//! Agent Analyzer — Kubernetes-native AI component discovery and reachability analyzer.
//!
//! The library is organized into two top-level modules:
//!
//! * [`core`] — domain types, discovery, probing, server, and client logic.
//! * [`cli`] — command-line interface built on top of `core`.

pub mod cli;
pub mod core;

pub use core::errors::{Error, Result};
