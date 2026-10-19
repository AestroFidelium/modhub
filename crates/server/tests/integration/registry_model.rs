//! The store against an in-memory model of the registry: any sequence of registrations,
//! publications and listings gives the same outcomes on PostgreSQL as on a map.

use std::collections::{BTreeMap, BTreeSet};

use bolero::{TypeGenerator, check, produce};
use modhub_core::{ModName, ModVersion};
use modhub_server::StoreError;

use crate::database::TestDb;

/// One of four fixed names, so registrations and publications collide often.
#[derive(Debug, Clone, Copy, TypeGenerator)]
struct NameSlot(#[generator(0u8..4)] u8);

impl NameSlot {
    fn name(self) -> ModName {
        format!("mod-{}", char::from(b'a' + self.0))
            .parse()
            .unwrap()
    }
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Stage {
    Release,
    Beta,
}

/// A version from a small grid, so duplicate publications happen.
#[derive(Debug, Clone, Copy, TypeGenerator)]
struct VersionSlot {
    #[generator(0u64..=2)]
    major: u64,
    #[generator(0u64..=2)]
    minor: u64,
    stage: Stage,
}

impl VersionSlot {
    fn version(self) -> ModVersion {
        let suffix = match self.stage {
            Stage::Release => "",
            Stage::Beta => "-beta",
        };
        format!("{}.{}.0{suffix}", self.major, self.minor)
            .parse()
            .unwrap()
    }
}

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
    Versions(Vec<ModVersion>),
    NameTaken,
    UnknownMod,
    VersionExists,
}

impl From<Result<(), StoreError>> for Outcome {
    fn from(result: Result<(), StoreError>) -> Self {
        result.map_or_else(Self::from_error, |()| Self::Done)
    }
}

impl Outcome {
    fn from_error(error: StoreError) -> Self {
        match error {
            StoreError::NameTaken { .. } => Self::NameTaken,
            StoreError::UnknownMod { .. } => Self::UnknownMod,
            StoreError::VersionExists { .. } => Self::VersionExists,
            other => panic!("unexpected store error: {other}"),
        }
    }
}

/// The registry as a map from name to its published versions, ordered by precedence.
#[derive(Default)]
struct Model(BTreeMap<ModName, BTreeSet<ModVersion>>);

impl Model {
    fn apply(&mut self, step: &Step) -> Outcome {
        match *step {
            Step::Register { name } => {
                if self.0.contains_key(&name.name()) {
                    return Outcome::NameTaken;
                }
                self.0.insert(name.name(), BTreeSet::new());
                Outcome::Done
            }
            Step::Publish { name, version } => match self.0.get_mut(&name.name()) {
                None => Outcome::UnknownMod,
                Some(versions) => {
                    if versions.insert(version.version()) {
                        Outcome::Done
                    } else {
                        Outcome::VersionExists
                    }
                }
            },
            Step::ListVersions { name } => self
                .0
                .get(&name.name())
                .map_or(Outcome::UnknownMod, |versions| {
                    Outcome::Versions(versions.iter().cloned().collect())
                }),
        }
    }
}

fn run_on_store(db: &TestDb, step: &Step) -> Outcome {
    let store = db.store();
    match *step {
        Step::Register { name } => db.run(store.register(&name.name())).into(),
        Step::Publish { name, version } => db
            .run(store.publish(&name.name(), &version.version()))
            .into(),
        Step::ListVersions { name } => db
            .run(store.versions(&name.name()))
            .map_or_else(Outcome::from_error, Outcome::Versions),
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
