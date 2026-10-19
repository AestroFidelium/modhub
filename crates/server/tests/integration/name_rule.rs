//! The naming rule exists twice, in `ModName` and in the `mods_name_rule` CHECK. They
//! must agree, or a row written past the application would break every reader.

use bolero::{TypeGenerator, check, produce};
use modhub_core::ModName;

use crate::database::TestDb;

/// A character from a set where valid names are common and every way to break the
/// rule is one pick away.
#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Glyph {
    Letter(#[generator(b'a'..=b'c')] u8),
    Digit(#[generator(b'0'..=b'2')] u8),
    Hyphen,
    Uppercase,
    Underscore,
    Cyrillic,
}

impl Glyph {
    fn char(self) -> char {
        match self {
            Self::Letter(byte) | Self::Digit(byte) => char::from(byte),
            Self::Hyphen => '-',
            Self::Uppercase => 'A',
            Self::Underscore => '_',
            Self::Cyrillic => 'ж',
        }
    }
}

/// A candidate name, long enough to cross the 64-byte limit.
#[derive(Debug, TypeGenerator)]
struct Candidate {
    #[generator(produce::<Vec<Glyph>>().with().len(0usize..=70))]
    glyphs: Vec<Glyph>,
}

#[test]
fn database_check_accepts_exactly_what_mod_name_accepts() {
    let db = TestDb::new();
    check!()
        .with_type::<Candidate>()
        .for_each(|Candidate { glyphs }| {
            db.reset();
            let text: String = glyphs.iter().map(|glyph| glyph.char()).collect();
            let stored = db.run(
                sqlx::query("INSERT INTO mods (name) VALUES ($1)")
                    .bind(&text)
                    .execute(db.pool()),
            );
            assert_eq!(
                stored.is_ok(),
                text.parse::<ModName>().is_ok(),
                "{text:?}: database said {stored:?}"
            );
        });
}
