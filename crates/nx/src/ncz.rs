// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! NCZ decompression and NSZ -> NSP conversion.
//!
//! # The NCZ format
//!
//! An NCZ entry (`.ncz`) inside an NSZ container is an NCA recompressed with
//! zstd. The layout is:
//!
//! ```text
//! 0x0000            original NCA header (0x4000 bytes, stored verbatim)
//! 0x4000            magic "NCZSECTN" (8 bytes)
//! 0x4008            section count (u64 LE)
//! 0x4010            section table, 0x40 bytes per section:
//!                     offset (s64)   - absolute offset in the NCA
//!                     size   (s64)
//!                     crypto type (u64) - 0/1/2: stored plaintext,
//!                                         3: AES-128-CTR, 4: BKTR AES-128-CTR
//!                     padding (8)
//!                     key     (16)  - AES key for CTR sections
//!                     counter (16)  - high 8 bytes of the CTR counter
//! then, either:
//!   "NCZBLOCK"      block header (0x18 + n*4 bytes of per-block sizes);
//!                   followed by n independently zstd-compressed blocks
//!                   (2^exponent bytes each, exponent 14..=32) OR
//!   a single solid zstd stream covering the whole body.
//! ```
//!
//! The compressed *body* is the NCA byte range `[0x4000, nca_size)` with
//! CTR-encrypted sections stored **decrypted**, in section-table order
//! (tables in the wild are sorted by offset and contiguous). If the first
//! section starts after 0x4000, the uncovered range `[0x4000,
//! first_offset)` is stored plaintext at the front of the body. Any gap
//! between sections is zero padding in the original NCA and is not stored.
//!
//! Decompression writes: the NCA header verbatim, the uncovered range
//! verbatim, every section at its absolute offset (re-encrypting types 3/4
//! with AES-128-CTR; the low 8 bytes of the counter are the absolute NCA
//! offset divided by 16), and zeros for gaps.
//!
//! # Verification
//!
//! NSZ file names carry the NCA *content id*: the first 16 bytes of the
//! SHA-256 of the reconstructed NCA, in hex. After producing an NCA the
//! whole output is hashed (streaming) and the digest prefix must match the
//! entry's name id, exactly like nsz's own verifier.
//!
//! # References
//!
//! Ported from the format implementations in:
//! * Eden Emulator `ncz_decompressor.cpp` (GPL-3.0-or-later)
//! * the nsz project `BlockDecompressorReader.py` / `Decompressor.py`
//!   (MIT)

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ctr::cipher::{KeyIvInit, StreamCipher};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::pfs0::{self, EntrySpec, Pfs0, Pfs0Writer};

/// Size of the NCA header region stored verbatim in the NCZ.
pub const NCA_HEADER_SIZE: u64 = 0x4000;

const SECTIONS_MAGIC: [u8; 8] = *b"NCZSECTN";
const BLOCK_MAGIC: [u8; 8] = *b"NCZBLOCK";
/// Wire size of one section-table entry.
const SECTION_WIRE: u64 = 0x40;
/// Size of the fixed part of the NCZBLOCK header (before the size list).
const BLOCK_FIXED: u64 = 0x18;
/// Upper bound on section-table entries (64 bytes each -> at most 4 MiB of
/// table). BKTR update NCAs store one entry per subsection (thousands).
const MAX_SECTIONS: u64 = 0x10000;
/// Chunk size for streaming (bounded memory).
const CHUNK: usize = 1 << 20;
/// I/O buffer size for the output file during conversion.
const OUT_BUF: usize = 8 << 20;

/// Crypto types stored in the section table.
const CRYPTO_CTR: u64 = 3;
const CRYPTO_BKTR: u64 = 4;

/// One entry of the NCZ section table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    /// Absolute offset of the section in the NCA (s64 on disk; negative
    /// values are clamped to the header end when reconstructing).
    pub offset: i64,
    /// Section size in bytes.
    pub size: i64,
    /// Crypto type: 3 = AES-128-CTR, 4 = BKTR AES-128-CTR (both stored
    /// decrypted and re-encrypted on decompression); anything else is
    /// stored plaintext.
    pub crypto_type: u64,
    /// AES-128 key used for CTR sections.
    pub key: [u8; 16],
    /// High 8 bytes of the CTR counter; the low 8 bytes are derived from the
    /// absolute NCA offset (`offset >> 4`).
    pub counter: [u8; 16],
}

