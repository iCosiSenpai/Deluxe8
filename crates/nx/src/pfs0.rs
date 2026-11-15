// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! PFS0 partition filesystem reader and writer.
//!
//! NSP and NSZ containers are PFS0 archives. The on-disk layout is:
//!
//! ```text
//! 0x00  magic "PFS0"                  (4 bytes)
//! 0x04  file count                    (u32 LE)
//! 0x08  string table size             (u32 LE, includes padding)
//! 0x0C  reserved                      (4 bytes, zero)
//! 0x10  file table: count x 0x18      (u64 data_offset, u64 size,
//!                                      u32 name_offset, u32 reserved)
//!      string table                   (NUL-terminated names)
//!      file data                      (entries may start at any offset)
//! ```
//!
//! Note: the `data_offset` field stored on disk is relative to the *end of
//! the string table* (this is how Nintendo-authored files, nsz and Eden all
//! read/write it). The [`Entry::data_offset`] exposed by this module is the
//! **absolute** offset in the file instead, which is what callers actually
//! want.

use std::io::{Read, Seek, SeekFrom, Write};

use crate::error::{Error, Result};

/// PFS0 magic bytes.
pub const MAGIC: [u8; 4] = *b"PFS0";

/// Size of the fixed PFS0 header.
pub const HEADER_SIZE: u64 = 0x10;
/// Size of one file-table entry.
pub const FILE_ENTRY_SIZE: u64 = 0x18;
/// Maximum number of entries we accept (defensive bound; the table is 0x18
/// bytes per entry so this caps the metadata at 1.5 MiB).
pub const MAX_FILE_COUNT: u64 = 0x10000;

/// A single PFS0 entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// File name (from the string table, NUL-stripped).
    pub name: String,
    /// **Absolute** offset of the entry data in the file.
    pub data_offset: u64,
    /// Size of the entry data in bytes.
    pub size: u64,
}

/// A parsed PFS0 container over an open file.
pub struct Pfs0<R> {
    file: R,
    entries: Vec<Entry>,
    /// Absolute offset where the data area starts (end of the string table).
    content_offset: u64,
}

impl<R: Read + Seek> Pfs0<R> {
    /// Parses the PFS0 header of `file` (positioned anywhere; the header is
    /// read from the start) and validates every entry against the file size.
    pub fn read(mut file: R) -> Result<Self> {
        let file_size = file.seek(SeekFrom::End(0)).map_err(Error::Io)?;
        file.seek(SeekFrom::Start(0)).map_err(Error::Io)?;

        if file_size < HEADER_SIZE {
            return Err(Error::format("file too small to contain a PFS0 header"));
        }
        let mut hdr = [0u8; HEADER_SIZE as usize];
        file.read_exact(&mut hdr).map_err(Error::Io)?;
        if hdr[0..4] != MAGIC {
            return Err(Error::format("not a PFS0 container (bad magic)"));
        }
        let count = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as u64;
        let strtab_size = u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as u64;
        if count > MAX_FILE_COUNT {
            return Err(Error::format(format!(
                "PFS0 file count {count} exceeds the supported maximum"
            )));
        }

        let table_size = count * FILE_ENTRY_SIZE;
        let content_offset = HEADER_SIZE + table_size + strtab_size;
        let metadata_size = content_offset;
        if metadata_size > file_size {
            return Err(Error::format("PFS0 header exceeds the file size"));
        }

        let mut entries = Vec::with_capacity(count as usize);
        let mut name_offsets = Vec::with_capacity(count as usize);
        for i in 0..count {
            let mut raw = [0u8; FILE_ENTRY_SIZE as usize];
            file.read_exact(&mut raw).map_err(Error::Io)?;
            let stored_offset = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let size = u64::from_le_bytes(raw[8..16].try_into().unwrap());
            let name_offset = u32::from_le_bytes(raw[16..20].try_into().unwrap()) as u64;
            if name_offset >= strtab_size {
                return Err(Error::format(format!(
                    "PFS0 entry {i}: name offset 0x{name_offset:x} is outside the string table"
                )));
            }
            let data_offset = content_offset
                .checked_add(stored_offset)
                .ok_or_else(|| Error::format(format!("PFS0 entry {i}: data offset overflows")))?;
            if data_offset
                .checked_add(size)
                .is_none_or(|end| end > file_size)
            {
                return Err(Error::format(format!(
                    "PFS0 entry {i}: data range exceeds the file size"
                )));
            }
            entries.push(Entry {
                name: String::new(),
                data_offset,
                size,
            });
            name_offsets.push(name_offset);
        }

        // String table: read and resolve names.
        let mut strtab = vec![0u8; strtab_size as usize];
        file.read_exact(&mut strtab).map_err(Error::Io)?;
        for (i, &name_offset) in name_offsets.iter().enumerate() {
            let end = strtab[name_offset as usize..]
                .iter()
                .position(|&b| b == 0)
                .map(|p| name_offset as usize + p)
                .ok_or_else(|| {
                    Error::format(format!(
                        "PFS0 entry {i}: name is not NUL-terminated within the string table"
                    ))
                })?;
            entries[i].name =
                String::from_utf8_lossy(&strtab[name_offset as usize..end]).into_owned();
        }
        Ok(Self {
            file,
            entries,
            content_offset,
        })
    }

