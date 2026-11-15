// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! Inspection of firmware dumps.
//!
//! A firmware dump is a flat set of `.nca` files (some named `*.cnmt.nca`),
//! shipped either as a folder or as a zip archive (stored or deflate
//! entries). This module counts and sizes them and extracts the version
//! hint from the folder/zip name (e.g. `Firmware.22.5.0` -> `22.5.0`).

use std::fs;
use std::io::BufReader;
use std::path::Path;

use crate::error::{Error, Result};

/// Report about a firmware dump. Contains no key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareReport {
    /// Total number of `.nca` files found.
    pub nca_count: usize,
    /// Number of those named `*.cnmt.nca` (content metadata NCAs).
    pub cnmt_nca_count: usize,
    /// Sum of the uncompressed sizes of all `.nca` files.
    pub total_size: u64,
    /// Version hint parsed from the folder/zip name (`"Firmware.22.5.0"` ->
    /// `Some("22.5.0")`), if a `major.minor.micro` pattern is present.
    pub version_hint: Option<String>,
    /// True when the dump looks like a usable firmware (at least one NCA and
    /// at least one CNMT NCA).
    pub looks_valid: bool,
}

/// Extracts a `\d+\.\d+\.\d+` version hint from a file name, e.g.
/// `Firmware.22.5.0` or `Firmware.22.5.0.zip` -> `Some("22.5.0")`.
/// The first three digit-dot-digit segments win; longer runs (like
/// `1.2.3.4`) yield the first three (`1.2.3`).
pub fn version_hint_from_name(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let mut segments = 1;
            let mut cursor = i;
            while segments < 3 && cursor + 1 < bytes.len() && bytes[cursor] == b'.' {
                let mut j = cursor + 1;
                if !bytes[j].is_ascii_digit() {
                    break;
                }
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                segments += 1;
                cursor = j;
            }
            if segments == 3 {
                return Some(name[start..cursor].to_owned());
            }
            i = cursor.max(i);
        } else {
            i += 1;
        }
    }
    None
}

fn is_nca_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".nca")
}

fn is_cnmt_nca_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".cnmt.nca")
}

fn finish_report(
    nca_count: usize,
    cnmt_nca_count: usize,
    total_size: u64,
    name: &str,
) -> FirmwareReport {
    FirmwareReport {
        nca_count,
        cnmt_nca_count,
        total_size,
        version_hint: version_hint_from_name(name),
        looks_valid: nca_count > 0 && cnmt_nca_count > 0,
    }
}

/// Inspects a firmware dump stored as a folder (flat, non-recursive).
pub fn inspect_dir(path: &Path) -> Result<FirmwareReport> {
    if !path.is_dir() {
        return Err(Error::format(format!(
            "{} is not a directory",
            path.display()
        )));
    }
    let mut nca_count = 0usize;
    let mut cnmt_nca_count = 0usize;
    let mut total_size = 0u64;
    for entry in fs::read_dir(path).map_err(Error::Io)? {
        let entry = entry.map_err(Error::Io)?;
        if !entry.file_type().map_err(Error::Io)?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if is_nca_name(name) {
            nca_count += 1;
            if is_cnmt_nca_name(name) {
                cnmt_nca_count += 1;
            }
            total_size = total_size.saturating_add(entry.metadata().map_err(Error::Io)?.len());
        }
    }
    let base_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    Ok(finish_report(
        nca_count,
        cnmt_nca_count,
        total_size,
        base_name,
    ))
}

/// Inspects a firmware dump stored as a zip archive. Only stored and deflate
/// entries are supported (this crate is built with `zip/deflate` only); other
/// compression methods produce a [`Error::Zip`] from the archive layer.
pub fn inspect_zip(path: &Path) -> Result<FirmwareReport> {
    let file = fs::File::open(path).map_err(Error::Io)?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file)).map_err(Error::Zip)?;
    let mut nca_count = 0usize;
    let mut cnmt_nca_count = 0usize;
    let mut total_size = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(Error::Zip)?;
        let name = entry.name().to_owned();
        if is_nca_name(&name) {
            nca_count += 1;
            if is_cnmt_nca_name(&name) {
                cnmt_nca_count += 1;
            }
            total_size = total_size.saturating_add(entry.size());
        }
    }
    let base_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    Ok(finish_report(
        nca_count,
        cnmt_nca_count,
        total_size,
        base_name,
    ))
}

/// Inspects a firmware dump, dispatching on whether `path` is a directory
/// or a zip archive (by extension, falling back to the file system).
pub fn inspect(path: &Path) -> Result<FirmwareReport> {
    if path.is_dir() {
        inspect_dir(path)
    } else {
        inspect_zip(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_hints() {
        assert_eq!(
            version_hint_from_name("Firmware.22.5.0"),
            Some("22.5.0".into())
        );
        assert_eq!(
            version_hint_from_name("Firmware.22.5.0.zip"),
            Some("22.5.0".into())
        );
        assert_eq!(
            version_hint_from_name("Firmware.1.2.3.4"),
            Some("1.2.3".into()),
            "extra segments: first three win"
        );
        assert_eq!(
            version_hint_from_name("Firmware 18.1.0 (Rebootless)"),
            Some("18.1.0".into())
        );
        assert_eq!(version_hint_from_name("Firmware"), None);
        assert_eq!(version_hint_from_name("22.5"), None);
        assert_eq!(version_hint_from_name(""), None);
    }

    #[test]
    fn dir_inspection() {
        let dir = std::env::temp_dir().join(format!("d8-nx-fw-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for name in ["a.nca", "b.cnmt.nca", "c.nca", "readme.txt"] {
            fs::write(
                dir.join(name),
                vec![0u8; if name.starts_with('a') { 10 } else { 20 }],
            )
            .unwrap();
        }
        // Name the folder like a firmware so the hint is picked up.
        let dir2 =
            std::env::temp_dir().join(format!("Firmware.22.5.0-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir2);
        fs::rename(&dir, &dir2).unwrap();
        let report = inspect_dir(&dir2).unwrap();
        assert_eq!(report.nca_count, 3);
        assert_eq!(report.cnmt_nca_count, 1);
        assert_eq!(report.total_size, 50);
        assert_eq!(report.version_hint.as_deref(), Some("22.5.0"));
        assert!(report.looks_valid);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn zip_inspection_deflate_and_stored() {
        use std::io::Write as _;
        let dir = std::env::temp_dir().join(format!("d8-nx-fw-zip-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("Firmware.9.9.9.zip");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            w.start_file("1.cnmt.nca", opts).unwrap();
            w.write_all(&[1u8; 100]).unwrap();
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            w.start_file("2.nca", opts).unwrap();
            w.write_all(&[2u8; 200]).unwrap();
            w.finish().unwrap();
        }
        let report = inspect_zip(&zip_path).unwrap();
        assert_eq!(report.nca_count, 2);
        assert_eq!(report.cnmt_nca_count, 1);
        assert_eq!(report.total_size, 300);
        assert_eq!(report.version_hint.as_deref(), Some("9.9.9"));
        assert!(report.looks_valid);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_dir() {
        let p = std::env::temp_dir().join(format!("d8-nx-missing-{}", std::process::id()));
        assert!(inspect_dir(&p).is_err());
    }
}
