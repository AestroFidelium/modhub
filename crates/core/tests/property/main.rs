//! Laws of the registry's domain values.
//!
//! Modules are `#[cfg(test)]` so clippy treats their helpers as test code (`clippy.toml`
//! lets tests unwrap, index and panic).

#[cfg(test)]
#[path = "../support/names.rs"]
mod names;

#[cfg(test)]
mod mod_name;
