//! The HTTP API against a model: any sequence of requests, valid or not, gets the
//! status and body the model predicts. Errors are JSON with a stable `error` code.

use std::collections::BTreeMap;
use std::fmt::Write;

use axum::http::Method;
use bolero::{TypeGenerator, check, produce};
use modhub_core::{ModModule, ModName, ModVersion};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::http::{Fetched, Payload, Reply, TestApi};
use crate::modules::{EXAMPLE_MOD, module_for};
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

/// The body of a module upload: the right module half the time, or one way to get it
/// wrong, in the order the API checks them.
#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Upload {
    Valid,
    Invalid(BadUpload),
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum BadUpload {
    /// One byte over the limit: refused before the body is even read in full.
    Oversized,
    /// The connection breaks halfway through the body. The rest of the module may
    /// still parse as a smaller valid one, so this must be caught by the transport.
    Interrupted,
    WrongContentType,
    Empty,
    NotWasm,
    /// A well-formed module that imports a host function.
    ImportsHostFunction,
}

/// `(module (import "env" "f" (func)))`: valid WebAssembly, against the mod contract.
const IMPORTING_MODULE: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
    0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // type section: () -> ()
    0x02, 0x09, 0x01, 0x03, b'e', b'n', b'v', 0x01, b'f', 0x00, 0x00, // import env::f
];

impl Upload {
    /// The bytes and how they are sent. A valid upload is the example mod tagged with
    /// the version, so each release has its own bytes.
    fn payload(self, version: VersionInput) -> Payload {
        match self {
            Self::Valid => Payload::Wasm(match version {
                VersionInput::Valid(slot) => module_for(slot),
                VersionInput::Invalid(_) => EXAMPLE_MOD.to_vec(),
            }),
            Self::Invalid(BadUpload::Oversized) => Payload::Wasm(vec![0; ModModule::MAX_BYTES + 1]),
            Self::Invalid(BadUpload::Interrupted) => {
                Payload::WasmInterrupted(EXAMPLE_MOD[..EXAMPLE_MOD.len() / 2].to_vec())
            }
            Self::Invalid(BadUpload::WrongContentType) => Payload::WasmAsJson(EXAMPLE_MOD.to_vec()),
            Self::Invalid(BadUpload::Empty) => Payload::Wasm(Vec::new()),
            Self::Invalid(BadUpload::NotWasm) => Payload::Wasm(b"not a module".to_vec()),
            Self::Invalid(BadUpload::ImportsHostFunction) => {
                Payload::Wasm(IMPORTING_MODULE.to_vec())
            }
        }
    }
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
        upload: Upload,
    },
    /// The same publication twice in a row, so conflicts happen in every scenario that
    /// has one, not only when the generator happens to repeat a version.
    PublishTwice {
        name: NameInput,
        version: VersionInput,
    },
    /// A publication and an immediate download of it: what comes back must be exactly
    /// what went in. Rarely lined up by chance across steps.
    PublishThenDownload {
        name: NameInput,
        version: VersionInput,
    },
    ListVersions {
        name: NameInput,
    },
    Download {
        name: NameInput,
        version: VersionInput,
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
    Success {
        status: u16,
        body: Value,
    },
    Failure {
        status: u16,
        error: &'static str,
    },
    Twice(Box<(Expected, Expected)>),
    /// A download: the bytes and the digest its `ETag` carries.
    Module {
        sha256: String,
        bytes: Vec<u8>,
    },
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
            Some("invalid_module") => "invalid_module",
            Some("too_large") => "too_large",
            Some("name_taken") => "name_taken",
            Some("unknown_mod") => "unknown_mod",
            Some("unknown_version") => "unknown_version",
            Some("version_exists") => "version_exists",
            other => panic!("unexpected error code {other:?} in {body}"),
        };
        Self::Failure { status, error }
    }
}

fn failure(status: u16, error: &'static str) -> Expected {
    Expected::Failure { status, error }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            write!(hex, "{byte:02x}").unwrap();
            hex
        })
}

/// Registered mods, each with its published versions (precedence order) and modules.
#[derive(Default)]
struct Model(BTreeMap<ModName, BTreeMap<ModVersion, Vec<u8>>>);

