//! Service configuration, read once from the process environment.

use std::ffi::OsString;
use std::fmt;
use std::net::{AddrParseError, Ipv4Addr, SocketAddr, SocketAddrV4};

use sqlx::postgres::PgConnectOptions;

const DATABASE_URL: &str = "DATABASE_URL";
const LISTEN: &str = "MODHUB_LISTEN";

/// What the server needs to start, validated up front: a bad variable stops startup
/// with a precise error instead of failing on first use.
///
/// | Variable | Required | Meaning |
/// | --- | --- | --- |
/// | `DATABASE_URL` | yes | PostgreSQL URL. Parts it leaves out come from the usual `PG*` variables, as in any PostgreSQL client. |
/// | `MODHUB_LISTEN` | no | `IP:port` to listen on, [`Config::DEFAULT_LISTEN`] by default. |
///
/// `Debug` never shows the database password, so a config is safe to log.
///
/// ```
/// use std::ffi::OsString;
/// use modhub_server::Config;
///
/// let env = |var: &str| (var == "DATABASE_URL").then(|| OsString::from("postgres://modhub:secret@db/modhub"));
/// let config = Config::from_env(env)?;
/// assert_eq!(config.listen(), Config::DEFAULT_LISTEN);
/// assert!(!format!("{config:?}").contains("secret"));
/// # Ok::<(), modhub_server::ConfigError>(())
/// ```
#[derive(Clone)]
pub struct Config {
    database: PgConnectOptions,
    listen: SocketAddr,
}

impl Config {
    /// Loopback only: exposing the service is an explicit decision, not a default.
    pub const DEFAULT_LISTEN: SocketAddr =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8080));

    /// Reads the configuration through `var`, a lookup of one environment variable
    /// (`std::env::var_os` in the binary, a map in tests).
    ///
    /// # Errors
    ///
    /// The first variable that is missing, not UTF-8, or does not parse.
    pub fn from_env(var: impl Fn(&str) -> Option<OsString>) -> Result<Self, ConfigError> {
        let read = |name: &'static str| {
            var(name)
                .map(|value| {
                    value
                        .into_string()
                        .map_err(|_| ConfigError::NotUnicode { var: name })
                })
                .transpose()
        };

        let url = read(DATABASE_URL)?.ok_or(ConfigError::Missing { var: DATABASE_URL })?;
        // sqlx parses any scheme as PostgreSQL: `mysql://…` would silently connect with
        // the wrong protocol. Refuse it here instead.
        if !(url.starts_with("postgres://") || url.starts_with("postgresql://")) {
            return Err(ConfigError::NotPostgres);
        }
        let database = url.parse().map_err(ConfigError::InvalidDatabaseUrl)?;
        let listen = match read(LISTEN)? {
            None => Self::DEFAULT_LISTEN,
            Some(value) => value
                .parse()
                .map_err(|source| ConfigError::InvalidListen { value, source })?,
        };
        Ok(Self { database, listen })
    }

    /// Where the database is and how to log in.
    pub fn database(&self) -> &PgConnectOptions {
        &self.database
    }

    /// The address to listen on.
    pub fn listen(&self) -> SocketAddr {
        self.listen
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Not derived: PgConnectOptions' own Debug prints the password.
        let database = &self.database;
        f.debug_struct("Config")
            .field("database_host", &database.get_host())
            .field("database_port", &database.get_port())
            .field("database_user", &database.get_username())
            .field("database_name", &database.get_database())
            .field("listen", &self.listen)
            .finish_non_exhaustive()
    }
}

/// Why the environment does not describe a usable configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required variable is not set.
    #[error("{var} is not set")]
    Missing {
        /// The variable.
        var: &'static str,
    },
    /// A variable holds bytes that are not UTF-8.
    #[error("{var} is not valid UTF-8")]
    NotUnicode {
        /// The variable.
        var: &'static str,
    },
    /// `DATABASE_URL` does not start with `postgres://` or `postgresql://`.
    #[error("DATABASE_URL must start with postgres:// or postgresql://")]
    NotPostgres,
    /// `DATABASE_URL` is a PostgreSQL URL that does not parse. The URL itself is left
    /// out of the message: it may hold a password.
    #[error("DATABASE_URL is not a valid PostgreSQL URL")]
    InvalidDatabaseUrl(#[source] sqlx::Error),
    /// `MODHUB_LISTEN` is not `IP:port`.
    #[error("MODHUB_LISTEN {value:?} is not an IP address with a port, such as 127.0.0.1:8080")]
    InvalidListen {
        /// The rejected value.
        value: String,
        /// Why it does not parse.
        source: AddrParseError,
    },
}
