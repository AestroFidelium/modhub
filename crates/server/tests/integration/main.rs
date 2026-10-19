//! The store on a real PostgreSQL (the `tools/pg` cluster; `tools/gate` starts it).
//!
//! Modules are `#[cfg(test)]` so clippy treats their helpers as test code (`clippy.toml`
//! lets tests unwrap, index and panic).

#[cfg(test)]
#[path = "../support/database.rs"]
mod database;
#[cfg(test)]
#[path = "../support/http.rs"]
mod http;
#[cfg(test)]
#[path = "../support/slots.rs"]
mod slots;

#[cfg(test)]
mod api_failure;
#[cfg(test)]
mod api_model;
#[cfg(test)]
mod corrupt_version;
#[cfg(test)]
mod database_failure;
#[cfg(test)]
mod name_rule;
#[cfg(test)]
mod registry_model;
