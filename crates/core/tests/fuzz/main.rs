//! Parsers of external input on arbitrary data: they never panic and never accept
//! what the rule forbids. Long campaigns: `cargo bolero test --test fuzz <name> -T 10min`.

#[cfg(test)]
#[path = "../support/modules.rs"]
mod modules;

#[cfg(test)]
mod mod_module;
#[cfg(test)]
mod mod_name;
#[cfg(test)]
mod mod_version;