impl Model {
    fn apply(&mut self, step: &Step) -> Expected {
        match *step {
            Step::PublishTwice { name, version } => {
                let publish = Step::Publish {
                    name,
                    version,
                    upload: Upload::Valid,
                };
                let first = self.apply(&publish);
                let second = self.apply(&publish);
                Expected::Twice(Box::new((first, second)))
            }
            Step::PublishThenDownload { name, version } => {
                let published = self.apply(&Step::Publish {
                    name,
                    version,
                    upload: Upload::Valid,
                });
                let downloaded = self.apply(&Step::Download { name, version });
                Expected::Twice(Box::new((published, downloaded)))
            }
            Step::Register { name } => {
                let Some(parsed) = name.parsed() else {
                    return failure(422, "invalid_name");
                };
                if self.0.contains_key(&parsed) {
                    return failure(409, "name_taken");
                }
                self.0.insert(parsed, BTreeMap::new());
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
            Step::Publish {
                name,
                version,
                upload,
            } => self.publish(name, version, upload),
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
                            "versions": versions.keys().map(ToString::to_string).collect::<Vec<_>>(),
                        }),
                    },
                }
            }
            Step::Download { name, version } => {
                let Some(parsed_name) = name.parsed() else {
                    return failure(422, "invalid_name");
                };
                let Some(parsed_version) = version.parsed() else {
                    return failure(422, "invalid_version");
                };
                let Some(versions) = self.0.get(&parsed_name) else {
                    return failure(404, "unknown_mod");
                };
                versions.get(&parsed_version).map_or_else(
                    || failure(404, "unknown_version"),
                    |module| Expected::Module {
                        sha256: sha256_hex(module),
                        bytes: module.clone(),
                    },
                )
            }
        }
    }

    fn publish(&mut self, name: NameInput, version: VersionInput, upload: Upload) -> Expected {
        // The body is checked as it is read, before the handler looks at the path.
        match upload {
            Upload::Invalid(BadUpload::Oversized) => return failure(413, "too_large"),
            Upload::Invalid(BadUpload::Interrupted) => return failure(400, "invalid_body"),
            Upload::Invalid(BadUpload::WrongContentType) => return failure(415, "invalid_body"),
            _ => {}
        }
        let Some(parsed_name) = name.parsed() else {
            return failure(422, "invalid_name");
        };
        let Some(parsed_version) = version.parsed() else {
            return failure(422, "invalid_version");
        };
        let Upload::Valid = upload else {
            return failure(422, "invalid_module");
        };
        let Some(versions) = self.0.get_mut(&parsed_name) else {
            return failure(404, "unknown_mod");
        };
        if versions.contains_key(&parsed_version) {
            return failure(409, "version_exists");
        }
        let VersionInput::Valid(slot) = version else {
            unreachable!("an invalid version was refused above");
        };
        let module = module_for(slot);
        let body = json!({
            "name": name.text(),
            "version": version.text(),
            "sha256": sha256_hex(&module),
            "size": module.len(),
            "entry_points": ["handle", "trigger", "tick"],
        });
        versions.insert(parsed_version, module);
        Expected::Success { status: 201, body }
    }
}

fn run_on_api(api: &TestApi, step: &Step) -> Expected {
    let reply = match *step {
        Step::PublishTwice { name, version } => {
            let publish = Step::Publish {
                name,
                version,
                upload: Upload::Valid,
            };
            let first = run_on_api(api, &publish);
            let second = run_on_api(api, &publish);
            return Expected::Twice(Box::new((first, second)));
        }
        Step::PublishThenDownload { name, version } => {
            let published = run_on_api(
                api,
                &Step::Publish {
                    name,
                    version,
                    upload: Upload::Valid,
                },
            );
            let downloaded = run_on_api(api, &Step::Download { name, version });
            return Expected::Twice(Box::new((published, downloaded)));
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
        Step::Publish {
            name,
            version,
            upload,
        } => api.send(
            Method::PUT,
            &format!("/mods/{}/versions/{}", name.text(), version.text()),
            upload.payload(version),
        ),
        Step::ListVersions { name } => api.send(
            Method::GET,
            &format!("/mods/{}/versions", name.text()),
            Payload::Empty,
        ),
        Step::Download { name, version } => {
            let fetched = api.fetch(
                Method::GET,
                &format!("/mods/{}/versions/{}", name.text(), version.text()),
                Payload::Empty,
            );
            if fetched.status == 200 {
                return downloaded(&fetched);
            }
            Reply {
                status: fetched.status,
                body: serde_json::from_slice(&fetched.bytes).unwrap(),
            }
        }
    };
    reply.into()
}

/// A successful download: the module, served as an immutable `application/wasm` with
/// its digest as the `ETag`.
fn downloaded(fetched: &Fetched) -> Expected {
    let header = |name: &str| fetched.headers[name].to_str().unwrap();
    assert_eq!(header("content-type"), "application/wasm");
    assert_eq!(
        header("cache-control"),
        "public, max-age=31536000, immutable"
    );
    let etag = header("etag");
    let sha256 = etag.trim_matches('"');
    assert_eq!(etag, format!("\"{sha256}\""), "a strong ETag is quoted");
    Expected::Module {
        sha256: sha256.into(),
        bytes: fetched.bytes.clone(),
    }
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