    /// The parsed entries, in file-table order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Absolute offset where the data area starts (end of the string table).
    /// The on-disk `data_offset` fields are relative to this.
    pub fn content_offset(&self) -> u64 {
        self.content_offset
    }

    /// Finds the index of the first entry whose name ends with `suffix`
    /// (case-insensitive). Used to locate `.cnmt.xml`, `.tik`, `.ncz`, ...
    /// entries.
    pub fn find_suffix_index(&self, suffix: &str) -> Option<usize> {
        self.entries.iter().position(|e| {
            e.name
                .to_ascii_lowercase()
                .ends_with(&suffix.to_ascii_lowercase())
        })
    }

    /// Finds the first entry whose name ends with `suffix` (case-insensitive).
    pub fn find_suffix(&self, suffix: &str) -> Option<&Entry> {
        self.find_suffix_index(suffix).map(|i| &self.entries[i])
    }

    /// Returns a [`Read`] view over the exact byte range of entry `index`.
    /// The returned reader stops at the end of the entry, so it can never
    /// bleed into the next one.
    pub fn entry_reader(&mut self, index: usize) -> Result<EntryReader<'_, R>> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| Error::format(format!("PFS0 entry index {index} out of range")))?;
        self.file
            .seek(SeekFrom::Start(entry.data_offset))
            .map_err(Error::Io)?;
        Ok(EntryReader {
            file: &mut self.file,
            data_offset: entry.data_offset,
            size: entry.size,
            pos: 0,
        })
    }

    /// Consumes the container, returning the underlying file.
    pub fn into_inner(self) -> R {
        self.file
    }
}

/// A [`Read`] + [`Seek`] view over exactly one PFS0 entry's data. Seeks are
/// clamped to the entry range, so it can never bleed into the next entry.
pub struct EntryReader<'a, R> {
    file: &'a mut R,
    /// Absolute offset of the entry data in the file.
    data_offset: u64,
    /// Size of the entry in bytes.
    size: u64,
    /// Current position within the entry.
    pos: u64,
}

impl<R: Read> Read for EntryReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.size {
            return Ok(0);
        }
        let want = (buf.len() as u64).min(self.size - self.pos) as usize;
        let n = self.file.read(&mut buf[..want])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> Seek for EntryReader<'_, R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let target: i128 = match pos {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
            SeekFrom::End(d) => self.size as i128 + d as i128,
        };
        let clamped = target.clamp(0, self.size as i128) as u64;
        self.file
            .seek(SeekFrom::Start(self.data_offset + clamped))?;
        self.pos = clamped;
        Ok(clamped)
    }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// Specification of one entry for [`Pfs0Writer`]. The size must be known