impl Section {
    /// First NCA byte this section covers (clamped to the header end).
    fn begin(&self) -> u64 {
        self.offset.max(NCA_HEADER_SIZE as i64) as u64
    }

    /// One past the last NCA byte this section covers (>= `begin`).
    fn end(&self) -> u64 {
        self.offset
            .saturating_add(self.size)
            .max(self.offset)
            .max(NCA_HEADER_SIZE as i64) as u64
    }

    /// Whether the section content is stored decrypted and must be
    /// re-encrypted with AES-128-CTR on decompression.
    fn is_ctr(&self) -> bool {
        matches!(self.crypto_type, CRYPTO_CTR | CRYPTO_BKTR)
    }
}

/// Parsed NCZBLOCK header (block-compressed body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeader {
    /// Block size in bytes (2^exponent, 2^14..=2^32).
    pub block_size: u64,
    /// Decompressed body size in bytes (nca_size - 0x4000).
    pub decompressed_size: u64,
    /// Per-block compressed sizes; a block whose entry is >= the block size
    /// is stored raw.
    pub sizes: Vec<u32>,
    /// Entry-relative offset of the first block's compressed data.
    pub payload_start: u64,
}

/// A fully parsed NCZ header. All offsets are relative to the start of the
/// NCZ entry.
#[derive(Debug, Clone)]
pub struct NczHeader {
    /// Section table in body order (sorted by offset, zero-sized entries
    /// dropped). When the first section starts after the NCA header, a
    /// synthetic plaintext section covering the uncovered range is inserted
    /// at the front — the uncovered bytes are part of the body.
    pub sections: Vec<Section>,
    /// Block-compression header, or `None` for a solid zstd stream.
    pub block: Option<BlockHeader>,
    /// Entry-relative offset where the compressed body starts.
    pub payload_start: u64,
    /// Size of the reconstructed NCA (0x4000 + body).
    pub nca_size: u64,
    /// Size of the NCZ entry in bytes.
    pub entry_size: u64,
}

fn le64(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes(b[..4].try_into().unwrap())
}

