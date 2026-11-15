// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! Unit tests for NCZ decompression and NSZ->NSP conversion, all with
//! synthetic data built in memory.

#![allow(dead_code)]

use std::io::Cursor;

use sha2::{Digest, Sha256};

use super::test_helpers::*;
use super::*;
use crate::error::Error;
use crate::pfs0::{EntrySpec, Pfs0, Pfs0Writer};

#[test]
fn solid_with_uncovered_range() {
    let header = pattern(0xA0, NCA_HEADER_SIZE as usize);
    let sections = vec![
        Section {
            offset: 0x5000,
            size: 0x1000,
            crypto_type: 3,
            key: TEST_KEY,
            counter: TEST_CTR,
        },
        Section {
            offset: 0x6000,
            size: 0x800,
            crypto_type: 0,
            key: [0; 16],
            counter: [0; 16],
        },
    ];
    let uncovered = pattern(0x10, 0x1000);
    let sec1_plain = pattern(0x20, 0x1000);
    let sec2_plain = pattern(0x30, 0x800);
    let mut body = uncovered.clone();
    body.extend_from_slice(&ctr_encrypt(&TEST_CTR, 0x5000, &sec1_plain));
    body.extend_from_slice(&sec2_plain);

    let ncz = solid_ncz(&header, &sections, &body);
    let out = decompress_all(&ncz).unwrap();
    let mut expected = header.clone();
    expected.extend_from_slice(&uncovered);
    expected.extend_from_slice(&sec1_plain);
    expected.extend_from_slice(&sec2_plain);
    assert_eq!(out, expected);
}

#[test]
fn solid_bktr_and_gap_zero_fill() {
    let header = pattern(0x50, NCA_HEADER_SIZE as usize);
    let sections = vec![
        Section {
            offset: 0x4000,
            size: 0x800,
            crypto_type: 4,
            key: TEST_KEY,
            counter: TEST_CTR,
        },
        Section {
            offset: 0x6000,
            size: 0x800,
            crypto_type: 0,
            key: [0; 16],
            counter: [0; 16],
        },
    ];
    let sec1_plain = pattern(0x61, 0x800);
    let sec2_plain = pattern(0x71, 0x800);
    let mut body = ctr_encrypt(&TEST_CTR, 0x4000, &sec1_plain);
    body.extend_from_slice(&sec2_plain);

    let ncz = solid_ncz(&header, &sections, &body);
    let out = decompress_all(&ncz).unwrap();
    let mut expected = header;
    expected.extend_from_slice(&sec1_plain);
    expected.extend_from_slice(&[0u8; 0x1800]); // gap [0x4800, 0x6000)
    expected.extend_from_slice(&sec2_plain);
    assert_eq!(out, expected);
}

#[test]
fn solid_non_multiple_of_16_section_size() {
    // Exercises the partial-block tail path in ctr_encrypt_chunk.
    let header = vec![0u8; NCA_HEADER_SIZE as usize];
    let sections = vec![Section {
        offset: 0x4000,
        size: 0x123, // not a multiple of 16
        crypto_type: 3,
        key: TEST_KEY,
        counter: TEST_CTR,
    }];
    let plain = pattern(0x33, 0x123);
    let body = ctr_encrypt(&TEST_CTR, 0x4000, &plain);
    let ncz = solid_ncz(&header, &sections, &body);
    let out = decompress_all(&ncz).unwrap();
    assert_eq!(&out[NCA_HEADER_SIZE as usize..], &plain);
}

#[test]
fn solid_multi_chunk_ctr_continuity() {
    // Regression test: multiple 1 MiB chunks plus a final partial chunk,
    // followed by a second CTR section with its own counter. The keystream
    // must stay exact across every chunk boundary (this caught the
    // short-read counter-desync bug during development).
    let header = vec![0u8; NCA_HEADER_SIZE as usize];
    let sec1_size = 0x100000 + 0x8123i64; // 1 MiB + partial chunk
    let sec1_off = 0x4000i64;
    let sec2_off = sec1_off + sec1_size;
    let sections = vec![
        Section {
            offset: sec1_off,
            size: sec1_size,
            crypto_type: 3,
            key: TEST_KEY,
            counter: TEST_CTR,
        },
        Section {
            offset: sec2_off,
            size: 0x9000,
            crypto_type: 3,
            key: TEST_KEY,
            counter: [0x77; 16],
        },
    ];
    let plain1: Vec<u8> = (0..sec1_size as usize)
        .map(|i| (i.wrapping_mul(31) % 251) as u8)
        .collect();
    let plain2 = pattern(0x42, 0x9000);
    let mut body = ctr_encrypt(&TEST_CTR, sec1_off as u64, &plain1);
    body.extend_from_slice(&ctr_encrypt(&[0x77; 16], sec2_off as u64, &plain2));
    let ncz = solid_ncz(&header, &sections, &body);
    let out = decompress_all(&ncz).unwrap();
    let mut expected = header;
    expected.extend_from_slice(&plain1);
    expected.extend_from_slice(&plain2);
    assert_eq!(out, expected);
}