/// before the data is streamed (the header is written first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySpec {
    /// File name stored in the string table.
    pub name: String,
    /// Exact size of the entry data in bytes.
    pub size: u64,
}

/// Builds the PFS0 header bytes for `entries` (string table padded so the
/// data area starts at a 0x10-aligned offset). Returns the header and the
/// absolute data-area start offset.
pub fn build_header(entries: &[EntrySpec]) -> Result<(Vec<u8>, u64)> {
    if entries.is_empty() {
        return Err(Error::format("PFS0 writer: at least one entry is required"));
    }
    if entries.len() as u64 > MAX_FILE_COUNT {
        return Err(Error::format("PFS0 writer: too many entries"));
    }
    let mut names = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        if e.name.is_empty() || e.name.as_bytes().contains(&0) {
            return Err(Error::format(format!(
                "PFS0 writer: entry {i} has an invalid name"
            )));
        }
        names.push(e.name.as_bytes());
    }
    let raw_strtab_size: u64 = names.iter().map(|n| n.len() as u64 + 1).sum();
    // Pad the string table so that the data start is 0x10-aligned.
    let align = 0x10;
    let unpadded = HEADER_SIZE + entries.len() as u64 * FILE_ENTRY_SIZE + raw_strtab_size;
    let strtab_size = raw_strtab_size + (align - unpadded % align) % align;
    let data_start = HEADER_SIZE + entries.len() as u64 * FILE_ENTRY_SIZE + strtab_size;

    let mut hdr = Vec::with_capacity(data_start as usize);
    hdr.extend_from_slice(&MAGIC);
    hdr.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    hdr.extend_from_slice(&(strtab_size as u32).to_le_bytes());
    hdr.extend_from_slice(&[0u8; 4]);

    let mut name_offset: u32 = 0;
    let mut data_offset: u64 = 0; // stored relative to the data start
    for (e, name) in entries.iter().zip(&names) {
        hdr.extend_from_slice(&data_offset.to_le_bytes());
        hdr.extend_from_slice(&e.size.to_le_bytes());
        hdr.extend_from_slice(&name_offset.to_le_bytes());
        hdr.extend_from_slice(&[0u8; 4]);
        name_offset += name.len() as u32 + 1;
        data_offset = data_offset
            .checked_add(e.size)
            .ok_or_else(|| Error::format("PFS0 writer: entry sizes overflow"))?;
    }
    for name in names {
        hdr.extend_from_slice(name);
        hdr.push(0);
    }
    hdr.resize(data_start as usize, 0);
    Ok((hdr, data_start))
}

/// Streaming PFS0 writer.
///
/// ```no_run
/// # use std::io::Cursor;
/// # use d8_nx::pfs0::{EntrySpec, Pfs0Writer};
/// # fn main() -> d8_nx::Result<()> {
/// let mut out = Vec::new();
/// let mut w = Pfs0Writer::new(&mut out, vec![EntrySpec { name: "a.bin".into(), size: 3 }])?;
/// w.write_next(&mut Cursor::new(vec![1u8, 2, 3]))?;
/// w.finish()?;
/// # Ok(())
/// # }
/// ```
pub struct Pfs0Writer<'a, W: Write> {
    inner: &'a mut W,
    entries: Vec<EntrySpec>,
    /// Absolute offset of the data area start (end of the padded string table).
    data_start: u64,
    /// Index of the entry currently being written.
    next: usize,
    /// Bytes already written for `entries[next]`.
    current_written: u64,
    finished: bool,
}

impl<'a, W: Write> Pfs0Writer<'a, W> {
    /// Writes the PFS0 header for `entries` and returns the writer.
    /// Entry data is written immediately after the header, in the given
    /// order.
    pub fn new(inner: &'a mut W, entries: Vec<EntrySpec>) -> Result<Self> {
        let (hdr, data_start) = build_header(&entries)?;
        inner.write_all(&hdr).map_err(Error::Io)?;
        Ok(Self {
            inner,
            entries,
            data_start,
            next: 0,
            current_written: 0,
            finished: false,
        })
    }