/// Parses the NCZ header of the entry starting at `base` with size
/// `entry_size` in `f`. `f` may be positioned anywhere; it is left at an
/// unspecified position.
pub fn read_header<R: Read + Seek>(f: &mut R, base: u64, entry_size: u64) -> Result<NczHeader> {
    if entry_size < NCA_HEADER_SIZE + 16 {
        return Err(Error::format("NCZ entry too small to hold a header"));
    }
    f.seek(SeekFrom::Start(base + NCA_HEADER_SIZE))
        .map_err(Error::Io)?;
    let mut magic_count = [0u8; 16];
    f.read_exact(&mut magic_count).map_err(Error::Io)?;
    if magic_count[..8] != SECTIONS_MAGIC {
        return Err(Error::format("NCZ: NCZSECTN magic not found"));
    }
    let count = le64(&magic_count[8..]);
    if count == 0 || count > MAX_SECTIONS {
        return Err(Error::format(format!("NCZ: invalid section count {count}")));
    }
    let table_size = count * SECTION_WIRE;
    if NCA_HEADER_SIZE + 16 + table_size > entry_size {
        return Err(Error::format("NCZ: section table exceeds the entry size"));
    }

    let mut table = vec![0u8; table_size as usize];
    f.read_exact(&mut table).map_err(Error::Io)?;
    let mut sections = Vec::with_capacity(count as usize + 1);
    for i in 0..count as usize {
        let e = &table[i * SECTION_WIRE as usize..][..SECTION_WIRE as usize];
        let s = Section {
            offset: le64(&e[0..]) as i64,
            size: le64(&e[8..]) as i64,
            crypto_type: le64(&e[16..]),
            key: e[32..48].try_into().unwrap(),
            counter: e[48..64].try_into().unwrap(),
        };
        if s.size <= 0 {
            continue; // zero-sized entries consume no body and no output
        }
        sections.push(s);
    }
    if sections.is_empty() {
        return Err(Error::format("NCZ: no non-empty sections"));
    }
    // The body is the concatenation of section data in table order, so the
    // table must be sorted by offset for the reconstruction to be possible.
    for w in sections.windows(2) {
        if w[1].offset < w[0].offset {
            return Err(Error::format("NCZ: section table is not sorted by offset"));
        }
    }
    // Uncovered range between the header and the first section: stored
    // plaintext at the front of the body (mirrors nsz/Eden).
    if sections[0].offset > NCA_HEADER_SIZE as i64 {
        let gap = sections[0].offset as u64 - NCA_HEADER_SIZE;
        sections.insert(
            0,
            Section {
                offset: NCA_HEADER_SIZE as i64,
                size: gap as i64,
                crypto_type: 0,
                key: [0; 16],
                counter: [0; 16],
            },
        );
    }

    // Optional NCZBLOCK header right after the table.
    let block_pos = NCA_HEADER_SIZE + 16 + table_size;
    let block = if entry_size >= block_pos + BLOCK_FIXED {
        f.seek(SeekFrom::Start(base + block_pos))
            .map_err(Error::Io)?;
        let mut fixed = [0u8; BLOCK_FIXED as usize];
        f.read_exact(&mut fixed).map_err(Error::Io)?;
        if fixed[..8] == BLOCK_MAGIC {
            let exponent = fixed[11] as u32;
            if !(14..=32).contains(&exponent) {
                return Err(Error::format(format!(
                    "NCZ: NCZBLOCK exponent {exponent} out of range (14..=32)"
                )));
            }
            let nblocks = le32(&fixed[12..]) as u64;
            if nblocks == 0 {
                return Err(Error::format("NCZ: NCZBLOCK declares zero blocks"));
            }
            let size_list = nblocks * 4;
            let payload_start = block_pos + BLOCK_FIXED + size_list;
            if payload_start > entry_size {
                return Err(Error::format(
                    "NCZ: NCZBLOCK size list exceeds the entry size",
                ));
            }
            let mut sizes = vec![0u32; nblocks as usize];
            let mut sum = 0u64;
            for slot in &mut sizes {
                let mut raw = [0u8; 4];
                f.read_exact(&mut raw).map_err(Error::Io)?;
                *slot = le32(&raw);
                sum += *slot as u64;
            }
            if sum > entry_size - payload_start {
                return Err(Error::format(
                    "NCZ: NCZBLOCK compressed data exceeds the entry size",
                ));
            }
            Some(BlockHeader {
                block_size: 1u64 << exponent,
                decompressed_size: le64(&fixed[16..]),
                sizes,
                payload_start,
            })
        } else {
            None
        }
    } else {
        None
    };

    let payload_start = block.as_ref().map(|b| b.payload_start).unwrap_or(block_pos);
    let nca_size = sections
        .iter()
        .map(Section::end)
        .max()
        .unwrap_or(NCA_HEADER_SIZE)
        .max(NCA_HEADER_SIZE);

    Ok(NczHeader {
        sections,
        block,
        payload_start,
        nca_size,
        entry_size,
    })
}

/// A `Read` view limited to exactly `remaining` bytes (used to bound body
/// readers to the NCZ entry so a corrupt stream cannot bleed into the next
/// PFS0 entry).
struct SubRead<'a, R> {
    inner: &'a mut R,
    remaining: u64,
}

impl<R: Read> Read for SubRead<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let want = (buf.len() as u64).min(self.remaining) as usize;
        let n = self.inner.read(&mut buf[..want])?;
        self.remaining -= n as u64;
        Ok(n)
    }
}

fn zstd_error(context: &str, e: std::io::Error) -> Error {
    // The zstd crate reports decode failures as `InvalidData` or `Other`;
    // errors from the underlying file reader keep their original kinds and
    // stay plain I/O errors.
    match e.kind() {
        std::io::ErrorKind::InvalidData
        | std::io::ErrorKind::Other
        | std::io::ErrorKind::UnexpectedEof => {
            Error::format(format!("{context}: zstd decode error: {e}"))
        }
        _ => Error::Io(e),
    }
}