#[test]
fn block_stream_with_stored_and_compressed_blocks() {
    let header = pattern(0x90, NCA_HEADER_SIZE as usize);
    let sections = vec![
        Section {
            offset: 0x4000,
            size: 0x8000,
            crypto_type: 3,
            key: TEST_KEY,
            counter: TEST_CTR,
        },
        Section {
            offset: 0xc000,
            size: 0x1000,
            crypto_type: 0,
            key: [0; 16],
            counter: [0; 16],
        },
    ];
    let sec1_plain: Vec<u8> = (0..0x8000).map(|i| (i % 251) as u8).collect(); // incompressible -> stored
    let sec2_plain = vec![0u8; 0x1000]; // compressible -> zstd
    let mut body = ctr_encrypt(&TEST_CTR, 0x4000, &sec1_plain);
    body.extend_from_slice(&sec2_plain);
    // Split into 16 KiB blocks (exponent 14): 0x9000 -> 2 full blocks
    // + 0x1000 partial.
    let blocks: Vec<&[u8]> = body.chunks(1 << 14).collect();
    let ncz = block_ncz(&header, &sections, 14, &blocks);
    let out = decompress_all(&ncz).unwrap();
    let mut expected = header;
    expected.extend_from_slice(&sec1_plain);
    expected.extend_from_slice(&sec2_plain);
    assert_eq!(out, expected);
}

#[test]
fn truncated_solid_stream_errors() {
    let header = vec![0u8; NCA_HEADER_SIZE as usize];
    let sections = vec![Section {
        offset: 0x4000,
        size: 0x2000,
        crypto_type: 0,
        key: [0; 16],
        counter: [0; 16],
    }];
    let body = pattern(0x11, 0x2000);
    let mut ncz = solid_ncz(&header, &sections, &body);
    // Chop the zstd stream short.
    ncz.truncate(ncz.len() - 20);
    let err = decompress_all(&ncz).expect_err("must fail");
    assert!(err.to_string().contains("NCZ"), "unexpected error: {err}");
}

#[test]
fn truncated_block_stream_errors() {
    let header = vec![0u8; NCA_HEADER_SIZE as usize];
    let sections = vec![Section {
        offset: 0x4000,
        size: 0x9000,
        crypto_type: 0,
        key: [0; 16],
        counter: [0; 16],
    }];
    let body = pattern(0x22, 0x9000);
    let blocks: Vec<&[u8]> = body.chunks(1 << 14).collect();
    let mut ncz = block_ncz(&header, &sections, 14, &blocks);
    // Cut the last block's compressed data in half.
    ncz.truncate(ncz.len() - 64);
    let err = decompress_all(&ncz).expect_err("must fail");
    assert!(err.to_string().contains("NCZ"), "unexpected error: {err}");
}

#[test]
fn bad_magic() {
    let ncz = vec![0u8; NCA_HEADER_SIZE as usize + 0x100];
    assert!(read_header(&mut Cursor::new(ncz.clone()), 0, ncz.len() as u64).is_err());
}

#[test]
fn unsorted_sections_error() {
    let header = vec![0u8; NCA_HEADER_SIZE as usize];
    let sections = vec![
        Section {
            offset: 0x5000,
            size: 0x100,
            crypto_type: 0,
            key: [0; 16],
            counter: [0; 16],
        },
        Section {
            offset: 0x4800,
            size: 0x100,
            crypto_type: 0,
            key: [0; 16],
            counter: [0; 16],
        },
    ];
    let body = vec![0u8; 0x200];
    let ncz = solid_ncz(&header, &sections, &body);
    let mut cur = Cursor::new(ncz.clone());
    let err = read_header(&mut cur, 0, ncz.len() as u64).expect_err("must fail");
    assert!(err.to_string().contains("sorted"));
}

