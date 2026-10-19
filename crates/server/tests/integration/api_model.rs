//! The HTTP API against a model: any sequence of requests, valid or not, gets the
//! status and body the model predicts. Errors are JSON with a stable `error` code.

use std::collections::{BTreeMap, BTreeSet};

use axum::http::Method;
use bolero::{TypeGenerator, check, produce};
use modhub_core::{ModName, ModVersion};
use serde_json::{Value, json};

use crate::http::{Payload, Reply, TestApi};
use crate::slots::{NameSlot, VersionSlot};

/// A name as a client might send it. Valid half the time (enum variants are drawn
/// evenly), so requests often get past validation to the rules behind it.
#[derive(Debug, Clone, Copy, TypeGenerator)]
enum NameInput {
    Valid(NameSlot),
    Invalid(BadName),
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum BadName {
    Uppercase,
    Underscore,
    DoubleHyphen,
}

impl NameInput {
    fn text(self) -> String {
        match self {
            Self::Valid(slot) => slot.text(),
            Self::Invalid(BadName::Uppercase) => "Mod-a".into(),
            Self::Invalid(BadName::Underscore) => "mod_a".into(),
            Self::Invalid(BadName::DoubleHyphen) => "mod--a".into(),
        }
    }

    fn parsed(self) -> Option<ModName> {
        match self {
            Self::Valid(slot) => Some(slot.name()),
            Self::Invalid(_) => None,
        }
    }
}

/// A version as a client might send it, valid half the time.
#[derive(Debug, Clone, Copy, TypeGenerator)]
enum VersionInput {
    Valid(VersionSlot),
    Invalid(BadVersionText),
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum BadVersionText {
    NotSemver,
    LeadingZero,
    BuildMetadata,
}

impl VersionInput {
    fn text(self) -> String {
        match self {
            Self::Valid(slot) => slot.text(),
            Self::Invalid(BadVersionText::NotSemver) => "latest".into(),
            Self::Invalid(BadVersionText::LeadingZero) => "01.0.0".into(),
            // '+' is not reserved in a path segment, so it reaches the handler as is.
            Self::Invalid(BadVersionText::BuildMetadata) => "1.0.0+nightly".into(),
        }
    }

