// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! `d8_nx` — Nintendo Switch package handling for the Deluxe 8 launcher.
//!
//! The crate understands the container formats used for Switch home-screen
//! installables:
//!
//! * [`pfs0`] — the PFS0 partition filesystem used by NSP and NSZ containers
//!   (read + streaming write).
//! * [`cnmt`] — the plaintext `<id>.cnmt.xml` content-metadata manifest found
//!   inside NSP/NSZ packages.
//! * [`ncz`] — the NCZ recompression wrapper used by NSZ files. Provides
//!   streaming decompression back to the original NCA (NCZBLOCK block streams
//!   and solid zstd streams are both supported) plus whole-container
//!   `nsz` → `nsp` conversion with per-NCA SHA-256 verification.
//! * [`package`] — high level inspection of `.nsp` / `.nsz` files.
//! * [`keys`] — parsing of `prod.keys` / `title.keys` (key values are never
//!   printed, logged, or exposed through `Debug`/`Display`).
//! * [`firmware`] — inspection of firmware dumps (folders and zip archives).
//!
//! All parsing functions return [`Error`] on malformed input; nothing in this
//! crate panics on bad data.
//!
//! # Format references
//!
//! The NCZ layout implemented here follows the reference implementations that
//! define the format in practice:
//!
//! * Eden Emulator's NCZ decompressor (`ncz_decompressor.cpp`), GPL-3.0-or-later.
//! * The nsz project (`nicoboss/nsz`), MIT licensed, which documents and
//!   produces the files in the wild.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cnmt;
pub mod error;
pub mod firmware;
pub mod keys;
pub mod ncz;
pub mod package;
pub mod pfs0;

pub use error::{Error, Result};
