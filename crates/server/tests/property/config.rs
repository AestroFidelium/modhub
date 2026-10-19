//! Configuration from the environment: what parses into what, which error each broken
//! variable gives, and that the database password never reaches `Debug` output.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

use bolero::{TypeGenerator, check};
use modhub_server::{Config, ConfigError};

use crate::env::{DatabaseUrl, Env, ListenAddr};

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Listen {
    Unset,
    Set(ListenAddr),
}

/// A complete, valid environment.
#[derive(Debug, TypeGenerator)]
struct ValidEnv {
    database: DatabaseUrl,
    listen: Listen,
}

impl ValidEnv {
    fn env(&self) -> Env {
        let mut env = Env::default();
        env.set("DATABASE_URL", self.database.text());
        if let Listen::Set(addr) = self.listen {
            env.set("MODHUB_LISTEN", addr.addr().to_string());
        }
        env
    }
}

#[test]
fn valid_environment_parses_into_its_parts() {
    check!().with_type::<ValidEnv>().for_each(|valid| {
        let config = Config::from_env(valid.env().lookup()).unwrap();
        let database = config.database();
        assert_eq!(database.get_username(), valid.database.user.0);
        assert_eq!(database.get_host(), valid.database.host.text());
        assert_eq!(database.get_port(), valid.database.port);
        assert_eq!(
            database.get_database(),
            Some(valid.database.database.0.as_str())
        );
        let expected_listen = match valid.listen {
            Listen::Unset => Config::DEFAULT_LISTEN,
            Listen::Set(addr) => addr.addr(),
        };
        assert_eq!(config.listen(), expected_listen);
    });
}

#[test]
fn debug_output_never_contains_the_password() {
    check!().with_type::<ValidEnv>().for_each(|valid| {
        let config = Config::from_env(valid.env().lookup()).unwrap();
        let debug = format!("{config:?} {config:#?}");
        assert!(!debug.contains(&valid.database.password.0), "{debug}");
        assert!(debug.contains(valid.database.host.text()), "{debug}");
    });
}

/// One way to break an otherwise valid environment.
#[derive(Debug, TypeGenerator)]
enum Breakage {
    DatabaseUrlUnset,
    DatabaseUrlNotUnicode,
    DatabaseUrlNotPostgres(BadUrl),
    ListenNotUnicode,
    ListenNotAnAddress(BadListen),
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum BadUrl {
    Empty,
    NoScheme,
    OtherScheme,
    BadPort,
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum BadListen {
    Empty,
    PortOnly,
    Hostname,
}

/// A byte sequence that is not UTF-8.
fn not_unicode() -> OsString {
    OsString::from_vec(vec![b'p', 0xff, b'g'])
}

#[derive(Debug, TypeGenerator)]
struct Broken {
    base: ValidEnv,
    breakage: Breakage,
}

impl Broken {
    fn env(&self) -> Env {
        let mut env = self.base.env();
        match self.breakage {
            Breakage::DatabaseUrlUnset => env = Env::default(),
            Breakage::DatabaseUrlNotUnicode => env.set("DATABASE_URL", not_unicode()),
            Breakage::DatabaseUrlNotPostgres(bad) => env.set(
                "DATABASE_URL",
                match bad {
                    BadUrl::Empty => "",
                    BadUrl::NoScheme => "localhost/modhub",
                    BadUrl::OtherScheme => "mysql://localhost/modhub",
                    BadUrl::BadPort => "postgres://localhost:port/modhub",
                },
            ),
            Breakage::ListenNotUnicode => env.set("MODHUB_LISTEN", not_unicode()),
            Breakage::ListenNotAnAddress(bad) => env.set(
                "MODHUB_LISTEN",
                match bad {
                    BadListen::Empty => "",
                    BadListen::PortOnly => "8080",
                    BadListen::Hostname => "localhost:8080",
                },
            ),
        }
        env
    }

    fn matches(&self, error: &ConfigError) -> bool {
        match self.breakage {
            Breakage::DatabaseUrlUnset => {
                matches!(
                    error,
                    ConfigError::Missing {
                        var: "DATABASE_URL"
                    }
                )
            }
            Breakage::DatabaseUrlNotUnicode => {
                matches!(
                    error,
                    ConfigError::NotUnicode {
                        var: "DATABASE_URL"
                    }
                )
            }
            Breakage::DatabaseUrlNotPostgres(BadUrl::BadPort) => {
                matches!(error, ConfigError::InvalidDatabaseUrl(_))
            }
            Breakage::DatabaseUrlNotPostgres(_) => matches!(error, ConfigError::NotPostgres),
            Breakage::ListenNotUnicode => {
                matches!(
                    error,
                    ConfigError::NotUnicode {
                        var: "MODHUB_LISTEN"
                    }
                )
            }
            Breakage::ListenNotAnAddress(_) => {
                matches!(error, ConfigError::InvalidListen { .. })
            }
        }
    }
}

#[test]
fn each_broken_variable_gives_its_own_error() {
    check!().with_type::<Broken>().for_each(|broken| {
        let error = Config::from_env(broken.env().lookup()).unwrap_err();
        assert!(broken.matches(&error), "{broken:?} gave {error:?}");
    });
}
