// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! Parsing and validation of `prod.keys` / `title.keys` files.
//!
//! Both files use the same line grammar:
//!
//! ```text
//! # comment
//! key_name = hex_value   # inline comment
//! ```
//!
//! * `prod.keys` maps key names (`header_key`, `master_key_00`, ...) to hex
//!   byte strings.
//! * `title.keys` maps a 32-hex-digit rights id to the title key for that
//!   rights id.
//!
//! # Secrecy
//!
//! Key *values* are deliberately never printed: [`Key`] and the key
//! containers implement **no** `Debug`/`Display`, so key material cannot leak
//! through `{:?}`, `eprintln!`, or log formatting. Only counts, key *names*,
//! and validation results are exposed.

use std::collections::HashMap;
use std::io::BufRead;

use crate::error::{Error, Result};

/// One raw key value. Deliberately has no `Debug`/`Display`/`AsRef`
/// implementations that could leak the bytes into formatted output.
pub struct Key(Vec<u8>);

impl Key {
    /// Borrows the raw key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the key, returning the raw bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

/// Parses one line of a keys file. Returns `Ok(None)` for blank lines and
/// comments, `Ok(Some((name, value)))` for key lines, or an error for
/// malformed lines.
fn parse_line(line: &str, line_no: usize) -> Result<Option<(String, Vec<u8>)>> {
    // A UTF-8 byte order mark (some Windows editors add one) is not part of
    // the first key's name.
    let line = if line_no == 1 { line.trim_start_matches('\u{feff}') } else { line };
    // Strip inline comments ('#' or ';') — these never appear in key names
    // or hex values.
    let line = line.split(['#', ';']).next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    let (name, value) = line.split_once('=').ok_or_else(|| Error::KeyParse {
        line: line_no,
        msg: "missing '=' separator".into(),
    })?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() {
        return Err(Error::KeyParse {
            line: line_no,
            msg: "empty key name".into(),
        });
    }
    if value.is_empty() || value.len() % 2 != 0 {
        return Err(Error::KeyParse {
            line: line_no,
            msg: "key value must be non-empty hex with an even number of digits".into(),
        });
    }
    let bytes = hex::decode(value).map_err(|_| Error::KeyParse {
        line: line_no,
        msg: "key value is not valid hex".into(),
    })?;
    Ok(Some((name.to_owned(), bytes)))
}

/// A set of named keys parsed from `prod.keys`.
///
/// Deliberately implements no `Debug`/`Display`: the values are secrets.
pub struct KeySet {
    keys: HashMap<String, Key>,
}

impl KeySet {
    /// Parses a `prod.keys`-style file (any [`BufRead`]).
    pub fn parse(reader: impl BufRead) -> Result<Self> {
        let mut keys = HashMap::new();
        for (line_no, line) in reader.lines().enumerate() {
            let line = line.map_err(Error::Io)?;
            if let Some((name, value)) = parse_line(&line, line_no + 1)? {
                keys.insert(name, Key(value));
            }
        }
        Ok(Self { keys })
    }

    /// Borrows the raw bytes of `name`, if present.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.keys.get(name).map(|k| k.as_bytes())
    }

    /// Returns true if a key with this name is present.
    pub fn contains(&self, name: &str) -> bool {
        self.keys.contains_key(name)
    }

    /// Number of keys in the set.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Returns true if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Iterates key names (values are not exposed by the iterator).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.keys.keys().map(String::as_str)
    }
}

/// Key names that a working Switch crypto setup needs.
pub const REQUIRED_KEYS: &[&str] = &[
    "header_key",
    "aes_kek_generation_source",
    "aes_key_generation_source",
    "key_area_key_application_00",
    "master_key_00",
    "titlekek_00",
];

/// Validation report for a [`KeySet`]. Contains no key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysReport {
    /// Total number of keys present.
    pub count: usize,
    /// Whether `header_key` is present.
    pub has_header_key: bool,
    /// Highest revision index found among `master_key_XX`,
    /// `key_area_key_application_XX` and `titlekek_XX` entries (0 when none
    /// of those families are present).
    pub max_master_key_revision: u32,
    /// Names from [`REQUIRED_KEYS`] that are missing.
    pub missing_required: Vec<String>,
}

/// Validates a key set, producing a report without any key values.
pub fn validate(keys: &KeySet) -> KeysReport {
    const FAMILIES: &[&str] = &["master_key_", "key_area_key_application_", "titlekek_"];
    let mut max_rev = 0u32;
    for name in keys.names() {
        for fam in FAMILIES {
            if let Some(suffix) = name.strip_prefix(fam) {
                if let Ok(rev) = suffix.parse::<u32>() {
                    max_rev = max_rev.max(rev);
                }
                break;
            }
        }
    }
    let missing_required = REQUIRED_KEYS
        .iter()
        .filter(|name| !keys.contains(name))
        .map(|s| (*s).to_owned())
        .collect();
    KeysReport {
        count: keys.len(),
        has_header_key: keys.contains("header_key"),
        max_master_key_revision: max_rev,
        missing_required,
    }
}

/// Title keys parsed from `title.keys` (rights id -> key).
///
/// Deliberately implements no `Debug`/`Display`: the values are secrets.
pub struct TitleKeys {
    keys: HashMap<String, Key>,
}

