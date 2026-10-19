//! When the database is unreachable every operation returns `StoreError::Database`
//! instead of panicking or misreporting it as a domain error.

use bolero::{TypeGenerator, check};
use modhub_server::StoreError;

use crate::database::TestDb;

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Operation {
    Register,
    Publish,
    ListVersions,
}

#[test]
fn closed_pool_gives_database_errors() {
    let db = TestDb::new();
    db.run(db.pool().close());
    check!().with_type::<Operation>().for_each(|operation| {
        let store = db.store();
        let name = "cloth".parse().unwrap();
        let result = match operation {
            Operation::Register => db.run(store.register(&name)),
            Operation::Publish => db.run(store.publish(&name, &"1.0.0".parse().unwrap())),
            Operation::ListVersions => db.run(store.versions(&name)).map(drop),
        };
        assert!(
            matches!(result, Err(StoreError::Database(_))),
            "{operation:?}: {result:?}"
        );
    });
}
