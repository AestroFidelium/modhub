//! The `modhub` binary as a process: it starts from the environment, serves, logs
//! without leaking the database password, stops cleanly on SIGTERM, and every startup
//! failure exits non-zero with the cause.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use bolero::{TypeGenerator, check, produce};

use crate::database::TestDb;
use crate::slots::NameSlot;

const PASSWORD: &str = "Pw0NotInLogs";

/// How long a healthy server may take to start or to stop. A server that misses it
/// fails the test at once instead of hanging it (sqlx alone retries a dead database
/// for 30 s).
const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Probe {
    Register(NameSlot),
    ListVersions(NameSlot),
}

/// How the server is told to stop: by a supervisor, or by Ctrl-C in a terminal.
#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Stop {
    Terminate,
    Interrupt,
}

#[derive(Debug, TypeGenerator)]
enum Launch {
    Serve {
        #[generator(produce::<Vec<Probe>>().with().len(0usize..=3))]
        probes: Vec<Probe>,
        stop: Stop,
    },
    NoDatabaseUrl,
    NotPostgresUrl,
    UnreachableDatabase,
    ListenAddressTaken,
}

/// The binary with a clean slate: none of the variables it reads are inherited.
fn modhub() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_modhub"));
    for (var, _) in std::env::vars_os() {
        let name = var.to_string_lossy();
        if name.starts_with("PG") || ["DATABASE_URL", "MODHUB_LISTEN", "RUST_LOG"].contains(&&*name)
        {
            command.env_remove(&var);
        }
    }
    command
        .env("MODHUB_LISTEN", "127.0.0.1:0")
        .stderr(Stdio::piped());
    command
}

/// Runs a launch that must fail; returns its log.
fn fails(command: Command) -> String {
    let mut server = Server::spawn(command);
    let status = server.wait();
    assert!(!status.success(), "exited successfully:\n{}", server.log);
    std::mem::take(&mut server.log)
}

/// A running server process, killed if the test fails before it stops.
struct Server {
    child: Child,
    lines: Receiver<String>,
    log: String,
}

impl Server {
    fn spawn(mut command: Command) -> Self {
        let mut child = command.spawn().unwrap();
        let stderr = BufReader::new(child.stderr.take().unwrap());
        // A reader thread, so waiting for a log line can time out.
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in stderr.lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            lines,
            log: String::new(),
        }
    }

    /// Reads log lines until the server says where it listens.
    fn listening_address(&mut self) -> SocketAddr {
        loop {
            let line = self
                .lines
                .recv_timeout(DEADLINE)
                .unwrap_or_else(|error| panic!("no `listening` line ({error}):\n{}", self.log));
            self.log.push_str(&line);
            self.log.push('\n');
            if let Some(address) = line.split("address=").nth(1) {
                return address.trim().parse().unwrap();
            }
        }
    }

    /// Waits for the process to exit and for the rest of its log.
    fn wait(&mut self) -> ExitStatus {
        let started = Instant::now();
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                started.elapsed() < DEADLINE,
                "did not exit in time:\n{}",
                self.log
            );
            thread::sleep(Duration::from_millis(20));
        };
        // The reader thread ends when the pipe closes, after the process exits.
        for line in &self.lines {
            self.log.push_str(&line);
            self.log.push('\n');
        }
        status
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Only matters when a failed assertion skipped the orderly stop.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn exchange(addr: SocketAddr, probe: Probe) -> u16 {
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
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response.split(' ').nth(1).unwrap().parse().unwrap()
}

fn signal(child: &Child, stop: Stop) {
    let flag = match stop {
        Stop::Terminate => "-TERM",
        Stop::Interrupt => "-INT",
    };
    let status = Command::new("kill")
        .args([flag, &child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
}

fn serve(db: &TestDb, probes: &[Probe], stop: Stop) {
    let mut command = modhub();
    command.env("DATABASE_URL", db.url_with_password(PASSWORD));
    let mut server = Server::spawn(command);
    let addr = server.listening_address();

    let mut registered = BTreeSet::new();
    for &probe in probes {
        let expected = match probe {
            Probe::Register(name) if registered.insert(name.text()) => 201,
            Probe::Register(_) => 409,
            Probe::ListVersions(name) if registered.contains(&name.text()) => 200,
            Probe::ListVersions(_) => 404,
        };
        assert_eq!(exchange(addr, probe), expected, "{probe:?}\n{}", server.log);
    }

    signal(&server.child, stop);
    let status = server.wait();
    let log = &server.log;
    assert!(status.success(), "{status}:\n{log}");
    assert!(log.contains("stopped"), "{log}");
    assert!(!log.contains(PASSWORD), "password in the log:\n{log}");
}

#[test]
fn binary_serves_stops_and_reports_startup_failures() {
    let db = TestDb::new();
    check!().with_type::<Launch>().for_each(|launch| {
        db.reset();
        match launch {
            Launch::Serve { probes, stop } => serve(&db, probes, *stop),
            Launch::NoDatabaseUrl => {
                let stderr = fails(modhub());
                assert!(stderr.contains("DATABASE_URL is not set"), "{stderr}");
            }
            Launch::NotPostgresUrl => {
                let mut command = modhub();
                command.env("DATABASE_URL", "mysql://localhost/modhub");
                let stderr = fails(command);
                assert!(stderr.contains("must start with postgres://"), "{stderr}");
            }
            Launch::UnreachableDatabase => {
                let mut command = modhub();
                command.env(
                    "DATABASE_URL",
                    "postgres://modhub@localhost/modhub?host=/nonexistent",
                );
                let stderr = fails(command);
                assert!(stderr.contains("connecting to PostgreSQL"), "{stderr}");
            }
            Launch::ListenAddressTaken => {
                let taken = TcpListener::bind("127.0.0.1:0").unwrap();
                let mut command = modhub();
                command
                    .env("DATABASE_URL", db.url_with_password(PASSWORD))
                    .env("MODHUB_LISTEN", taken.local_addr().unwrap().to_string());
                let stderr = fails(command);
                assert!(stderr.contains("binding"), "{stderr}");
            }
        }
    });
}
