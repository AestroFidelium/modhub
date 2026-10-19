//! The mod naming rule: what parses, what is rejected, and with which error.

use bolero::{TypeGenerator, check};
use modhub_core::{ModName, ModNameError};

use crate::names::ValidName;

#[test]
fn valid_names_parse_and_display_unchanged() {
    check!()
        .with_type::<ValidName>()
        .for_each(|ValidName(text)| {
            let name: ModName = text.parse().unwrap();
            assert_eq!(name.as_str(), text);
            assert_eq!(name.to_string(), *text);
        });
}

/// One way to break a valid name. Each variant names the single rule it violates.
#[derive(Debug, TypeGenerator)]
enum Corruption {
    Empty,
    Overlong {
        name: ValidName,
        extra: Extra,
    },
    BadStart {
        name: ValidName,
        start: BadStart,
    },
    ForeignChar {
        name: ValidName<4>,
        at: Position,
        glyph: Foreign,
    },
    TrailingHyphen {
        name: ValidName<1>,
    },
    DoubleHyphen {
        name: ValidName<2>,
        at: Position,
    },
}

/// How many bytes past the limit an overlong name goes, minus one.
#[derive(Debug, TypeGenerator)]
struct Extra(#[generator(0usize..=64)] usize);

/// A byte offset after the first character, taken modulo the name length.
#[derive(Debug, TypeGenerator)]
struct Position(u8);

impl Position {
    /// An insertion point in `1..=len`: never in front of the first character.
    fn within(&self, len: usize) -> usize {
        1 + usize::from(self.0) % len
    }
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum BadStart {
    Digit,
    Hyphen,
    Uppercase,
}

#[derive(Debug, Clone, Copy, TypeGenerator)]
enum Foreign {
    Uppercase,
    Underscore,
    Space,
    Dot,
    Cyrillic,
}

impl Foreign {
    fn glyph(self) -> char {
        match self {
            Self::Uppercase => 'Q',
            Self::Underscore => '_',
            Self::Space => ' ',
            Self::Dot => '.',
            Self::Cyrillic => 'я',
        }
    }
}

impl Corruption {
    /// The broken text and the error the rule it breaks must produce.
    fn apply(&self) -> (String, ModNameError) {
        match self {
            Self::Empty => (String::new(), ModNameError::Empty),
            Self::Overlong { name, extra } => {
                let len = ModName::MAX_LEN + 1 + extra.0;
                let mut text = name.0.clone();
                text.extend(std::iter::repeat_n('a', len - text.len()));
                (text, ModNameError::TooLong { len })
            }
            Self::BadStart { name, start } => {
                let found = match start {
                    BadStart::Digit => '7',
                    BadStart::Hyphen => '-',
                    BadStart::Uppercase => name.0.chars().next().unwrap().to_ascii_uppercase(),
                };
                let text = format!("{found}{}", &name.0[1..]);
                (text, ModNameError::BadStart { found })
            }
            Self::ForeignChar { name, at, glyph } => {
                let at = at.within(name.0.len());
                let found = glyph.glyph();
                let mut text = name.0.clone();
                text.insert(at, found);
                (text, ModNameError::BadChar { found, at })
            }
            Self::TrailingHyphen { name } => (format!("{}-", name.0), ModNameError::TrailingHyphen),
            Self::DoubleHyphen { name, at } => {
                let mut text = name.0.clone();
                text.insert_str(at.within(name.0.len()), "--");
                // Inserting next to an existing hyphen moves the first "--" left.
                let at = text.find("--").unwrap();
                (text, ModNameError::DoubleHyphen { at })
            }
        }
    }
}

#[test]
fn each_broken_rule_is_rejected_with_its_own_error() {
    check!().with_type::<Corruption>().for_each(|corruption| {
        let (text, expected) = corruption.apply();
        assert_eq!(text.parse::<ModName>(), Err(expected), "text: {text:?}");
    });
}