#[test]
fn block_exponent_out_of_range() {
    let header = vec![0u8; NCA_HEADER_SIZE as usize];
    let sections = vec![Section {
        offset: 0x4000,
        size: 0x100,
        crypto_type: 0,
        key: [0; 16],
        counter: [0; 16],
    }];
    let mut ncz = block_ncz(&header, &sections, 20, &[&vec![0u8; 0x100][..]]);
    // Corrupt the exponent byte (fixed[11]).
    let exponent_pos = NCA_HEADER_SIZE as usize + 16 + SECTION_WIRE as usize + 11;
    ncz[exponent_pos] = 13;
    let mut cur = Cursor::new(ncz.clone());
    let err = read_header(&mut cur, 0, ncz.len() as u64).expect_err("must fail");
    assert!(err.to_string().contains("exponent"));
}

#[test]
fn verify_content_id_matches() {
    let mut nca = vec![0u8; 0x6000];
    nca[0] = 1;
    let hash: [u8; 32] = Sha256::digest(&nca).into();
    let name = format!("{}.nca", hex::encode(&hash[..16]));
    verify_content_id(&hash, &name).unwrap();
    // Wrong id fails.
    let wrong = format!("{}.nca", "f".repeat(32));
    assert!(matches!(
        verify_content_id(&hash, &wrong),
        Err(Error::HashMismatch { .. })
    ));
    // Non-hex name cannot be verified.
    assert!(verify_content_id(&hash, "not-hex.nca").is_err());
}

