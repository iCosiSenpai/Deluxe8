// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! Parsing of the plaintext `<id>.cnmt.xml` manifest found inside NSP/NSZ
//! packages.
//!
//! The manifest is a small XML document produced by nsz-style tooling:
//!
//! ```xml
//! <ContentMeta>
//!   <Type>Application</Type>          <!-- Application/Patch/AddOnContent/... -->
//!   <Id>0x0100152000022000</Id>       <!-- hex, 0x prefix -->
//!   <Version>0</Version>              <!-- decimal -->
//!   <RequiredDownloadSystemVersion>0</RequiredDownloadSystemVersion>
//!   <Content>
//!     <Type>Program</Type>
//!     <Id>e1ace5248218fa6393ec1637ce8c2c79</Id>
//!     <Size>2947907584</Size>
//!     <Hash>e1ace524...64 hex chars...</Hash>
//!     <KeyGeneration>0</KeyGeneration>
//!   </Content>
//!   ...
//!   <RequiredSystemVersion>0</RequiredSystemVersion>
//!   <PatchId>0x0100152000022800</PatchId>   <!-- base -> update id -->
//!   <OriginalId>0x0100152000022000</OriginalId> <!-- update -> base id -->
//! </ContentMeta>
//! ```

use crate::error::{Error, Result};

/// One `<Content>` element of a CNMT.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CnmtContent {
    /// Content type string: `Program`, `Meta`, `Control`, `HtmlDocument`,
    /// `LegalInformation`, ...
    pub content_type: String,
    /// Content id as a 32-char lowercase hex string (no `0x` prefix).
    pub id: String,
    /// Declared content size in bytes, if present.
    pub size: Option<u64>,
    /// SHA-256 hash of the content as a 64-char hex string, if present.
    pub hash: Option<String>,
    /// Key generation the content is encrypted with, if present.
    pub key_generation: Option<u32>,
}

/// A parsed `<id>.cnmt.xml` document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cnmt {
    /// `<Type>`: `Application`, `Patch`, `AddOnContent`, `Delta`,
    /// `SystemProgram`, ... (kept as a string; the enum of possible values is
    /// open-ended).
    pub meta_type: Option<String>,
    /// `<Id>` parsed as a u64 (hex with optional `0x` prefix).
    pub title_id: Option<u64>,
    /// `<Version>` (decimal).
    pub version: Option<u32>,
    /// `<RequiredSystemVersion>` (decimal). Note: some dumps carry a raw
    /// packed value here (e.g. `1543569458`); it is reported verbatim.
    pub required_system_version: Option<u32>,
    /// `<RequiredDownloadSystemVersion>` (decimal).
    pub required_download_system_version: Option<u32>,
    /// All `<Content>` elements in document order.
    pub contents: Vec<CnmtContent>,
}

fn parse_hex_u64(s: &str) -> Result<u64> {
    let s = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    u64::from_str_radix(s, 16).map_err(|_| Error::format(format!("invalid hex u64 '{s}'")))
}

fn parse_dec_u32(s: &str) -> Result<u32> {
    s.trim()
        .parse::<u32>()
        .map_err(|_| Error::format(format!("invalid decimal u32 '{s}'")))
}