/// Streaming reader over a solid (single-frame) zstd body.
struct SolidBody<'a, R: Read> {
    dec: zstd::stream::read::Decoder<'static, std::io::BufReader<SubRead<'a, R>>>,
}

impl<'a, R: Read> SolidBody<'a, R> {
    fn new(src: SubRead<'a, R>) -> Result<Self> {
        let mut dec =
            zstd::stream::read::Decoder::new(src).map_err(|e| zstd_error("solid NCZ", e))?;
        // nsz solid streams are written without a window descriptor, so the
        // frame may use a window up to 2 GiB; Eden sets the same limit.
        dec.window_log_max(31)
            .map_err(|e| zstd_error("solid NCZ", e))?;
        Ok(Self { dec })
    }

    /// Fills `out` with the next decompressed bytes. Returns 0 at the end of
    /// the frame.
    fn read_chunk(&mut self, out: &mut [u8]) -> Result<usize> {
        self.dec.read(out).map_err(|e| zstd_error("solid NCZ", e))
    }
}

/// Streaming reader over an NCZBLOCK body. Each block is decoded on demand
/// into an owned buffer (at most one block in memory, the same model as the
/// Eden and nsz reference implementations) and served incrementally; blocks
/// beyond the end of the last section are never decoded.
struct BlockBody<'a, R: Read + Seek> {
    src: &'a mut R,
    block_size: u64,
    decompressed_size: u64,
    sizes: Vec<u32>,
    payload_start: u64,
    /// Index of the block currently being served.
    block_idx: u64,
    /// Entry-relative offset of the current block's compressed data.
    block_offset_sum: u64,
    /// Decompressed contents of the current block.
    block_buf: Vec<u8>,
    /// Bytes of `block_buf` already served.
    buf_pos: usize,
    /// Total decompressed bytes served.
    pos: u64,
}

impl<'a, R: Read + Seek> BlockBody<'a, R> {
    fn new(src: &'a mut R, bh: &BlockHeader) -> Self {
        Self {
            src,
            block_size: bh.block_size,
            decompressed_size: bh.decompressed_size,
            sizes: bh.sizes.clone(),
            payload_start: bh.payload_start,
            block_idx: 0,
            block_offset_sum: 0,
            block_buf: Vec::new(),
            buf_pos: 0,
            pos: 0,
        }
    }

    /// Size in bytes the block decompresses to (smaller for the last block
    /// when the body size is not a multiple of the block size — this also
    /// avoids the original nsz last-stored-block bug).
    fn block_want(&self, idx: u64) -> u64 {
        let mut want = self.block_size;
        if idx + 1 == self.sizes.len() as u64 {
            let rem = self.decompressed_size % self.block_size;
            if rem > 0 {
                want = rem;
            }
        }
        want
    }

    /// Moves to the next block (updating the compressed-offset accumulator).
    fn advance_block(&mut self) {
        if let Some(&csize) = self.sizes.get(self.block_idx as usize) {
            self.block_offset_sum += csize as u64;
        }
        self.block_idx += 1;
        self.block_buf.clear();
        self.buf_pos = 0;
    }

    /// Decodes the current block into `block_buf` (or reads it raw when it
    /// was stored uncompressed).
    fn load_block(&mut self) -> Result<()> {
        let idx = self.block_idx;
        let want = self.block_want(idx);
        let csize = self.sizes[idx as usize] as u64;
        self.src
            .seek(SeekFrom::Start(self.payload_start + self.block_offset_sum))
            .map_err(Error::Io)?;
        if csize >= want {
            // Stored (incompressible) block.
            self.block_buf.resize(want as usize, 0);
            self.src
                .read_exact(&mut self.block_buf)
                .map_err(Error::Io)?;
            return Ok(());
        }
        // Compressed block: stream-decode exactly `csize` compressed bytes.
        let sub = SubRead {
            inner: self.src,
            remaining: csize,
        };
        let mut dec =
            zstd::stream::read::Decoder::new(sub).map_err(|e| zstd_error("NCZBLOCK", e))?;
        dec.window_log_max(31)
            .map_err(|e| zstd_error("NCZBLOCK", e))?;
        self.block_buf.clear();
        dec.take(want + 1)
            .read_to_end(&mut self.block_buf)
            .map_err(|e| zstd_error("NCZBLOCK", e))?;
        if self.block_buf.len() as u64 != want {
            return Err(Error::format(format!(
                "NCZBLOCK block {idx}: decompressed to {} bytes, expected {want}",
                self.block_buf.len()
            )));
        }
        Ok(())
    }

