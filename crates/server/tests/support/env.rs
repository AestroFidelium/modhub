//! Process environments for `Config::from_env`, built from parts so a test can compare
//! the parsed config against the parts.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::net::{Ipv4Addr, SocketAddr};

use bolero::{Driver, TypeGenerator, ValueGenerator};

/// An environment: variable name → raw value. Lookups of absent names give `None`.
#[derive(Debug, Default)]
pub(crate) struct Env(BTreeMap<&'static str, OsString>);

impl Env {
    pub(crate) fn set(&mut self, var: &'static str, value: impl Into<OsString>) {
        self.0.insert(var, value.into());
    }

    pub(crate) fn lookup(&self) -> impl Fn(&str) -> Option<OsString> + '_ {
        |var| self.0.get(var).cloned()
    }
}

/// One to eight lowercase letters: user names, database names.
#[derive(Debug, Clone)]
pub(crate) struct Word(pub(crate) String);

impl TypeGenerator for Word {
    fn generate<D: Driver>(driver: &mut D) -> Option<Self> {
        let len = (1usize..=8).generate(driver)?;
        let word = (0..len)
            .map(|_| Some(char::from((b'a'..=b'z').generate(driver)?)))
            .collect::<Option<_>>()?;
        Some(Self(word))
    }
}

/// A password that cannot occur by accident anywhere else in a config's text: it
/// starts with `Pw0`, and every other generated part is lowercase or digits only.
#[derive(Debug, Clone)]
pub(crate) struct Password(pub(crate) String);

impl TypeGenerator for Password {
    fn generate<D: Driver>(driver: &mut D) -> Option<Self> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let len = (5usize..=13).generate(driver)?;
        let mut password = String::from("Pw0");
        for _ in 0..len {
            let index = (0..ALPHABET.len()).generate(driver)?;
            password.push(char::from(*ALPHABET.get(index)?));
        }
        Some(Self(password))
    }
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) enum Host {
    Localhost,
    Loopback,
    Named,
}

impl Host {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Localhost => "localhost",
            Self::Loopback => "127.0.0.1",
            Self::Named => "db.internal",
        }
    }
}

/// Both spellings PostgreSQL accepts.
#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) enum Scheme {
    Postgres,
    Postgresql,
}

/// A valid PostgreSQL URL with every part spelled out.
#[derive(Debug, Clone, TypeGenerator)]
pub(crate) struct DatabaseUrl {
    pub(crate) scheme: Scheme,
    pub(crate) user: Word,
    pub(crate) password: Password,
    pub(crate) host: Host,
    #[generator(1u16..)]
    pub(crate) port: u16,
    pub(crate) database: Word,
}

impl DatabaseUrl {
    pub(crate) fn text(&self) -> String {
        let scheme = match self.scheme {
            Scheme::Postgres => "postgres",
            Scheme::Postgresql => "postgresql",
        };
        format!(
            "{scheme}://{}:{}@{}:{}/{}",
            self.user.0,
            self.password.0,
            self.host.text(),
            self.port,
            self.database.0
        )
    }
}

/// A valid IPv4 listen address.
#[derive(Debug, Clone, Copy, TypeGenerator)]
pub(crate) struct ListenAddr {
    octets: [u8; 4],
    port: u16,
}

impl ListenAddr {
    pub(crate) fn addr(self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::from(self.octets), self.port))
    }
}
