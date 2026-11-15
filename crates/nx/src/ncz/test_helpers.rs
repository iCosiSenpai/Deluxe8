// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared builders for the NCZ unit tests.

#![allow(dead_code)]

use std::io::Cursor;

use super::*;

pub const TEST_KEY: [u8; 16] = [0x42; 16];
pub const TEST_CTR: [u8; 16] = [0x13; 16];

/// Pattern filler.
pub fn pattern(start: u8, len: usize) -> Vec<u8> {
    (0..len).map(|i| start.wrapping_add(i as u8)).collect()
}

/// AES-128-CTR encryption of `data` as an NCA section starting at
/// `nca_offset` (uses the same helper as the decompressor).
pub fn ctr_encrypt(counter: &[u8; 16], nca_offset: u64, data: &[u8]) -> Vec<u8> {
    let mut iv = *counter;
    iv[8..].copy_from_slice(&(nca_offset >> 4).to_be_bytes());
    let mut cipher = ctr::Ctr128BE::<aes::Aes128>::new((&TEST_KEY).into(), (&iv).into());
    let mut out = data.to_vec();
    let mut scratch = [0u8; 16];
    ctr_encrypt_chunk(&mut cipher, &mut out, &mut scratch);
    out
}

pub fn build_section_wire(s: &Section) -> Vec<u8> {
    let mut e = Vec::with_capacity(SECTION_WIRE as usize);
    e.extend_from_slice(&s.offset.to_le_bytes());
    e.extend_from_slice(&s.size.to_le_bytes());
    e.extend_from_slice(&s.crypto_type.to_le_bytes());
    e.extend_from_slice(&[0u8; 8]);
    e.extend_from_slice(&s.key);
    e.extend_from_slice(&s.counter);
    e
}

/// Builds a solid NCZ: header + section table + zstd(body).
pub fn solid_ncz(header: &[u8], sections: &[Section], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(header);
    out.extend_from_slice(&SECTIONS_MAGIC);
    out.extend_from_slice(&(sections.len() as u64).to_le_bytes());
    for s in sections {
        out.extend_from_slice(&build_section_wire(s));
    }
    let comp = zstd::stream::encode_all(Cursor::new(body), 3).unwrap();
    out.extend_from_slice(&comp);
    out
}

/// Builds an NCZBLOCK NCZ. `blocks` holds the decompressed block contents;
/// each block is zstd-compressed unless that would not shrink it.
pub fn block_ncz(header: &[u8], sections: &[Section], exponent: u8, blocks: &[&[u8]]) -> Vec<u8> {
    let body: Vec<u8> = blocks.iter().flat_map(|b| b.iter().copied()).collect();
    let block_size = 1u64 << exponent;
    let nblocks = body.len().div_ceil(block_size as usize);
    let mut sizes = Vec::new();
    let mut payload = Vec::new();
    for i in 0..nblocks {
        let raw = &body[i * block_size as usize..((i + 1) * block_size as usize).min(body.len())];
        let comp = zstd::stream::encode_all(Cursor::new(raw), 3).unwrap();
        if comp.len() < raw.len() {
            sizes.push(comp.len() as u32);
            payload.extend_from_slice(&comp);
        } else {
            sizes.push(raw.len() as u32);
            payload.extend_from_slice(raw);
        }
    }
    let mut out = Vec::new();
    out.extend_from_slice(header);
    out.extend_from_slice(&SECTIONS_MAGIC);
    out.extend_from_slice(&(sections.len() as u64).to_le_bytes());
    for s in sections {
        out.extend_from_slice(&build_section_wire(s));
    }
    out.extend_from_slice(&BLOCK_MAGIC);
    out.push(2); // version
    out.push(1); // type
    out.push(0); // unused
    out.push(exponent);
    out.extend_from_slice(&(nblocks as u32).to_le_bytes());
    out.extend_from_slice(&(body.len() as u64).to_le_bytes());
    for s in &sizes {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out.extend_from_slice(&payload);
    out
}

/// Decompresses an in-memory NCZ fully and checks hash consistency.
pub fn decompress_all(ncz_bytes: &[u8]) -> Result<Vec<u8>> {
    let mut cur = Cursor::new(ncz_bytes.to_vec());
    let hdr = read_header(&mut cur, 0, ncz_bytes.len() as u64)?;
    let mut out = Vec::new();
    let cancel = AtomicBool::new(false);
    let res = decompress_to(&mut cur, &hdr, &mut out, |_| {}, &cancel)?;
    assert_eq!(res.nca_size, out.len() as u64);
    assert_eq!(res.sha256, Sha256::digest(&out).as_slice());
    Ok(out)
}

/// Creates a unique temp dir for a test.
pub fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("d8-nx-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
