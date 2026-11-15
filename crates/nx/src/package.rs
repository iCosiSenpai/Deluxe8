// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! High-level inspection of `.nsp` / `.nsz` package files.
//!
//! [`inspect`] parses the PFS0 container, the plaintext `<id>.cnmt.xml`
//! manifest when present, and falls back gracefully when it is not: title
//! id and kind are then derived from the ticket's rights id (whose first
//! 8 bytes are the title id) and from the Switch title-id bit conventions
//! (0x800 = update, 0x1000 = DLC).

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::cnmt::{self, Cnmt};
use crate::error::{Error, Result};
use crate::ncz;
use crate::pfs0::Pfs0;

/// The container format of the package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// A plain NSP (PFS0 with NCA entries).
    Nsp,
    /// An NSZ (PFS0 where some NCAs are stored as NCZ-compressed entries).
    Nsz,
}

/// What a package is, by Switch conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    /// Base game / application.
    Base,
    /// Update (patch).
    Update,
    /// DLC / add-on content.
    Dlc,
    /// Could not be determined.
    Unknown,
}

impl PackageKind {
    /// Display name of the kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            PackageKind::Base => "Base",
            PackageKind::Update => "Update",
            PackageKind::Dlc => "Dlc",
            PackageKind::Unknown => "Unknown",
        }
    }
}

/// One NCZ-compressed entry of an NSZ package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NczEntryInfo {
    /// Entry name (e.g. `<id>.ncz`).
    pub name: String,
    /// True when the body is a single solid zstd stream; false when it is
    /// NCZBLOCK block-compressed (or when the header could not be parsed,
    /// in which case this is a best-effort default).
    pub solid: bool,
    /// Compressed entry size in bytes.
    pub size: u64,
}

/// One entry of the package's PFS0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEntry {
    /// Entry name.
    pub name: String,
    /// Entry size in bytes.
    pub size: u64,
}

/// Everything [`inspect`] knows about a package.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    /// Path the package was inspected from.
    pub path: PathBuf,
    /// Container format.
    pub container: Container,
    /// Title id from the CNMT (or the ticket rights id when there is no
    /// CNMT XML).
    pub title_id: Option<u64>,
    /// Base application title id: `title_id & !0xFFF`, clearing the update
    /// (0x800) and DLC (0x1000) bits per Switch rules.
    pub application_title_id: Option<u64>,
    /// Base/Update/Dlc/Unknown.
    pub kind: PackageKind,
    /// CNMT `<Type>` (`Application`, `Patch`, `AddOnContent`, ...) when a
    /// CNMT XML was parsed; otherwise inferred from the title id bits.
    pub cnmt_type: Option<String>,
    /// CNMT `<Version>`, or 0 when unknown.
    pub version: u32,
    /// Known display version (MK8D map: 0 -> "1.0.0", 20<<16 -> "3.0.4",
    /// 23<<16 -> "4.0.1"; other versions -> None).
    pub display_version: Option<String>,
    /// CNMT `<RequiredSystemVersion>`, if present.
    pub required_system_version: Option<u32>,
    /// CNMT `<RequiredDownloadSystemVersion>`, if present.
    pub required_download_system_version: Option<u32>,
    /// NCZ-compressed entries.
    pub ncz: Vec<NczEntryInfo>,
    /// Whether the package contains a ticket (`.tik` entry).
    pub has_ticket: bool,
    /// All PFS0 entries.
    pub entries: Vec<PackageEntry>,
}

/// Mario Kart 8 Deluxe application title id.
pub const MK8D_TITLE_ID: u64 = 0x0100152000022000;

/// The low 13 bits of a Switch title id encode the variant (bit 0x800 =
/// update, bit 0x1000 = DLC) plus a per-DLC index (0x7FF). Clearing them
/// yields the base application id: updates follow `id & !0x800`, DLCs
/// follow the same rule as the Switch itself (this is also what yuzu's
/// `GetBaseTitleId` does).
const APP_ID_MASK: u64 = !0x1FFF;

