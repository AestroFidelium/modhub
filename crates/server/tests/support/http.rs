//! Requests straight into the router: no socket, no port, the whole HTTP stack above it.

use std::io;
use std::ops::Deref;
use std::panic::AssertUnwindSafe;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use crate::database::TestDb;

/// What came back: the status and the body parsed as JSON.
#[derive(Debug, PartialEq)]
pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) body: Value,
}

/// The body of a request under test.
pub(crate) enum Payload {
    Empty,
    Json(Value),
    /// Bytes sent with a JSON content type, whatever they are.
    RawJson(&'static str),
    /// Bytes sent with no content type at all.
    Untyped(&'static str),
    /// A module upload: bytes with the `application/wasm` content type.
    Wasm(Vec<u8>),
    /// Bytes with a content type that is not `application/wasm`.
    WasmAsJson(Vec<u8>),
    /// A module upload whose connection fails after these bytes.
    WasmInterrupted(Vec<u8>),
}

/// A response whose body may not be JSON (a module download).
#[derive(Debug)]
pub(crate) struct Fetched {
    pub(crate) status: u16,
    pub(crate) headers: HeaderMap,
    pub(crate) bytes: Vec<u8>,
}

/// The API over a throwaway database. Derefs to the [`TestDb`] for direct store and
/// SQL access.
pub(crate) struct TestApi {
    db: AssertUnwindSafe<TestDb>,
    router: Router,
}

impl TestApi {
    /// `AssertUnwindSafe` for the same reason as [`TestDb::new`]: the router holds boxed
    /// services that are not `RefUnwindSafe`, and nothing survives a panic but rows,
    /// which every iteration resets.
    pub(crate) fn new() -> AssertUnwindSafe<Self> {
        let db = TestDb::new();
        let router = modhub_server::router(db.store().clone());
        AssertUnwindSafe(Self { db, router })
    }

    pub(crate) fn send(&self, method: Method, uri: &str, payload: Payload) -> Reply {
        let Fetched { status, bytes, .. } = self.fetch(method, uri, payload);
        let body = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| panic!("non-JSON body: {:?}", String::from_utf8_lossy(&bytes)));
        Reply { status, body }
    }

    pub(crate) fn fetch(&self, method: Method, uri: &str, payload: Payload) -> Fetched {
        let request = Request::builder().method(method).uri(uri);
        let request = match payload {
            Payload::Empty => request.body(Body::empty()),
            Payload::Json(value) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(value.to_string())),
            Payload::RawJson(text) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(text)),
            Payload::Untyped(text) => request.body(Body::from(text)),
            Payload::Wasm(bytes) => request
                .header(header::CONTENT_TYPE, "application/wasm")
                .body(Body::from(bytes)),
            Payload::WasmAsJson(bytes) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(bytes)),
            Payload::WasmInterrupted(bytes) => {
                let chunks: [Result<Vec<u8>, io::Error>; 2] = [
                    Ok(bytes),
                    Err(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        "client went away",
                    )),
                ];
                request
                    .header(header::CONTENT_TYPE, "application/wasm")
                    .body(Body::from_stream(futures_util::stream::iter(chunks)))
            }
        }
        .unwrap();

        self.db.run(async {
            let response = self.router.clone().oneshot(request).await.unwrap();
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            Fetched {
                status,
                headers,
                bytes: bytes.to_vec(),
            }
        })
    }
}

impl Deref for TestApi {
    type Target = TestDb;

    fn deref(&self) -> &TestDb {
        &self.db
    }
}
