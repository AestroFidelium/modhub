//! Parsers of external input on arbitrary data: they never panic and never accept
//! what the rule forbids. Long campaigns: `cargo bolero test --test fuzz <name> -T 10min`.

#[cfg(test)]
mod mod_name;