impl PackageInfo {
    /// For MK8D updates: builds below `20<<16` are 32-bit, at/above are
    /// 64-bit. Returns `None` for anything else.
    pub fn bitness_note(&self) -> Option<&'static str> {
        if self.application_title_id == Some(MK8D_TITLE_ID) && self.kind == PackageKind::Update {
            Some(if self.version < 20 << 16 {
                "32-bit"
            } else {
                "64-bit"
            })
        } else {
            None
        }
    }
}

/// Derives the kind from a title id's update/DLC bits.
fn kind_from_title_id(title_id: u64) -> PackageKind {
    if title_id & 0x800 != 0 {
        PackageKind::Update
    } else if title_id & 0x1000 != 0 {
        PackageKind::Dlc
    } else {
        PackageKind::Base
    }
}

/// Maps a CNMT `<Type>` to a [`PackageKind`].
fn kind_from_cnmt_type(meta_type: &str) -> PackageKind {
    match meta_type {
        "Application" => PackageKind::Base,
        "Patch" => PackageKind::Update,
        "AddOnContent" => PackageKind::Dlc,
        _ => PackageKind::Unknown,
    }
}

/// The CNMT type implied by a title id (used when no CNMT XML is present).
fn cnmt_type_from_title_id(title_id: u64) -> Option<&'static str> {
    if title_id & 0x800 != 0 {
        Some("Patch")
    } else if title_id & 0x1000 != 0 {
        Some("AddOnContent")
    } else {
        Some("Application")
    }
}

/// Reads the title id out of a ticket: the rights id (whose first 8 bytes
/// are the title id) sits at a fixed offset past the signature, which
/// depends on the signature type:
/// `0x010000/0x010003` (RSA-4096) -> data at 0x240, `0x010001/0x010004`
/// (RSA-2048) -> data at 0x140, `0x010002/0x010005` (ECDSA) -> data at
/// 0x80; the rights id is 0x160 bytes into that data.
fn ticket_title_id<R: Read + Seek>(ticket: &mut R, size: u64) -> Result<Option<u64>> {
    if size < 4 {
        return Ok(None);
    }
    let mut sig_type = [0u8; 4];
    ticket.read_exact(&mut sig_type).map_err(Error::Io)?;
    let sig_type = u32::from_le_bytes(sig_type);
    let data_offset: u64 = match sig_type {
        0x010000 | 0x010003 => 0x240,
        0x010001 | 0x010004 => 0x140,
        0x010002 | 0x010005 => 0x80,
        _ => return Ok(None),
    };
    let rights_id_offset = data_offset + 0x160;
    if size < rights_id_offset + 0x10 {
        return Ok(None);
    }
    ticket
        .seek(SeekFrom::Start(rights_id_offset))
        .map_err(Error::Io)?;
    let mut rights_id = [0u8; 16];
    ticket.read_exact(&mut rights_id).map_err(Error::Io)?;
    // The rights id is the title id printed as a big-endian hex string, so
    // its first 8 bytes are the title id in big-endian byte order.
    let title_id = u64::from_be_bytes(rights_id[..8].try_into().unwrap());
    // Sanity: Switch title ids are 0x0100... prefixed (top 16 bits).
    if title_id >> 48 != 0x0100 {
        return Ok(None);
    }
    Ok(Some(title_id))
}

