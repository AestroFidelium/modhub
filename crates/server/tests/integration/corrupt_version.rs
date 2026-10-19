//! A version row that is not a valid `ModVersion` (written past the application) is
//! reported as corruption, not silently skipped or turned into a panic.

use bolero::{TypeGenerator, check};
use modhub_server::StoreError;

use crate::database::TestDb;

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum BadVersion {
    Empty,
    NotSemver,
    LeadingZero,
    BuildMetadata,
}

impl BadVersion {
    fn text(self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::NotSemver => "latest",
            Self::LeadingZero => "01.0.0",
            Self::BuildMetadata => "1.0.0+nightly",
        }
    }
}

#[test]
fn invalid_stored_version_is_reported_as_corruption() {
    let db = TestDb::new();
    check!().with_type::<BadVersion>().for_each(|bad| {
        db.reset();
        let name = "cloth".parse().unwrap();
        db.run(db.store().register(&name)).unwrap();
        db.run(
            sqlx::query("INSERT INTO releases (mod_id, version) SELECT id, $1 FROM mods")
                .bind(bad.text())
                .execute(db.pool()),
        )
        .unwrap();

        let error = db.run(db.store().versions(&name)).unwrap_err();
        assert!(
            matches!(&error, StoreError::CorruptVersion { text, .. } if text == bad.text()),
            "{error:?}"
        );
    });
}
