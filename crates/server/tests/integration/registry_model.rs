//! The store against an in-memory model of the registry: any sequence of registrations,
//! publications, listings and downloads gives the same outcomes on PostgreSQL as on a
//! map.

use std::collections::BTreeMap;
use std::fmt::Write;

use bolero::{TypeGenerator, check, produce};
use modhub_core::{ModModule, ModName, ModVersion};
use modhub_server::StoreError;
use sha2::{Digest, Sha256};

use crate::database::TestDb;
use crate::modules::module_for;
use crate::slots::{NameSlot, VersionSlot};

#[derive(Debug, TypeGenerator)]
enum Step {
    Register {
        name: NameSlot,
    },
    Publish {
        name: NameSlot,
        version: VersionSlot,
    },
    ListVersions {
        name: NameSlot,
    },
    Download {
        name: NameSlot,
        version: VersionSlot,
    },
}

#[derive(Debug, TypeGenerator)]
struct Scenario {
    #[generator(produce::<Vec<Step>>().with().len(1usize..=16))]
    steps: Vec<Step>,
}

/// What a step did, comparable between the store and the model.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Done,
    /// A publication, with the digest the store reports.
    Published(String),
    Versions(Vec<ModVersion>),
    /// A download: the bytes and the digest that came with them.
    Module {
        bytes: Vec<u8>,
        sha256: String,
    },
    NameTaken,
    UnknownMod,
    UnknownVersion,
    VersionExists,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            write!(hex, "{byte:02x}").unwrap();
            hex
        })
}

impl Outcome {
    fn from_error(error: StoreError) -> Self {
        match error {
            StoreError::NameTaken { .. } => Self::NameTaken,
            StoreError::UnknownMod { .. } => Self::UnknownMod,
            StoreError::VersionExists { .. } => Self::VersionExists,
            StoreError::UnknownVersion { .. } => Self::UnknownVersion,
            other => panic!("unexpected store error: {other}"),
        }
    }
}

/// The registry as a map from name to its published versions (ordered by precedence)
/// and their modules.
#[derive(Default)]
struct Model(BTreeMap<ModName, BTreeMap<ModVersion, Vec<u8>>>);

impl Model {
    fn apply(&mut self, step: &Step) -> Outcome {
        match *step {
            Step::Register { name } => {
                if self.0.contains_key(&name.name()) {
                    return Outcome::NameTaken;
                }
                self.0.insert(name.name(), BTreeMap::new());
                Outcome::Done
            }
            Step::Publish { name, version } => match self.0.get_mut(&name.name()) {
                None => Outcome::UnknownMod,
                Some(versions) if versions.contains_key(&version.version()) => {
                    Outcome::VersionExists
                }
                Some(versions) => {
                    let module = module_for(version);
                    let digest = sha256_hex(&module);
                    versions.insert(version.version(), module);
                    Outcome::Published(digest)
                }
            },
            Step::ListVersions { name } => self
                .0
                .get(&name.name())
                .map_or(Outcome::UnknownMod, |versions| {
                    Outcome::Versions(versions.keys().cloned().collect())
                }),
            Step::Download { name, version } => match self.0.get(&name.name()) {
                None => Outcome::UnknownMod,
                Some(versions) => {
                    versions
                        .get(&version.version())
                        .map_or(Outcome::UnknownVersion, |module| Outcome::Module {
                            bytes: module.clone(),
                            sha256: sha256_hex(module),
                        })
                }
            },
        }
    }
}

fn run_on_store(db: &TestDb, step: &Step) -> Outcome {
    let store = db.store();
    match *step {
        Step::Register { name } => db
            .run(store.register(&name.name()))
            .map_or_else(Outcome::from_error, |()| Outcome::Done),
        Step::Publish { name, version } => {
            let bytes = module_for(version);
            let module = ModModule::check(&bytes).unwrap();
            db.run(store.publish(&name.name(), &version.version(), &module))
                .map_or_else(Outcome::from_error, |digest| {
                    Outcome::Published(digest.to_string())
                })
        }
        Step::ListVersions { name } => db
            .run(store.versions(&name.name()))
            .map_or_else(Outcome::from_error, Outcome::Versions),
        Step::Download { name, version } => db
            .run(store.download(&name.name(), &version.version()))
            .map_or_else(Outcome::from_error, |stored| Outcome::Module {
                sha256: stored.sha256().to_string(),
                bytes: stored.into_bytes(),
            }),
    }
}

#[test]
fn store_agrees_with_the_model_on_every_step() {
    let db = TestDb::new();
    check!()
        .with_type::<Scenario>()
        .for_each(|Scenario { steps }| {
            db.reset();
            let mut model = Model::default();
            for (index, step) in steps.iter().enumerate() {
                assert_eq!(
                    run_on_store(&db, step),
                    model.apply(step),
                    "step {index}: {step:?}"
                );
            }
        });
}