    /// Fills `out` with the next decompressed body bytes; returns 0 at the
    /// end of the body.
    fn read_chunk(&mut self, out: &mut [u8]) -> Result<usize> {
        loop {
            if self.pos >= self.decompressed_size || self.block_idx as usize >= self.sizes.len() {
                return Ok(0);
            }
            let want = self.block_want(self.block_idx);
            if want == 0 {
                self.advance_block();
                continue; // degenerate final block
            }
            if self.buf_pos == self.block_buf.len() {
                self.load_block()?;
                self.buf_pos = 0;
                if self.block_buf.is_empty() {
                    // Defensive: block produced nothing (already handled by
                    // want == 0 above, but guard anyway).
                    self.advance_block();
                    continue;
                }
            }
            let take = out.len().min(self.block_buf.len() - self.buf_pos);
            out[..take].copy_from_slice(&self.block_buf[self.buf_pos..self.buf_pos + take]);
            self.buf_pos += take;
            self.pos += take as u64;
            if self.buf_pos == self.block_buf.len() {
                self.advance_block();
            }
            return Ok(take);
        }
    }
}

/// Either body variant behind one interface.
enum Body<'a, R: Read + Seek> {
    Solid(SolidBody<'a, R>),
    Block(BlockBody<'a, R>),
}

impl<R: Read + Seek> Body<'_, R> {
    fn read_chunk(&mut self, out: &mut [u8]) -> Result<usize> {
        match self {
            Body::Solid(s) => s.read_chunk(out),
            Body::Block(b) => b.read_chunk(out),
        }
    }
}

/// Encrypts `data` in place with the section's CTR keystream.
///
/// The input is always a continuation of the section byte stream. Buffers
/// other than a section's final one are required to be whole 16-byte
/// blocks; the final buffer may end mid-block and is handled through
/// `scratch` (the cipher is discarded right after, so the keystream
/// position no longer matters). This keeps the keystream position exact
/// across chunk boundaries.
fn ctr_encrypt_chunk(
    cipher: &mut ctr::Ctr128BE<aes::Aes128>,
    data: &mut [u8],
    scratch: &mut [u8; 16],
) {
    let full = data.len() / 16 * 16;
    if full > 0 {
        cipher.apply_keystream(&mut data[..full]);
    }
    let rest = data.len() - full;
    if rest > 0 {
        scratch[..rest].copy_from_slice(&data[full..]);
        cipher.apply_keystream(&mut scratch[..]);
        data[full..].copy_from_slice(&scratch[..rest]);
    }
}

/// Output of decompressing one NCZ entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NczOutput {
    /// Size of the reconstructed NCA in bytes.
    pub nca_size: u64,
    /// SHA-256 of the reconstructed NCA (header + gaps + sections).
    pub sha256: [u8; 32],
}

