//! A stored module whose bytes no longer match the digest written with them (bit rot,
//! a write past the store) is reported as corruption, never served.

use bolero::{TypeGenerator, check};
use modhub_core::ModModule;
use modhub_server::StoreError;

use crate::database::TestDb;
use crate::modules::EXAMPLE_MOD;

/// What happened to the stored module.
#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Damage {
    /// One byte overwritten.
    Flipped {
        at: u32,
        byte: u8,
    },
    Truncated {
        by: u16,
    },
    Appended {
        byte: u8,
    },
}

#[test]
fn damaged_module_is_reported_and_not_served() {
    let db = TestDb::new();
    check!().with_type::<Damage>().for_each(|damage| {
        db.reset();
        let name = "cloth".parse().unwrap();
        let version = "1.0.0".parse().unwrap();
        db.run(db.store().register(&name)).unwrap();
        let module = ModModule::check(EXAMPLE_MOD).unwrap();
        db.run(db.store().publish(&name, &version, &module))
            .unwrap();

        let mut bytes = EXAMPLE_MOD.to_vec();
        match *damage {
            Damage::Flipped { at, byte } => {
                let at = usize::try_from(at).unwrap() % bytes.len();
                // A flip to the same byte is no damage; make it a real change.
                bytes[at] = if bytes[at] == byte { !byte } else { byte };
            }
            Damage::Truncated { by } => bytes.truncate(bytes.len() - 1 - usize::from(by)),
            Damage::Appended { byte } => bytes.push(byte),
        }
        db.run(
            sqlx::query("UPDATE releases SET module = $1")
                .bind(&bytes)
                .execute(db.pool()),
        )
        .unwrap();

        let error = db.run(db.store().download(&name, &version)).unwrap_err();
        assert!(
            matches!(error, StoreError::CorruptModule { .. }),
            "{error:?}"
        );
    });
}
