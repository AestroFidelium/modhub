//! Store failures surface as `500 internal` with a generic message: the client learns
//! that something broke on our side, not how (no SQL, no row contents).

use axum::http::Method;
use bolero::{TypeGenerator, check};
use serde_json::json;

use crate::http::{Payload, Reply, TestApi};
use crate::slots::BadVersion;

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Request {
    Register,
    Publish,
    ListVersions,
}

fn send_request(api: &TestApi, request: Request) -> Reply {
    match request {
        Request::Register => api.send(
            Method::POST,
            "/mods",
            Payload::Json(json!({ "name": "cloth" })),
        ),
        Request::Publish => api.send(Method::PUT, "/mods/cloth/versions/1.0.0", Payload::Empty),
        Request::ListVersions => api.send(Method::GET, "/mods/cloth/versions", Payload::Empty),
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
        assert_eq!(send_request(&api, *request), internal(), "{request:?}");
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
            sqlx::query("INSERT INTO releases (mod_id, version) SELECT id, $1 FROM mods")
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