/// Streaming NCZ -> NCA decompression.
///
/// `input` must be a `Read + Seek` positioned at the *start of the NCZ
/// entry* (`base` = 0; all offsets in `header` are entry-relative). `output`
/// receives the NCA sequentially (it does not need to be seekable).
/// `progress` is called with the number of NCA bytes written so far;
/// `cancel` is polled between chunks and aborts with [`Error::Cancelled`]
/// when set.
///
/// Memory use is bounded by a few 1 MiB buffers regardless of entry size.
pub fn decompress_to<R: Read + Seek, W: Write>(
    input: &mut R,
    header: &NczHeader,
    output: &mut W,
    mut progress: impl FnMut(u64),
    cancel: &AtomicBool,
) -> Result<NczOutput> {
    // 1. NCA header, verbatim.
    let mut hasher = Sha256::new();
    let mut written: u64 = 0;
    {
        let mut buf = vec![0u8; NCA_HEADER_SIZE as usize];
        input.seek(SeekFrom::Start(0)).map_err(Error::Io)?;
        input.read_exact(&mut buf).map_err(Error::Io)?;
        output.write_all(&buf).map_err(Error::Io)?;
        hasher.update(&buf);
        written += NCA_HEADER_SIZE;
        progress(written);
    }

    // 2. Body reader: solid zstd stream or on-demand blocks, both capped to
    // the entry.
    let mut body = match &header.block {
        Some(bh) => {
            input
                .seek(SeekFrom::Start(bh.payload_start))
                .map_err(Error::Io)?;
            Body::Block(BlockBody::new(input, bh))
        }
        None => {
            input
                .seek(SeekFrom::Start(header.payload_start))
                .map_err(Error::Io)?;
            let sub = SubRead {
                inner: input,
                remaining: header.entry_size - header.payload_start,
            };
            Body::Solid(SolidBody::new(sub)?)
        }
    };

    // 3. Walk the sections in table order: zero-fill gaps, re-encrypt CTR
    // sections, write everything sequentially.
    let mut chunk = vec![0u8; CHUNK];
    let zeros = vec![0u8; CHUNK];
    let mut scratch = [0u8; 16];
    let mut out_pos = NCA_HEADER_SIZE;

    for section in &header.sections {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let begin = section.begin();
        let end = section.end();
        let len = end - begin;
        if len == 0 {
            continue;
        }
        if begin < out_pos {
            return Err(Error::format("NCZ: overlapping or unsorted section ranges"));
        }
        // Zero-fill the gap before this section (original NCA padding).
        let mut gap = begin - out_pos;
        while gap > 0 {
            let take = gap.min(zeros.len() as u64) as usize;
            output.write_all(&zeros[..take]).map_err(Error::Io)?;
            hasher.update(&zeros[..take]);
            gap -= take as u64;
            written += take as u64;
            out_pos += take as u64;
            progress(out_pos);
        }

        let mut cipher = section.is_ctr().then(|| {
            let mut iv = section.counter;
            iv[8..].copy_from_slice(&(begin >> 4).to_be_bytes());
            ctr::Ctr128BE::<aes::Aes128>::new((&section.key).into(), (&iv).into())
        });

        let mut consumed: u64 = 0;
        while consumed < len {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let take = (len - consumed).min(chunk.len() as u64) as usize;
            // Fill the chunk completely: the body reader may return short
            // reads, and every chunk except the section's last must be a
            // whole number of 16-byte blocks so the CTR keystream position
            // stays exact (partial blocks are only legal at the very end of
            // a section, where the cipher is discarded).
            let mut filled = 0;
            while filled < take {
                let n = body.read_chunk(&mut chunk[filled..take])?;
                if n == 0 {
                    return Err(Error::format(
                        "NCZ: compressed body ended before the last section",
                    ));
                }
                filled += n;
            }
            if let Some(c) = cipher.as_mut() {
                ctr_encrypt_chunk(c, &mut chunk[..filled], &mut scratch);
            }
            output.write_all(&chunk[..filled]).map_err(Error::Io)?;
            hasher.update(&chunk[..filled]);
            consumed += filled as u64;
            written += filled as u64;
            out_pos += filled as u64;
            progress(out_pos);
        }
    }

    output.flush().map_err(Error::Io)?;
    Ok(NczOutput {
        nca_size: written.max(header.nca_size),
        sha256: hasher.finalize().into(),
    })
}

/// Verifies that the first 16 bytes of an NCA's SHA-256 (hex) match the
/// content id embedded in its file name (`<32 hex chars>.nca`), i.e. the
/// same check nsz performs after decompression.
pub fn verify_content_id(sha256: &[u8; 32], name: &str) -> Result<()> {
    let stem = name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(name)
        .to_ascii_lowercase();
    let actual = hex::encode(&sha256[..16]);
    if stem.len() != 32 || !stem.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::format(format!(
            "cannot verify NCA '{name}': name id '{stem}' is not a 32-char hex content id"
        )));
    }
    if actual == stem {
        Ok(())
    } else {
        Err(Error::HashMismatch {
            name: name.to_owned(),
            expected: stem,
            actual,
        })
    }
}

