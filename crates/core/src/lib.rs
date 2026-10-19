//! The pure domain of modhub: the values a mod registry reasons about and the rules
//! they obey. No IO lives here; the server crate is a thin shell around these types.

mod mod_name;
mod mod_version;

pub use mod_name::{ModName, ModNameError};
pub use mod_version::{ModVersion, ModVersionError};