    /// Absolute offset (in the output) where the data area starts.
    pub fn data_start(&self) -> u64 {
        self.data_start
    }

    /// Writes raw bytes for the current entry. Errors if the entry's
    /// declared size would be exceeded; automatically advances to the next
    /// entry when the current one is complete.
    pub fn write_data(&mut self, buf: &[u8]) -> Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        let spec = self
            .entries
            .get(self.next)
            .ok_or_else(|| Error::format("PFS0 writer: all entries already written"))?;
        let remaining = spec.size - self.current_written;
        if buf.len() as u64 > remaining {
            return Err(Error::format(format!(
                "PFS0 writer: entry '{}' data exceeds the declared size ({} bytes)",
                spec.name, spec.size
            )));
        }
        self.inner.write_all(buf).map_err(Error::Io)?;
        self.current_written += buf.len() as u64;
        if self.current_written == spec.size {
            self.next += 1;
            self.current_written = 0;
        }
        Ok(())
    }

    /// Streams the next entry's data from `src` into the container.
    ///
    /// Exactly [`EntrySpec::size`] bytes must be provided; an error is
    /// returned if `src` ends early. Returns the number of bytes written.
    pub fn write_next(&mut self, src: &mut impl Read) -> Result<u64> {
        let (name, want) = match self.entries.get(self.next) {
            Some(s) => (s.name.clone(), s.size),
            None => return Err(Error::format("PFS0 writer: all entries already written")),
        };
        let mut copied = 0u64;
        // Heap buffer: the Windows main-thread stack is only 1 MiB, so this
        // must never be a fixed-size stack array.
        let mut buf = vec![0u8; 1 << 20];
        while copied < want {
            let take = ((want - copied) as usize).min(buf.len());
            let n = src.read(&mut buf[..take]).map_err(Error::Io)?;
            if n == 0 {
                return Err(Error::format(format!(
                    "PFS0 writer: entry '{name}' ended early ({copied} of {want} bytes)"
                )));
            }
            self.write_data(&buf[..n])?;
            copied += n as u64;
        }
        Ok(copied)
    }

    /// Checks that every declared entry was written. Call this once after the
    /// last [`write_next`](Self::write_next) / [`write_data`](Self::write_data).
    pub fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        if self.next != self.entries.len() {
            return Err(Error::format(format!(
                "PFS0 writer: {} of {} entries were written",
                self.next,
                self.entries.len()
            )));
        }
        self.finished = true;
        Ok(())
    }

    /// Flushes the underlying writer.
    pub fn flush_data(&mut self) -> Result<()> {
        self.inner.flush().map_err(Error::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn sample_entries() -> Vec<EntrySpec> {
        vec![
            EntrySpec {
                name: "a.bin".into(),
                size: 3,
            },
            EntrySpec {
                name: "longer-name.dat".into(),
                size: 0x11,
            },
            EntrySpec {
                name: "z".into(),
                size: 100,
            },
        ]
    }

    #[test]
    fn round_trip() {
        let mut out = Vec::new();
        {
            let mut w = Pfs0Writer::new(&mut out, sample_entries()).unwrap();
            assert_eq!(w.data_start() % 0x10, 0);
            w.write_next(&mut Cursor::new(vec![1, 2, 3])).unwrap();
            w.write_next(&mut Cursor::new(vec![7u8; 0x11])).unwrap();
            w.write_next(&mut Cursor::new(vec![9u8; 100])).unwrap();
            w.finish().unwrap();
        }
        let mut p = Pfs0::read(Cursor::new(out)).unwrap();
        assert_eq!(p.entries().len(), 3);
        let e = &p.entries()[0];
        assert_eq!(e.name, "a.bin");
        assert_eq!(e.size, 3);
        assert_eq!(e.data_offset, p.content_offset());
        assert_eq!(p.entries()[1].name, "longer-name.dat");
        assert_eq!(p.entries()[1].data_offset, p.content_offset() + 3);
        assert_eq!(p.entries()[2].name, "z");
        // Entry data must be readable with exact sizes.
        let mut r = p.entry_reader(0).unwrap();
        let mut buf = vec![0u8; 10];
        assert_eq!(r.read(&mut buf).unwrap(), 3);
        assert_eq!(&buf[..3], &[1, 2, 3]);
        assert_eq!(
            r.read(&mut buf).unwrap(),
            0,
            "entry reader must stop at entry end"
        );
    }

    #[test]
    fn data_start_is_16_aligned() {
        // Names chosen so the raw string table is not 0x10 aligned.
        let mut out = Vec::new();
        let mut w = Pfs0Writer::new(
            &mut out,
            vec![EntrySpec {
                name: "abc".into(),
                size: 1,
            }],
        )
        .unwrap();
        assert_eq!(w.data_start() % 0x10, 0);
        w.write_next(&mut Cursor::new(vec![0])).unwrap();
        w.finish().unwrap();
        let p = Pfs0::read(Cursor::new(out)).unwrap();
        assert_eq!(p.content_offset(), p.entries()[0].data_offset);
    }

    #[test]
    fn bad_magic() {
        let err = Pfs0::read(Cursor::new(vec![0u8; 0x40]))
            .err()
            .expect("must fail");
        assert!(err.to_string().contains("not a PFS0"));
    }

    #[test]
    fn short_file() {
        let err = Pfs0::read(Cursor::new(*b"PFS0")).err().expect("must fail");
        assert!(err.to_string().contains("too small"));
    }

    #[test]
    fn entry_beyond_file_size() {
        let mut out = Vec::new();
        let mut w = Pfs0Writer::new(
            &mut out,
            vec![EntrySpec {
                name: "x".into(),
                size: 10,
            }],
        )
        .unwrap();
        w.write_next(&mut Cursor::new(vec![0u8; 10])).unwrap();
        w.finish().unwrap();
        // Truncate the file data: header remains, entry range exceeds size.
        out.truncate(out.len() - 5);
        let err = Pfs0::read(Cursor::new(out)).err().expect("must fail");
        assert!(err.to_string().contains("exceeds the file size"));
    }

    #[test]
    fn name_offset_out_of_range() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&MAGIC);
        raw.extend_from_slice(&1u32.to_le_bytes());
        raw.extend_from_slice(&4u32.to_le_bytes()); // tiny string table
        raw.extend_from_slice(&[0u8; 4]);
        raw.extend_from_slice(&0u64.to_le_bytes()); // data offset
        raw.extend_from_slice(&0u64.to_le_bytes()); // size
        raw.extend_from_slice(&0x30u32.to_le_bytes()); // name offset beyond table
        raw.extend_from_slice(&[0u8; 4]);
        raw.extend_from_slice(b"\0\0\0\0");
        let err = Pfs0::read(Cursor::new(raw)).err().expect("must fail");
        assert!(err.to_string().contains("outside the string table"));
    }

    #[test]
    fn writer_rejects_early_source() {
        let mut out = Vec::new();
        let mut w = Pfs0Writer::new(
            &mut out,
            vec![EntrySpec {
                name: "x".into(),
                size: 10,
            }],
        )
        .unwrap();
        let err = w
            .write_next(&mut Cursor::new(vec![0u8; 3]))
            .expect_err("must fail");
        assert!(err.to_string().contains("ended early"));
    }

    #[test]
    fn writer_rejects_unfinished() {
        let mut out = Vec::new();
        let mut w = Pfs0Writer::new(
            &mut out,
            vec![EntrySpec {
                name: "x".into(),
                size: 1,
            }],
        )
        .unwrap();
        let err = w.finish().expect_err("must fail");
        assert!(err.to_string().contains("were written"));
    }
}