// ---------------------------------------------------------------------------
// NSZ -> NSP conversion
// ---------------------------------------------------------------------------

/// Summary of one converted entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryEntry {
    /// Name of the source entry in the NSZ.
    pub input_name: String,
    /// Name of the entry in the produced NSP.
    pub name: String,
    /// Size of the entry in the produced NSP.
    pub output_size: u64,
    /// SHA-256 of the produced NCA for decompressed entries.
    pub sha256: Option<[u8; 32]>,
    /// True when the entry was NCZ-decompressed (`.ncz` -> `.nca`).
    pub decompressed: bool,
}

/// Summary of an `nsz_to_nsp` conversion.
#[derive(Debug, Clone)]
pub struct Summary {
    /// Per-entry results, in container order.
    pub entries: Vec<SummaryEntry>,
    /// Total number of bytes written to the output (header + entries).
    pub total_output_size: u64,
    /// Wall-clock time spent converting.
    pub elapsed: Duration,
}

/// Renames `src` over `dst`; on Windows `rename` fails if `dst` exists, so
/// an existing destination is removed first (best effort).
fn rename_over(src: &Path, dst: &Path) -> Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) if dst.exists() => {
            fs::remove_file(dst).map_err(Error::Io)?;
            fs::rename(src, dst).map_err(Error::Io)
        }
        Err(e) => Err(Error::Io(e)),
    }
}

/// Converts an NSZ container into an NSP container:
///
/// * the PFS0 is rewritten with every `.ncz` entry replaced by its
///   decompressed `.nca` (name suffix swapped, verified below);
/// * all other entries are copied verbatim;
/// * every produced NCA is verified: the first 16 bytes of its SHA-256
///   (hex) must equal the content id in its file name. On any mismatch —
///   or any other error, or cancellation — the partial output is deleted
///   and an error is returned.
///
/// The output is written to `<output>.part` and atomically renamed to
/// `output` on success.
///
/// `progress` receives `(done_bytes, total_bytes, current_entry_name)`;
/// `cancel` aborts the conversion (and cleans up) when set.
pub fn nsz_to_nsp(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    progress: &mut dyn FnMut(u64, u64, &str),
    cancel: &AtomicBool,
) -> Result<Summary> {
    let input = input.as_ref();
    let output = output.as_ref();
    let started = Instant::now();

    let file = File::open(input).map_err(Error::Io)?;
    let mut pfs0 = Pfs0::read(BufReader::new(file))?;

    // Snapshot entry names/sizes so the file can be borrowed mutably below.
    let entry_names: Vec<String> = pfs0.entries().iter().map(|e| e.name.clone()).collect();
    let entry_sizes: Vec<u64> = pfs0.entries().iter().map(|e| e.size).collect();

    // Plan: parse every NCZ header up front so the PFS0 header (which needs
    // all output sizes) can be written before the data is streamed.
    let mut plan: Vec<(String, u64)> = Vec::with_capacity(entry_names.len());
    let mut entry_is_ncz = Vec::with_capacity(entry_names.len());
    for i in 0..entry_names.len() {
        let lower = entry_names[i].to_ascii_lowercase();
        if lower.ends_with(".ncz") {
            let mut er = pfs0.entry_reader(i)?;
            let hdr = read_header(&mut er, 0, entry_sizes[i])?;
            let out_name = format!("{}.nca", &entry_names[i][..entry_names[i].len() - 4]);
            plan.push((out_name, hdr.nca_size));
            entry_is_ncz.push(true);
        } else {
            plan.push((entry_names[i].clone(), entry_sizes[i]));
            entry_is_ncz.push(false);
        }
    }

    let specs: Vec<EntrySpec> = plan
        .iter()
        .map(|(name, size)| EntrySpec {
            name: name.clone(),
            size: *size,
        })
        .collect();
    let (_, data_start) = pfs0::build_header(&specs)?;
    let total = data_start + plan.iter().map(|(_, s)| s).sum::<u64>();

    // Output goes to "<output>.part" and is renamed into place on success.
    let part_path = part_path_for(output);
    let mut out_file = File::create(&part_path).map_err(Error::Io)?;
    let mut buf = BufWriter::with_capacity(OUT_BUF, &mut out_file);
    let mut writer = Pfs0Writer::new(&mut buf, specs)?;
    let mut done = data_start;
    let mut summaries = Vec::with_capacity(plan.len());

    let result = (|| -> Result<()> {
        for i in 0..plan.len() {
            let (out_name, _) = &plan[i];
            progress(done, total, out_name);
            let mut er = pfs0.entry_reader(i)?;
            if entry_is_ncz[i] {
                let hdr = read_header(&mut er, 0, entry_sizes[i])?;
                let entry_base = done;
                let mut sink = Pfs0EntrySink {
                    inner: &mut writer,
                    hasher: Sha256::new(),
                    written: 0,
                };
                decompress_to(
                    &mut er,
                    &hdr,
                    &mut sink,
                    |v| progress(entry_base + v, total, out_name),
                    cancel,
                )?;
                let (sha, n) = sink.finish()?;
                verify_content_id(&sha, out_name)?;
                summaries.push(SummaryEntry {
                    input_name: entry_names[i].clone(),
                    name: out_name.clone(),
                    output_size: n,
                    sha256: Some(sha),
                    decompressed: true,
                });
                done += n;
            } else {
                let entry_base = done;
                let mut counter = ProgressReader {
                    inner: er,
                    base: entry_base,
                    total,
                    name: out_name.clone(),
                    progress,
                    cancel,
                };
                let n = match writer.write_next(&mut counter) {
                    Ok(n) => n,
                    // The progress reader raises `Interrupted` when the
                    // cancel flag flips mid-copy.
                    Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::Interrupted => {
                        return Err(Error::Cancelled);
                    }
                    Err(e) => return Err(e),
                };
                if cancel.load(Ordering::Relaxed) {
                    return Err(Error::Cancelled);
                }
                summaries.push(SummaryEntry {
                    input_name: entry_names[i].clone(),
                    name: out_name.clone(),
                    output_size: n,
                    sha256: None,
                    decompressed: false,
                });
                done += n;
            }
        }
        writer.finish()?;
        writer.flush_data()?;
        Ok(())
    })();

    if let Err(e) = result {
        // Delete the partial output; never leave a corrupt file behind.
        let _ = fs::remove_file(&part_path);
        return Err(e);
    }
    drop(writer);
    buf.flush().map_err(Error::Io)?;
    drop(buf);
    rename_over(&part_path, output)?;

    Ok(Summary {
        entries: summaries,
        total_output_size: done,
        elapsed: started.elapsed(),
    })
}

