//! The modhub service: a thin shell of IO around `modhub-core`.

mod store;

pub use store::{Store, StoreError};