/// Inspects an `.nsp` / `.nsz` package without modifying it. No NCZ entry
/// is decompressed; only headers and small metadata entries are read.
pub fn inspect(path: impl AsRef<Path>) -> Result<PackageInfo> {
    let path = path.as_ref();
    let file = File::open(path).map_err(Error::Io)?;
    let mut pfs0 = Pfs0::read(BufReader::new(file))?;

    let container = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("nsz"))
    {
        Some(true) => Container::Nsz,
        _ => Container::Nsp,
    };

    let entries: Vec<PackageEntry> = pfs0
        .entries()
        .iter()
        .map(|e| PackageEntry {
            name: e.name.clone(),
            size: e.size,
        })
        .collect();
    let has_ticket = pfs0.find_suffix(".tik").is_some();

    // CNMT XML (plaintext metadata); a package without it falls back to
    // the ticket.
    let cnmt: Option<Cnmt> = match pfs0.find_suffix_index(".cnmt.xml") {
        Some(index) => {
            let size = pfs0.entries()[index].size;
            if size > 4 * 1024 * 1024 {
                return Err(Error::format(format!(
                    "cnmt.xml entry is implausibly large ({size} bytes)"
                )));
            }
            let mut er = pfs0.entry_reader(index)?;
            let mut xml = vec![0u8; size as usize];
            er.read_exact(&mut xml).map_err(Error::Io)?;
            Some(cnmt::parse(&String::from_utf8_lossy(&xml))?)
        }
        None => None,
    };

    // NCZ entry list with the solid/block flag from each NCZ header.
    let mut ncz_infos = Vec::new();
    let entry_names_sizes: Vec<(String, u64)> = pfs0
        .entries()
        .iter()
        .map(|e| (e.name.clone(), e.size))
        .collect();
    for (i, (name, size)) in entry_names_sizes.iter().enumerate() {
        if !name.to_ascii_lowercase().ends_with(".ncz") {
            continue;
        }
        let solid = match pfs0.entry_reader(i) {
            Ok(mut er) => ncz::read_header(&mut er, 0, *size)
                .map(|h| h.block.is_none())
                .unwrap_or(false),
            Err(_) => false,
        };
        ncz_infos.push(NczEntryInfo {
            name: name.clone(),
            solid,
            size: *size,
        });
    }

    // Title id: CNMT first, then the ticket.
    let mut title_id = cnmt.as_ref().and_then(|c| c.title_id);
    if title_id.is_none()
        && let Some(index) = pfs0.find_suffix_index(".tik")
    {
        let size = pfs0.entries()[index].size;
        let mut er = pfs0.entry_reader(index)?;
        title_id = ticket_title_id(&mut er, size)?;
    }

    let (kind, cnmt_type) = match (cnmt.as_ref(), title_id) {
        (Some(c), _) => (
            c.meta_type
                .as_deref()
                .map(kind_from_cnmt_type)
                .unwrap_or(PackageKind::Unknown),
            c.meta_type.clone(),
        ),
        (None, Some(id)) => (
            kind_from_title_id(id),
            cnmt_type_from_title_id(id).map(str::to_owned),
        ),
        (None, None) => (PackageKind::Unknown, None),
    };

    let application_title_id = title_id.map(|id| id & APP_ID_MASK);
    let version = cnmt.as_ref().and_then(|c| c.version).unwrap_or(0);
    // The MK8D display-version map applies to base/update titles only; for
    // DLCs (or unknown kinds) a version of 0 means "unknown" and must not
    // be dressed up as "1.0.0".
    let display_version = match (kind, application_title_id) {
        (PackageKind::Base | PackageKind::Update, Some(MK8D_TITLE_ID)) => match version {
            0 => Some("1.0.0".to_owned()),
            v if v == 20 << 16 => Some("3.0.4".to_owned()),
            v if v == 23 << 16 => Some("4.0.1".to_owned()),
            _ => None,
        },
        _ => None,
    };

    Ok(PackageInfo {
        path: path.to_path_buf(),
        container,
        title_id,
        application_title_id,
        kind,
        cnmt_type,
        version,
        display_version,
        required_system_version: cnmt.as_ref().and_then(|c| c.required_system_version),
        required_download_system_version: cnmt
            .as_ref()
            .and_then(|c| c.required_download_system_version),
        ncz: ncz_infos,
        has_ticket,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pfs0::{EntrySpec, Pfs0Writer};
    use std::io::Cursor;

    /// Builds a synthetic NSP in memory from (name, bytes) pairs.
    fn build_nsp(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        let specs: Vec<EntrySpec> = entries
            .iter()
            .map(|(name, data)| EntrySpec {
                name: (*name).to_owned(),
                size: data.len() as u64,
            })
            .collect();
        {
            let mut w = Pfs0Writer::new(&mut out, specs).unwrap();
            for (_, data) in entries {
                w.write_next(&mut Cursor::new(data)).unwrap();
            }
            w.finish().unwrap();
        }
        out
    }

    fn write_nsp_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    const CNMT_APP: &str = r#"<ContentMeta>
  <Type>Application</Type>
  <Id>0x0100152000022000</Id>
  <Version>0</Version>
  <RequiredSystemVersion>0</RequiredSystemVersion>
  <Content><Type>Program</Type><Id>aa</Id><Size>1</Size><Hash>bb</Hash><KeyGeneration>0</KeyGeneration></Content>
</ContentMeta>"#;

    #[test]
    fn inspects_base_package_with_cnmt() {
        let dir = std::env::temp_dir().join(format!("d8-nx-pkg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let nsp = build_nsp(&[
            ("abc.cnmt.xml", CNMT_APP.as_bytes().to_vec()),
            ("aaa.tik", vec![0u8; 0x2c0]),
            ("cc.nca", vec![7u8; 100]),
        ]);
        let path = write_nsp_file(&dir, "base.nsp", &nsp);
        let info = inspect(&path).unwrap();
        assert_eq!(info.container, Container::Nsp);
        assert_eq!(info.title_id, Some(MK8D_TITLE_ID));
        assert_eq!(info.application_title_id, Some(MK8D_TITLE_ID));
        assert_eq!(info.kind, PackageKind::Base);
        assert_eq!(info.cnmt_type.as_deref(), Some("Application"));
        assert_eq!(info.version, 0);
        assert_eq!(info.display_version.as_deref(), Some("1.0.0"));
        assert!(info.has_ticket);
        assert_eq!(info.entries.len(), 3);
        assert!(info.ncz.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A minimal RSA-2048 ticket with rights id 0100152000022800...04.
    fn update_ticket() -> Vec<u8> {
        let mut t = vec![0u8; 0x400];
        t[..4].copy_from_slice(&0x00010004u32.to_le_bytes()); // RSA-2048-SHA256
        let rights = hex::decode("01001520000228000000000000000004").unwrap();
        t[0x140 + 0x160..0x140 + 0x160 + 16].copy_from_slice(&rights);
        t
    }

    #[test]
    fn inspects_update_without_cnmt_via_ticket() {
        let dir = std::env::temp_dir().join(format!("d8-nx-pkg2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let nsp = build_nsp(&[
            ("01001520000228000000000000000004.tik", update_ticket()),
            ("dd.nca", vec![1u8; 10]),
        ]);
        let path = write_nsp_file(&dir, "upd.nsz", &nsp);
        let info = inspect(&path).unwrap();
        assert_eq!(info.container, Container::Nsz);
        assert_eq!(info.title_id, Some(0x0100152000022800));
        assert_eq!(info.application_title_id, Some(MK8D_TITLE_ID));
        assert_eq!(info.kind, PackageKind::Update);
        assert_eq!(info.cnmt_type.as_deref(), Some("Patch"), "inferred");
        assert_eq!(info.version, 0);
        // version 0 maps to "1.0.0" in the MK8D display-version map.
        assert_eq!(info.display_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            info.bitness_note(),
            Some("32-bit"),
            "v0 update is a 32-bit build"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn display_version_map() {
        let dir = std::env::temp_dir().join(format!("d8-nx-pkg3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let xml = CNMT_APP
            .replace("<Id>0x0100152000022000</Id>", "<Id>0x0100152000022800</Id>")
            .replace("<Type>Application</Type>", "<Type>Patch</Type>")
            .replace("<Version>0</Version>", "<Version>1507328</Version>");
        let nsp = build_nsp(&[("x.cnmt.xml", xml.into_bytes())]);
        let path = write_nsp_file(&dir, "u.nsp", &nsp);
        let info = inspect(&path).unwrap();
        assert_eq!(info.kind, PackageKind::Update);
        assert_eq!(info.version, 23 << 16);
        assert_eq!(info.display_version.as_deref(), Some("4.0.1"));
        assert_eq!(info.bitness_note(), Some("64-bit"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dlc_application_id_rule() {
        // 0x1000 bit + index bits -> base application id.
        assert_eq!(0x0100152000023001 & APP_ID_MASK, MK8D_TITLE_ID);
        // Update bit -> base application id.
        assert_eq!(0x0100152000022800 & APP_ID_MASK, MK8D_TITLE_ID);
        // Base is its own application id.
        assert_eq!(MK8D_TITLE_ID & APP_ID_MASK, MK8D_TITLE_ID);
    }

    #[test]
    fn not_a_package() {
        let dir = std::env::temp_dir().join(format!("d8-nx-pkg4-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("junk.nsp");
        std::fs::write(&path, b"garbage").unwrap();
        assert!(inspect(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
