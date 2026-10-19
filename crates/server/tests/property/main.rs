//! Laws of the server's pure parts: configuration.
//!
//! Modules are `#[cfg(test)]` so clippy treats their helpers as test code (`clippy.toml`
//! lets tests unwrap, index and panic).

#[cfg(test)]
#[path = "../support/env.rs"]
mod env;

#[cfg(test)]
mod config;
