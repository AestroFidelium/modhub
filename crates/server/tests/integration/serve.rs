//! `serve` on a real TCP socket: requests get answered, and the shutdown signal stops
//! the server cleanly (it returns `Ok` and the port closes).

use std::collections::BTreeSet;
use std::net::SocketAddr;

use bolero::{TypeGenerator, check, produce};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

use crate::database::TestDb;
use crate::slots::NameSlot;

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Probe {
    Register(NameSlot),
    ListVersions(NameSlot),
}

#[derive(Debug, TypeGenerator)]
struct Session {
    #[generator(produce::<Vec<Probe>>().with().len(0usize..=4))]
    probes: Vec<Probe>,
}

/// One HTTP/1.1 exchange over a fresh connection; returns the status code.
async fn exchange(addr: SocketAddr, probe: Probe) -> u16 {
    let request = match probe {
        Probe::Register(name) => {
            let body = format!(r#"{{"name":"{}"}}"#, name.text());
            format!(
                "POST /mods HTTP/1.1\r\nHost: modhub\r\nConnection: close\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
        }
        Probe::ListVersions(name) => format!(
            "GET /mods/{}/versions HTTP/1.1\r\nHost: modhub\r\nConnection: close\r\n\r\n",
            name.text()
        ),
    };
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    // "HTTP/1.1 201 Created\r\n…"
    response
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("not an HTTP response: {response:?}"))
}

#[test]
fn answers_over_tcp_and_stops_on_signal() {
    let db = TestDb::new();
    check!().with_type::<Session>().for_each(|session| {
        db.reset();
        db.run(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let (stop, stopped) = oneshot::channel::<()>();
            let server = tokio::spawn(modhub_server::serve(listener, db.store().clone(), async {
                let _ = stopped.await;
            }));

            let mut registered = BTreeSet::new();
            for &probe in &session.probes {
                let expected = match probe {
                    Probe::Register(name) if registered.insert(name.text()) => 201,
                    Probe::Register(_) => 409,
                    Probe::ListVersions(name) if registered.contains(&name.text()) => 200,
                    Probe::ListVersions(_) => 404,
                };
                assert_eq!(exchange(addr, probe).await, expected, "{probe:?}");
            }

            stop.send(()).unwrap();
            server.await.unwrap().unwrap();
            assert!(TcpStream::connect(addr).await.is_err(), "port still open");
        });
    });
}
