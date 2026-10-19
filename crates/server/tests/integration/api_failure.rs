//! Store failures surface as `500 internal` with a generic message: the client learns
//! that something broke on our side, not how (no SQL, no row contents).

use std::sync::{Arc, Mutex};

use axum::http::Method;
use bolero::{TypeGenerator, check};
use serde_json::json;

use crate::http::{Payload, Reply, TestApi};
use crate::modules::EXAMPLE_MOD;
use crate::slots::BadVersion;

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Request {
    Register,
    Publish,
    ListVersions,
    Download,
}

fn send_request(api: &TestApi, request: Request) -> Reply {
    match request {
        Request::Register => api.send(
            Method::POST,
            "/mods",
            Payload::Json(json!({ "name": "cloth" })),
        ),
        Request::Publish => api.send(
            Method::PUT,
            "/mods/cloth/versions/1.0.0",
            Payload::Wasm(EXAMPLE_MOD.to_vec()),
        ),
        Request::Download => api.send(Method::GET, "/mods/cloth/versions/1.0.0", Payload::Empty),
        Request::ListVersions => api.send(Method::GET, "/mods/cloth/versions", Payload::Empty),
    }
}

/// Sends `request` with a subscriber installed; returns the reply and the log.
fn send_logged(api: &TestApi, request: Request) -> (Reply, String) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let writer = Arc::clone(&log);
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || Writer(Arc::clone(&writer)))
        .finish();
    // The test runtime is single-threaded, so the thread-local default sees every event.
    let reply = tracing::subscriber::with_default(subscriber, || send_request(api, request));
    let log = String::from_utf8(log.lock().unwrap().clone()).unwrap();
    (reply, log)
}

struct Writer(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn internal() -> Reply {
    Reply {
        status: 500,
        body: json!({ "error": "internal", "message": "internal server error" }),
    }
}

#[test]
fn unreachable_database_gives_internal_error() {
    let api = TestApi::new();
    api.run(api.pool().close());
    check!().with_type::<Request>().for_each(|request| {
        let (reply, log) = send_logged(&api, *request);
        assert_eq!(reply, internal(), "{request:?}");
        // The client gets nothing; the operator must get the cause.
        assert!(log.contains("request failed in the store"), "{log}");
    });
}

#[test]
fn corrupt_stored_version_gives_internal_error() {
    let api = TestApi::new();
    check!().with_type::<BadVersion>().for_each(|bad| {
        api.reset();
        api.run(api.store().register(&"cloth".parse().unwrap()))
            .unwrap();
        api.run(
            sqlx::query(
                "INSERT INTO releases (mod_id, version, module, sha256) \
                 SELECT id, $1, '\\x00', sha256('\\x00') FROM mods",
            )
            .bind(bad.text())
            .execute(api.pool()),
        )
        .unwrap();
        assert_eq!(
            send_request(&api, Request::ListVersions),
            internal(),
            "{bad:?}"
        );
    });
}