    fn parsed(self) -> Option<ModVersion> {
        match self {
            Self::Valid(slot) => Some(slot.version()),
            Self::Invalid(_) => None,
        }
    }
}

/// A request body for `POST /mods` that is not a valid registration.
#[derive(Debug, Clone, Copy, TypeGenerator)]
enum BadBody {
    NotJson,
    MissingName,
    NoContentType,
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Step {
    Register {
        name: NameInput,
    },
    RegisterBadBody {
        body: BadBody,
    },
    Publish {
        name: NameInput,
        version: VersionInput,
    },
    /// The same publication twice in a row, so conflicts happen in every scenario that
    /// has one, not only when the generator happens to repeat a version.
    PublishTwice {
        name: NameInput,
        version: VersionInput,
    },
    ListVersions {
        name: NameInput,
    },
}

#[derive(Debug, TypeGenerator)]
struct Scenario {
    /// Registered before the steps, so publications land on existing mods and reach
    /// the rules behind registration (conflicts, ordering) instead of `unknown_mod`.
    #[generator(produce::<Vec<NameSlot>>().with().len(0usize..=4))]
    registered: Vec<NameSlot>,
    #[generator(produce::<Vec<Step>>().with().len(1usize..=16))]
    steps: Vec<Step>,
}

/// What the model predicts: a full body for successes, only the code for errors
/// (messages are for humans and may be reworded).
#[derive(Debug, PartialEq)]
enum Expected {
    Success { status: u16, body: Value },
    Failure { status: u16, error: &'static str },
    Twice(Box<(Expected, Expected)>),
}

impl From<Reply> for Expected {
    fn from(Reply { status, body }: Reply) -> Self {
        if status < 400 {
            return Self::Success { status, body };
        }
        assert!(
            body["message"].is_string(),
            "error without a message: {body}"
        );
        let error = match body["error"].as_str() {
            Some("invalid_name") => "invalid_name",
            Some("invalid_version") => "invalid_version",
            Some("invalid_body") => "invalid_body",
            Some("name_taken") => "name_taken",
            Some("unknown_mod") => "unknown_mod",
            Some("version_exists") => "version_exists",
            other => panic!("unexpected error code {other:?} in {body}"),
        };
        Self::Failure { status, error }
    }
}

fn failure(status: u16, error: &'static str) -> Expected {
    Expected::Failure { status, error }
}

#[derive(Default)]
struct Model(BTreeMap<ModName, BTreeSet<ModVersion>>);

impl Model {
    fn apply(&mut self, step: &Step) -> Expected {
        match *step {
            Step::PublishTwice { name, version } => {
                let first = self.apply(&Step::Publish { name, version });
                let second = self.apply(&Step::Publish { name, version });
                Expected::Twice(Box::new((first, second)))
            }
            Step::Register { name } => {
                let Some(parsed) = name.parsed() else {
                    return failure(422, "invalid_name");
                };
                if self.0.contains_key(&parsed) {
                    return failure(409, "name_taken");
                }
                self.0.insert(parsed, BTreeSet::new());
                Expected::Success {
                    status: 201,
                    body: json!({ "name": name.text() }),
                }
            }
            Step::RegisterBadBody { body } => match body {
                BadBody::NotJson => failure(400, "invalid_body"),
                BadBody::MissingName => failure(422, "invalid_body"),
                BadBody::NoContentType => failure(415, "invalid_body"),
            },
            Step::Publish { name, version } => {
                let Some(parsed_name) = name.parsed() else {
                    return failure(422, "invalid_name");
                };
                let Some(parsed_version) = version.parsed() else {
                    return failure(422, "invalid_version");
                };
                let Some(versions) = self.0.get_mut(&parsed_name) else {
                    return failure(404, "unknown_mod");
                };
                if !versions.insert(parsed_version) {
                    return failure(409, "version_exists");
                }
                Expected::Success {
                    status: 201,
                    body: json!({ "name": name.text(), "version": version.text() }),
                }
            }
            Step::ListVersions { name } => {
                let Some(parsed) = name.parsed() else {
                    return failure(422, "invalid_name");
                };
                match self.0.get(&parsed) {
                    None => failure(404, "unknown_mod"),
                    Some(versions) => Expected::Success {
                        status: 200,
                        body: json!({
                            "name": name.text(),
                            "versions": versions.iter().map(ToString::to_string).collect::<Vec<_>>(),
                        }),
                    },
                }
            }
        }
    }
}

fn run_on_api(api: &TestApi, step: &Step) -> Expected {
    let reply = match *step {
        Step::PublishTwice { name, version } => {
            let first = run_on_api(api, &Step::Publish { name, version });
            let second = run_on_api(api, &Step::Publish { name, version });
            return Expected::Twice(Box::new((first, second)));
        }
        Step::Register { name } => api.send(
            Method::POST,
            "/mods",
            Payload::Json(json!({ "name": name.text() })),
        ),
        Step::RegisterBadBody { body } => {
            let payload = match body {
                BadBody::NotJson => Payload::RawJson("{\"name\": "),
                BadBody::MissingName => Payload::RawJson("{\"title\": \"mod-a\"}"),
                BadBody::NoContentType => Payload::Untyped("{\"name\": \"mod-a\"}"),
            };
            api.send(Method::POST, "/mods", payload)
        }
        Step::Publish { name, version } => api.send(
            Method::PUT,
            &format!("/mods/{}/versions/{}", name.text(), version.text()),
            Payload::Empty,
        ),
        Step::ListVersions { name } => api.send(
            Method::GET,
            &format!("/mods/{}/versions", name.text()),
            Payload::Empty,
        ),
    };
    reply.into()
}

#[test]
fn api_agrees_with_the_model_on_every_request() {
    let api = TestApi::new();
    check!()
        .with_type::<Scenario>()
        .for_each(|Scenario { registered, steps }| {
            api.reset();
            let mut model = Model::default();
            let setup = registered.iter().map(|&slot| Step::Register {
                name: NameInput::Valid(slot),
            });
            for (index, step) in setup.chain(steps.iter().copied()).enumerate() {
                let step = &step;
                assert_eq!(
                    run_on_api(&api, step),
                    model.apply(step),
                    "step {index}: {step:?}"
                );
            }
        });
}
