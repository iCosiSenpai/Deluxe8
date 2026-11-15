// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! Crate-wide error type.

use std::io;

/// All errors produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A plain I/O error from the underlying file system.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),

    /// A container/file was structurally invalid (bad magic, out-of-range offsets, ...).
    #[error("{0}")]
    Format(String),

    /// A line in a `prod.keys` / `title.keys` file could not be parsed.
    #[error("key file parse error at line {line}: {msg}")]
    KeyParse {
        /// 1-based line number of the offending line.
        line: usize,
        /// Human-readable description of the problem.
        msg: String,
    },

    /// A CNMT XML document could not be parsed.
    #[error("cnmt xml parse error: {0}")]
    Cnmt(String),

    /// A `zip` archive error (firmware inspection).
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// The conversion was cancelled by the caller.
    #[error("conversion cancelled")]
    Cancelled,

    /// The SHA-256 of a produced NCA does not match the content id encoded
    /// in its file name; the output is considered corrupt.
    #[error("nca hash verification failed for {name}: expected {expected}..., got {actual}...")]
    HashMismatch {
        /// Output file name whose content id was checked.
        name: String,
        /// Content id from the file name (first 16 bytes of the hash, hex).
        expected: String,
        /// Actual first 16 bytes of the produced NCA's SHA-256, hex.
        actual: String,
    },
}

impl Error {
    /// Convenience constructor for format errors.
    pub(crate) fn format(msg: impl Into<String>) -> Self {
        Error::Format(msg.into())
    }
}

/// Crate result type.
pub type Result<T> = std::result::Result<T, Error>;