#[test]
fn nsz_to_nsp_end_to_end() {
    let dir = temp_dir("nsz2nsp");
    let header = pattern(0x40, NCA_HEADER_SIZE as usize);
    let sections = vec![Section {
        offset: 0x4000,
        size: 0x2000,
        crypto_type: 3,
        key: TEST_KEY,
        counter: TEST_CTR,
    }];
    let plain = pattern(0x77, 0x2000);
    let body = ctr_encrypt(&TEST_CTR, 0x4000, &plain);
    let ncz = solid_ncz(&header, &sections, &body);
    let mut expected_nca = header.clone();
    expected_nca.extend_from_slice(&plain);
    let hash: [u8; 32] = Sha256::digest(&expected_nca).into();
    let ncz_name = format!("{}.ncz", hex::encode(&hash[..16]));

    let mut nsz = Vec::new();
    {
        let specs = vec![
            EntrySpec {
                name: "cnmt.xml".into(),
                size: 4,
            },
            EntrySpec {
                name: ncz_name.clone(),
                size: ncz.len() as u64,
            },
            EntrySpec {
                name: "plain.bin".into(),
                size: 4,
            },
        ];
        let mut w = Pfs0Writer::new(&mut nsz, specs).unwrap();
        w.write_next(&mut Cursor::new(b"<x/>".to_vec())).unwrap();
        w.write_next(&mut Cursor::new(ncz.clone())).unwrap();
        w.write_next(&mut Cursor::new(vec![1, 2, 3, 4])).unwrap();
        w.finish().unwrap();
    }
    let in_path = dir.join("in.nsz");
    let out_path = dir.join("out.nsp");
    fs::write(&in_path, &nsz).unwrap();

    let cancel = AtomicBool::new(false);
    let mut ticks = Vec::new();
    let summary = nsz_to_nsp(
        &in_path,
        &out_path,
        &mut |d, t, e| ticks.push((d, t, e.to_owned())),
        &cancel,
    )
    .unwrap();
    assert_eq!(summary.entries.len(), 3);
    assert_eq!(
        summary.entries[1].name,
        format!("{}.nca", hex::encode(&hash[..16]))
    );
    assert!(summary.entries[1].decompressed);
    assert!(summary.entries[1].sha256.is_some());
    assert!(!summary.entries[2].decompressed);
    assert_eq!(summary.entries[2].output_size, 4);
    assert!(ticks.iter().any(|(_, _, e)| e == "cnmt.xml"));
    assert!(
        !out_path.with_file_name("out.nsp.part").exists(),
        "part file must be renamed away"
    );

    // Read the produced NSP back and check the entries.
    let f = File::open(&out_path).unwrap();
    let mut p = Pfs0::read(BufReader::new(f)).unwrap();
    assert_eq!(p.entries()[0].name, "cnmt.xml");
    assert!(p.entries()[1].name.ends_with(".nca"));
    assert_eq!(p.entries()[1].size, expected_nca.len() as u64);
    assert_eq!(p.entries()[2].name, "plain.bin");
    let mut er = p.entry_reader(1).unwrap();
    let mut produced = vec![0u8; expected_nca.len()];
    er.read_exact(&mut produced).unwrap();
    assert_eq!(produced, expected_nca);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn nsz_to_nsp_cancellation_cleans_up() {
    let dir = temp_dir("nsz2nsp-cancel");
    let header = vec![0u8; NCA_HEADER_SIZE as usize];
    let sections = vec![Section {
        offset: 0x4000,
        size: 0x2000,
        crypto_type: 0,
        key: [0; 16],
        counter: [0; 16],
    }];
    let body = vec![0x5Au8; 0x2000];
    let ncz = solid_ncz(&header, &sections, &body);
    let mut nca = header.clone();
    nca.extend_from_slice(&body);
    let hash: [u8; 32] = Sha256::digest(&nca).into();
    let mut nsz = Vec::new();
    {
        let specs = vec![
            EntrySpec {
                name: format!("{}.ncz", hex::encode(&hash[..16])),
                size: ncz.len() as u64,
            },
            EntrySpec {
                name: "big.bin".into(),
                size: 8 << 20,
            },
        ];
        let mut w = Pfs0Writer::new(&mut nsz, specs).unwrap();
        w.write_next(&mut Cursor::new(ncz)).unwrap();
        w.write_next(&mut Cursor::new(vec![0u8; 8 << 20])).unwrap();
        w.finish().unwrap();
    }
    let in_path = dir.join("in.nsz");
    let out_path = dir.join("out.nsp");
    fs::write(&in_path, &nsz).unwrap();

    // Cancel after the first entry completes.
    let cancel = AtomicBool::new(false);
    let mut seen_ncz_done = false;
    let result = nsz_to_nsp(
        &in_path,
        &out_path,
        &mut |_d, _t, entry| {
            if entry.ends_with(".nca") {
                seen_ncz_done = true;
            }
            if seen_ncz_done && !entry.ends_with(".nca") {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    match result {
        Err(Error::Cancelled) => {}
        other => panic!("expected Cancelled, got {other:?}"),
    }
    assert!(!out_path.exists(), "no output on cancellation");
    assert!(
        !out_path.with_file_name("out.nsp.part").exists(),
        "partial output must be deleted on cancellation"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn nsz_to_nsp_hash_mismatch_deletes_output() {
    let dir = temp_dir("nsz2nsp-fail");
    let header = vec![0u8; NCA_HEADER_SIZE as usize];
    let sections = vec![Section {
        offset: 0x4000,
        size: 0x1000,
        crypto_type: 0,
        key: [0; 16],
        counter: [0; 16],
    }];
    let body = vec![0x55u8; 0x1000];
    let ncz = solid_ncz(&header, &sections, &body);
    let mut nsz = Vec::new();
    {
        let specs = vec![EntrySpec {
            name: format!("{}.ncz", "e".repeat(32)), // wrong content id
            size: ncz.len() as u64,
        }];
        let mut w = Pfs0Writer::new(&mut nsz, specs).unwrap();
        w.write_next(&mut Cursor::new(ncz)).unwrap();
        w.finish().unwrap();
    }
    let in_path = dir.join("in.nsz");
    let out_path = dir.join("out.nsp");
    fs::write(&in_path, &nsz).unwrap();

    let cancel = AtomicBool::new(false);
    let err = nsz_to_nsp(&in_path, &out_path, &mut |_, _, _| {}, &cancel).expect_err("must fail");
    assert!(matches!(err, Error::HashMismatch { .. }), "got: {err}");
    assert!(!out_path.exists(), "no output on failure");
    assert!(
        !out_path.with_file_name("out.nsp.part").exists(),
        "partial output must be deleted on failure"
    );
    let _ = fs::remove_dir_all(&dir);
}