impl TitleKeys {
    /// Parses a `title.keys`-style file.
    pub fn parse(reader: impl BufRead) -> Result<Self> {
        let mut keys = HashMap::new();
        for (line_no, line) in reader.lines().enumerate() {
            let line = line.map_err(Error::Io)?;
            if let Some((name, value)) = parse_line(&line, line_no + 1)? {
                keys.insert(name, Key(value));
            }
        }
        Ok(Self { keys })
    }

    /// Number of title keys in the set.
    pub fn count(&self) -> usize {
        self.keys.len()
    }

    /// Borrows the key for a rights id (32 hex chars, lower case).
    pub fn get(&self, rights_id: &str) -> Option<&[u8]> {
        self.keys.get(rights_id).map(|k| k.as_bytes())
    }
}

/// Report for a [`TitleKeys`] set. Contains no key material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TitleKeysReport {
    /// Number of title keys present.
    pub count: usize,
}

/// Produces a report for a title key set.
pub fn validate_title_keys(keys: &TitleKeys) -> TitleKeysReport {
    TitleKeysReport {
        count: keys.count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const PROD: &str = "\
# comment line
header_key = 00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff
aes_kek_generation_source = 00112233445566778899aabbccddeeff
aes_key_generation_source = 00112233445566778899aabbccddeeff
master_key_00 = 000102030405060708090a0b0c0d0e0f
master_key_01 = 101112131415161718191a1b1c1d1e1f
key_area_key_application_02 = 202122232425262728292a2b2c2d2e2f
key_area_key_application_00 = 303132333435363738393a3b3c3d3e3f
titlekek_00 = 404142434445464748494a4b4c4d4e4f
titlekek_03 = 505152535455565758595a5b5c5d5e5f ; trailing comment
";

    #[test]
    fn parses_and_validates() {
        let set = KeySet::parse(Cursor::new(PROD)).unwrap();
        assert_eq!(set.len(), 9);
        assert_eq!(
            set.get("header_key").unwrap(),
            &hex::decode("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
                .unwrap()
        );
        let report = validate(&set);
        assert_eq!(report.count, 9);
        assert!(report.has_header_key);
        // max over master_key (1), key_area_key_application (2), titlekek (3)
        assert_eq!(report.max_master_key_revision, 3);
        assert_eq!(report.missing_required, Vec::<String>::new());
    }

    #[test]
    fn reports_missing_required() {
        let set = KeySet::parse(Cursor::new(
            "master_key_00 = 000102030405060708090a0b0c0d0e0f\n",
        ))
        .unwrap();
        let report = validate(&set);
        assert!(!report.has_header_key);
        assert_eq!(report.max_master_key_revision, 0);
        assert!(!report.missing_required.is_empty());
        assert!(report.missing_required.iter().any(|n| n == "header_key"));
    }

    #[test]
    fn empty_set_report() {
        let set = KeySet::parse(Cursor::new("")).unwrap();
        let report = validate(&set);
        assert_eq!(report.count, 0);
        assert_eq!(report.max_master_key_revision, 0);
        assert_eq!(report.missing_required.len(), REQUIRED_KEYS.len());
    }

    #[test]
    fn ignores_a_byte_order_mark() {
        let set = KeySet::parse(Cursor::new(
            "\u{feff}header_key = 00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff\n",
        ))
        .unwrap();
        assert!(set.contains("header_key"));
    }

    #[test]
    fn handles_crlf_and_whitespace() {
        let set = KeySet::parse(Cursor::new(
            "master_key_00   =    000102030405060708090a0b0c0d0e0f\r\n",
        ))
        .unwrap();
        assert_eq!(set.len(), 1);
        assert!(set.get("master_key_00").is_some());
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(KeySet::parse(Cursor::new("no separator here\n")).is_err());
        assert!(KeySet::parse(Cursor::new("name = xyz\n")).is_err());
        assert!(
            KeySet::parse(Cursor::new("name = 123\n")).is_err(),
            "odd length"
        );
        assert!(KeySet::parse(Cursor::new("= 00\n")).is_err(), "empty name");
        assert!(
            KeySet::parse(Cursor::new("name =\n")).is_err(),
            "empty value"
        );
        assert!(
            KeySet::parse(Cursor::new("name = 0g\n")).is_err(),
            "bad hex"
        );
    }

    #[test]
    fn title_keys_parse() {
        let set = TitleKeys::parse(Cursor::new(
            "0100152000023001000000000000000d = 00112233445566778899aabbccddeeff\n",
        ))
        .unwrap();
        assert_eq!(set.count(), 1);
        assert_eq!(
            set.get("0100152000023001000000000000000d").unwrap(),
            &hex::decode("00112233445566778899aabbccddeeff").unwrap()
        );
        assert_eq!(validate_title_keys(&set).count, 1);
    }

    #[test]
    fn duplicate_names_last_wins() {
        let set = KeySet::parse(Cursor::new(
            "master_key_00 = 000102030405060708090a0b0c0d0e0f\n\
             master_key_00 = 101112131415161718191a1b1c1d1e1f\n",
        ))
        .unwrap();
        assert_eq!(set.len(), 1);
        assert_eq!(set.get("master_key_00").unwrap()[0], 0x10);
    }
}