/// Parses a CNMT XML document.
pub fn parse(xml: &str) -> Result<Cnmt> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| Error::Cnmt(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "ContentMeta" {
        return Err(Error::Cnmt(format!(
            "unexpected root element <{}>, expected <ContentMeta>",
            root.tag_name().name()
        )));
    }

    let mut out = Cnmt::default();
    for child in root.children().filter(|c| c.is_element()) {
        match child.tag_name().name() {
            "Type" => out.meta_type = child.text().map(str::trim).map(str::to_owned),
            "Id" => {
                out.title_id = child
                    .text()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(parse_hex_u64)
                    .transpose()?;
            }
            "Version" => {
                out.version = child
                    .text()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(parse_dec_u32)
                    .transpose()?;
            }
            "RequiredSystemVersion" => {
                out.required_system_version = child
                    .text()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(parse_dec_u32)
                    .transpose()?;
            }
            "RequiredDownloadSystemVersion" => {
                out.required_download_system_version = child
                    .text()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(parse_dec_u32)
                    .transpose()?;
            }
            "Content" => {
                let mut c = CnmtContent::default();
                for field in child.children().filter(|c| c.is_element()) {
                    match field.tag_name().name() {
                        "Type" => {
                            c.content_type = field.text().unwrap_or_default().trim().to_owned()
                        }
                        "Id" => c.id = field.text().unwrap_or_default().trim().to_ascii_lowercase(),
                        "Size" => {
                            c.size = field
                                .text()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(|s| {
                                    s.parse::<u64>().map_err(|_| {
                                        Error::Cnmt(format!("invalid Content/Size '{s}'"))
                                    })
                                })
                                .transpose()?;
                        }
                        "Hash" => {
                            c.hash = field
                                .text()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_ascii_lowercase())
                        }
                        "KeyGeneration" => {
                            c.key_generation = field
                                .text()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(parse_dec_u32)
                                .transpose()?;
                        }
                        _ => {}
                    }
                }
                if !c.content_type.is_empty() || !c.id.is_empty() {
                    out.contents.push(c);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Helper for tests and the CLI: formats a CNMT version as a Switch-style
/// `major.minor.micro` string (e.g. `1507328` -> `23.0.1`).
///
/// CNMT versions pack the display version as `(major << 26) | (minor << 20)
/// | (micro << 16) | build` where `major/minor/micro` are one byte each.
pub fn format_version(version: u32) -> String {
    let major = (version >> 26) & 0x3F;
    let minor = (version >> 20) & 0x3F;
    let micro = (version >> 16) & 0xF;
    format!("{major}.{minor}.{micro}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ContentMeta>
  <Type>Patch</Type>
  <Id>0x0100152000022800</Id>
  <Version>1507328</Version>
  <RequiredDownloadSystemVersion>5556</RequiredDownloadSystemVersion>
  <Content>
    <Type>Program</Type>
    <Id>9B5CA1863DA58DDFA3012591878EF670</Id>
    <Size>4848707072</Size>
    <Hash>9B5CA1863DA58DDFA3012591878EF6704CC1F39525D50B6C4252C550233A83B5</Hash>
    <KeyGeneration>4</KeyGeneration>
  </Content>
  <Content>
    <Type>Meta</Type>
    <Id>f6b0e9c108413934cc4ea53fbef207db</Id>
    <Size>9728</Size>
    <Hash>f6b0e9c108413934cc4ea53fbef207db59cc26a3a2a2f463bcf234c436aa5779</Hash>
    <KeyGeneration>4</KeyGeneration>
  </Content>
  <Digest>5930ca38055e98f53c664847a4b69a6a74ef7be0db311163ca2d1c43232c20f3</Digest>
  <KeyGenerationMin>4</KeyGenerationMin>
  <RequiredSystemVersion>1543569458</RequiredSystemVersion>
  <OriginalId>0x0100152000022000</OriginalId>
</ContentMeta>"#;

    #[test]
    fn parses_full_document() {
        let c = parse(SAMPLE).unwrap();
        assert_eq!(c.meta_type.as_deref(), Some("Patch"));
        assert_eq!(c.title_id, Some(0x0100152000022800));
        assert_eq!(c.version, Some(1507328));
        assert_eq!(c.required_system_version, Some(1543569458));
        assert_eq!(c.required_download_system_version, Some(5556));
        assert_eq!(c.contents.len(), 2);
        let prog = &c.contents[0];
        assert_eq!(prog.content_type, "Program");
        assert_eq!(prog.id, "9b5ca1863da58ddfa3012591878ef670");
        assert_eq!(prog.size, Some(4848707072));
        assert_eq!(
            prog.hash.as_deref(),
            Some("9b5ca1863da58ddfa3012591878ef6704cc1f39525d50b6c4252c550233a83b5")
        );
        assert_eq!(prog.key_generation, Some(4));
        assert_eq!(c.contents[1].content_type, "Meta");
    }

    #[test]
    fn minimal_document() {
        let c = parse("<ContentMeta><Type>AddOnContent</Type><Id>0x0100152000023001</Id><Version>65536</Version></ContentMeta>").unwrap();
        assert_eq!(c.meta_type.as_deref(), Some("AddOnContent"));
        assert_eq!(c.title_id, Some(0x0100152000023001));
        assert_eq!(c.version, Some(0x10000));
        assert!(c.contents.is_empty());
    }

    #[test]
    fn rejects_non_content_meta() {
        assert!(parse("<Other/>").is_err());
    }

    #[test]
    fn rejects_malformed_xml() {
        assert!(parse("<ContentMeta><Id>zz").is_err());
    }

    #[test]
    fn rejects_bad_numbers() {
        assert!(parse("<ContentMeta><Id>0xnothex</Id></ContentMeta>").is_err());
        assert!(parse("<ContentMeta><Version>huge</Version></ContentMeta>").is_err());
    }

    #[test]
    fn version_formatting() {
        // Standard packing: (major << 26) | (minor << 20) | (micro << 16).
        assert_eq!(format_version((23 << 26) | (1 << 16)), "23.0.1");
        assert_eq!(format_version(0), "0.0.0");
        // MK8D packs versions non-standardly (1507328 = 23 << 16 decodes to
        // 0.1.7 under the generic scheme) — that is exactly why the
        // display_version map in `package` exists.
        assert_eq!(format_version(1507328), "0.1.7");
        assert_eq!(format_version(20 << 16), "0.1.4");
    }
}
