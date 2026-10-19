//! `Config::from_env` on arbitrary bytes in both variables: it never panics.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

use bolero::{TypeGenerator, check};
use modhub_server::Config;

/// Raw values of the two variables, absent or any bytes at all.
#[derive(Debug, TypeGenerator)]
struct RawEnv {
    database_url: Option<Vec<u8>>,
    listen: Option<Vec<u8>>,
}

#[test]
fn parsing_is_total_on_arbitrary_environments() {
    check!().with_type::<RawEnv>().for_each(|raw| {
        let lookup = |var: &str| {
            let value = match var {
                "DATABASE_URL" => raw.database_url.as_ref(),
                "MODHUB_LISTEN" => raw.listen.as_ref(),
                _ => None,
            };
            value.map(|bytes| OsString::from_vec(bytes.clone()))
        };
        let _ = Config::from_env(lookup);
    });
}
