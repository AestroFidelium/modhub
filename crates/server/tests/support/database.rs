//! A throwaway PostgreSQL database per test, on the cluster of `tools/pg`.
//!
//! nextest runs every test in its own process, so each test gets its own database and
//! tests never see each other's rows. Bolero iterations within a test share it and call
//! [`TestDb::reset`] in between.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU32, Ordering};

use modhub_server::Store;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use tokio::runtime::Runtime;

pub(crate) struct TestDb {
    runtime: Runtime,
    admin: PgConnectOptions,
    name: String,
    pool: PgPool,
    store: Store,
}

impl TestDb {
    /// Creates and migrates a fresh database.
    ///
    /// Wrapped in `AssertUnwindSafe` because bolero runs each iteration under
    /// `catch_unwind` and the pool holds callbacks that are not `RefUnwindSafe`. Sound
    /// here: a panic can leave only stray rows behind, and every iteration starts with
    /// [`TestDb::reset`] (bolero keeps iterating after a panic while it shrinks).
    pub(crate) fn new() -> AssertUnwindSafe<Self> {
        static NEXT: AtomicU32 = AtomicU32::new(0);

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL is unset: run tests in the dev shell, after `tools/pg start`");
        let admin: PgConnectOptions = url.parse().unwrap();
        let name = format!(
            "test_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );

        let pool = runtime.block_on(async {
            let mut connection = PgConnection::connect_with(&admin).await.unwrap();
            connection
                .execute(format!("CREATE DATABASE {name}").as_str())
                .await
                .unwrap();
            PgPoolOptions::new()
                .max_connections(2)
                .connect_with(admin.clone().database(&name))
                .await
                .unwrap()
        });
        let store = Store::new(pool.clone());
        runtime.block_on(store.migrate()).unwrap();
        AssertUnwindSafe(Self {
            runtime,
            admin,
            name,
            pool,
            store,
        })
    }

    pub(crate) fn store(&self) -> &Store {
        &self.store
    }

    /// Raw access, for writes that bypass the store on purpose.
    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub(crate) fn run<F: Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    /// Empties every table, so the next bolero iteration starts from nothing.
    pub(crate) fn reset(&self) {
        self.run(
            self.pool
                .execute("TRUNCATE mods, releases RESTART IDENTITY CASCADE"),
        )
        .unwrap();
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        self.runtime.block_on(async {
            self.pool.close().await;
            let mut connection = PgConnection::connect_with(&self.admin).await.unwrap();
            connection
                .execute(format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", self.name).as_str())
                .await
                .unwrap();
        });
    }
}