/// "<output>.part" path for the atomic-write protocol.
fn part_path_for(output: &Path) -> PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    output.with_file_name(name)
}

/// Writer adapter that hashes + counts everything written into a PFS0 entry.
/// The declared entry size is enforced by the underlying [`Pfs0Writer`].
struct Pfs0EntrySink<'a, 'w, W: Write> {
    inner: &'w mut Pfs0Writer<'a, W>,
    hasher: Sha256,
    written: u64,
}

impl<W: Write> Pfs0EntrySink<'_, '_, W> {
    /// Consumes the sink, returning the entry hash and the number of bytes
    /// written.
    fn finish(self) -> Result<([u8; 32], u64)> {
        Ok((self.hasher.finalize().into(), self.written))
    }
}

impl<W: Write> Write for Pfs0EntrySink<'_, '_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner
            .write_data(buf)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.hasher.update(buf);
        self.written += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner
            .flush_data()
            .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

/// Reader adapter that reports per-chunk progress and honours cancellation
/// while an entry is copied verbatim.
struct ProgressReader<'a, 'p, R: Read> {
    inner: R,
    base: u64,
    total: u64,
    name: String,
    progress: &'a mut dyn FnMut(u64, u64, &str),
    cancel: &'p AtomicBool,
}

impl<R: Read> Read for ProgressReader<'_, '_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            ));
        }
        let n = self.inner.read(buf)?;
        self.base += n as u64;
        (self.progress)(self.base, self.total, &self.name);
        Ok(n)
    }
}

#[cfg(test)]
#[path = "ncz/test_helpers.rs"]
mod test_helpers;

#[cfg(test)]
#[path = "ncz/tests.rs"]
mod tests;
