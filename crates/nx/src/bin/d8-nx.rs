// SPDX-FileCopyrightText: Copyright 2026 Deluxe 8 Emulator Project
// SPDX-License-Identifier: GPL-3.0-or-later

//! `d8-nx` — command line interface for the `d8_nx` Switch package library.
//!
//! ```text
//! d8-nx inspect [--json] <file>...
//! d8-nx nsz2nsp <in.nsz> <out.nsp>
//! d8-nx keys   [--json] <prod.keys> [title.keys]
//! d8-nx firmware [--json] <dir|zip>
//! ```
//!
//! The `keys` command prints counts and validation results only — never key
//! values.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};

use d8_nx::{cnmt, firmware, keys, ncz, package};

// ---------------------------------------------------------------------------
// tiny JSON emitter (hand-rolled to avoid a serde dependency)
// ---------------------------------------------------------------------------

struct Json {
    out: String,
}

impl Json {
    fn new() -> Self {
        Self { out: String::new() }
    }

    fn key(&mut self, key: &str) {
        if !self.out.is_empty() && !self.out.ends_with('{') {
            self.out.push(',');
        }
        self.out.push_str(&format!("{:?}:", key));
    }

    fn str(&mut self, v: &str) {
        self.out.push_str(&format!("{:?}", v));
    }

    fn opt_str(&mut self, v: Option<&str>) {
        match v {
            Some(s) => self.str(s),
            None => self.out.push_str("null"),
        }
    }

    fn num(&mut self, v: u64) {
        self.out.push_str(&v.to_string());
    }

    fn opt_num(&mut self, v: Option<u64>) {
        match v {
            Some(n) => self.num(n),
            None => self.out.push_str("null"),
        }
    }

    fn bool(&mut self, v: bool) {
        self.out.push_str(if v { "true" } else { "false" });
    }

    fn obj_open(&mut self) {
        self.out.push('{');
    }

    fn obj_close(&mut self) {
        self.out.push('}');
    }

    fn arr_open(&mut self) {
        self.out.push('[');
    }

    fn arr_close(&mut self) {
        self.out.push(']');
    }

    fn comma(&mut self) {
        self.out.push(',');
    }
}

fn fmt_bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut v = n as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{v:.2} {}", UNITS[unit])
    }
}

fn print_json_document(j: Json) {
    println!("{}", j.out);
}

// ---------------------------------------------------------------------------
// inspect
// ---------------------------------------------------------------------------

fn cmd_inspect(paths: &[PathBuf]) -> Result<()> {
    let mut doc = Json::new();
    doc.arr_open();
    let mut first = true;
    for path in paths {
        let info = package::inspect(path).with_context(|| format!("{}", path.display()))?;
        if !first {
            doc.comma();
        }
        first = false;
        doc.obj_open();
        doc.key("path");
        doc.str(&info.path.display().to_string());
        doc.key("container");
        doc.str(match info.container {
            package::Container::Nsp => "nsp",
            package::Container::Nsz => "nsz",
        });
        doc.key("title_id");
        doc.opt_num(info.title_id);
        doc.key("application_title_id");
        doc.opt_num(info.application_title_id);
        doc.key("kind");
        doc.str(info.kind.as_str());
        doc.key("type");
        match &info.cnmt_type {
            Some(t) => doc.str(t),
            None => doc.out.push_str("null"),
        }
        doc.key("version");
        doc.num(info.version as u64);
        doc.key("display_version");
        doc.opt_str(info.display_version.as_deref());
        doc.key("bitness");
        doc.opt_str(info.bitness_note());
        doc.key("required_system_version");
        doc.opt_num(info.required_system_version.map(|v| v as u64));
        doc.key("required_download_system_version");
        doc.opt_num(info.required_download_system_version.map(|v| v as u64));
        doc.key("has_ticket");
        doc.bool(info.has_ticket);
        doc.key("ncz");
        doc.arr_open();
        for (i, n) in info.ncz.iter().enumerate() {
            if i > 0 {
                doc.comma();
            }
            doc.obj_open();
            doc.key("name");
            doc.str(&n.name);
            doc.key("solid");
            doc.bool(n.solid);
            doc.key("size");
            doc.num(n.size);
            doc.obj_close();
        }
        doc.arr_close();
        doc.key("entries");
        doc.arr_open();
        for (i, e) in info.entries.iter().enumerate() {
            if i > 0 {
                doc.comma();
            }
            doc.obj_open();
            doc.key("name");
            doc.str(&e.name);
            doc.key("size");
            doc.num(e.size);
            doc.obj_close();
        }
        doc.arr_close();
        doc.obj_close();
    }
    doc.arr_close();
    print_json_document(doc);
    Ok(())
}

fn cmd_inspect_text(paths: &[PathBuf]) -> Result<()> {
    for (idx, path) in paths.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        let info = package::inspect(path).with_context(|| format!("{}", path.display()))?;
        println!("{}", info.path.display());
        println!(
            "  Container:  {}",
            match info.container {
                package::Container::Nsp => "NSP",
                package::Container::Nsz => "NSZ",
            }
        );
        println!(
            "  Title ID:   {}",
            info.title_id
                .map(|id| format!("{id:016X}"))
                .unwrap_or_else(|| "unknown".into())
        );
        println!(
            "  App ID:     {}",
            info.application_title_id
                .map(|id| format!("{id:016X}"))
                .unwrap_or_else(|| "unknown".into())
        );
        let kind = info.kind.as_str();
        let ty = info
            .cnmt_type
            .as_deref()
            .map(|t| {
                if info.entries.iter().any(|e| e.name.ends_with(".cnmt.xml")) {
                    t.to_owned()
                } else {
                    format!("{t} (inferred from title id)")
                }
            })
            .unwrap_or_else(|| "unknown".into());
        println!("  Kind:       {kind:<8} Type: {ty}");
        println!(
            "  Version:    {}{}",
            info.version,
            info.display_version
                .as_deref()
                .map(|v| format!(" ({v})"))
                .unwrap_or_default()
        );
        if let Some(note) = info.bitness_note() {
            println!("  Bitness:    {note}");
        }
        if let Some(v) = info.required_system_version {
            println!("  Req. sys:   {v} ({})", cnmt::format_version(v));
        }
        if let Some(v) = info.required_download_system_version {
            println!("  Req. dl:    {v} ({})", cnmt::format_version(v));
        }
        println!(
            "  Ticket:     {}",
            if info.has_ticket { "yes" } else { "no" }
        );
        if !info.ncz.is_empty() {
            println!("  NCZ entries ({}):", info.ncz.len());
            for n in &info.ncz {
                println!(
                    "    {}  {}  {}",
                    n.name,
                    if n.solid { "solid" } else { "block" },
                    fmt_bytes(n.size)
                );
            }
        }
        println!("  Entries ({}):", info.entries.len());
        for e in &info.entries {
            println!("    {}  {}", e.name, fmt_bytes(e.size));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// nsz2nsp
// ---------------------------------------------------------------------------

fn cmd_nsz2nsp(input: &Path, output: &Path) -> Result<()> {
    let cancel = Arc::new(AtomicBool::new(false));
    // Ctrl+C sets the cancel flag; the converter then cleans up its partial
    // output and returns an error.
    {
        let cancel = Arc::clone(&cancel);
        ctrlc::set_handler(move || {
            cancel.store(true, Ordering::Relaxed);
        })
        .context("failed to install Ctrl+C handler")?;
    }

    let term = std::io::stdout().is_terminal();
    let started = Instant::now();
    let mut last_line: Option<String> = None;
    let mut progress = |done: u64, total: u64, entry: &str| {
        if !term {
            return;
        }
        let pct = if total > 0 {
            done as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        let rate = if started.elapsed().as_secs_f64() > 0.0 {
            done as f64 / started.elapsed().as_secs_f64()
        } else {
            0.0
        };
        let eta = if rate > 0.0 {
            (total - done) as f64 / rate
        } else {
            0.0
        };
        let line = format!(
            "{entry:<44} {pct:5.1}%  {} / {}  [{}/s, ETA {:>4.0}s]",
            fmt_bytes(done),
            fmt_bytes(total),
            fmt_bytes(rate as u64),
            eta
        );
        if last_line.as_deref() != Some(line.as_str()) {
            let mut out = std::io::stdout().lock();
            let _ = write!(out, "\r\x1b[2K{line}");
            let _ = out.flush();
            last_line = Some(line);
        }
    };

    let summary = ncz::nsz_to_nsp(input, output, &mut progress, &cancel)?;
    if term {
        println!();
    }

    let secs = summary.elapsed.as_secs_f64();
    let rate = if secs > 0.0 {
        summary.total_output_size as f64 / secs
    } else {
        0.0
    };
    println!("Converted {} -> {}", input.display(), output.display());
    println!(
        "  wrote {} in {:.1}s ({}/s)",
        fmt_bytes(summary.total_output_size),
        secs,
        fmt_bytes(rate as u64)
    );
    for e in &summary.entries {
        let verified = if e.decompressed {
            match &e.sha256 {
                Some(h) => format!("sha256[..16]={} VERIFIED", hex::encode(&h[..16])),
                None => "verified".into(),
            }
        } else {
            "copied".into()
        };
        println!(
            "  {} -> {}  {}  {verified}",
            e.input_name,
            e.name,
            fmt_bytes(e.output_size)
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// keys
// ---------------------------------------------------------------------------

fn cmd_keys(prod: &Path, title: Option<&Path>, json: bool) -> Result<()> {
    let prod_file =
        std::fs::File::open(prod).with_context(|| format!("cannot open {}", prod.display()))?;
    let set = keys::KeySet::parse(std::io::BufReader::new(prod_file))
        .with_context(|| format!("parsing {}", prod.display()))?;
    let report = keys::validate(&set);

    let title_count = match title {
        Some(p) => {
            let f =
                std::fs::File::open(p).with_context(|| format!("cannot open {}", p.display()))?;
            let set = keys::TitleKeys::parse(std::io::BufReader::new(f))
                .with_context(|| format!("parsing {}", p.display()))?;
            Some(set.count())
        }
        None => None,
    };

    if json {
        let mut doc = Json::new();
        doc.obj_open();
        doc.key("prod_keys");
        doc.str(&prod.display().to_string());
        doc.key("count");
        doc.num(report.count as u64);
        doc.key("has_header_key");
        doc.bool(report.has_header_key);
        doc.key("max_master_key_revision");
        doc.num(report.max_master_key_revision as u64);
        doc.key("missing_required");
        doc.arr_open();
        for (i, m) in report.missing_required.iter().enumerate() {
            if i > 0 {
                doc.comma();
            }
            doc.str(m);
        }
        doc.arr_close();
        doc.key("title_keys");
        match title_count {
            Some(n) => doc.num(n as u64),
            None => doc.out.push_str("null"),
        }
        doc.obj_close();
        print_json_document(doc);
    } else {
        println!("prod.keys: {}", prod.display());
        println!("  keys:                 {}", report.count);
        println!(
            "  header_key:           {}",
            if report.has_header_key {
                "present"
            } else {
                "missing"
            }
        );
        println!("  max master key rev:  {}", report.max_master_key_revision);
        if report.missing_required.is_empty() {
            println!("  required keys:        all present");
        } else {
            println!("  missing required keys:");
            for m in &report.missing_required {
                println!("    - {m}");
            }
        }
        if let Some(p) = title {
            println!("title.keys: {}", p.display());
            println!("  title keys:           {}", title_count.unwrap_or(0));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// firmware
// ---------------------------------------------------------------------------

fn cmd_firmware(path: &Path, json: bool) -> Result<()> {
    let report = firmware::inspect(path).with_context(|| format!("{}", path.display()))?;
    if json {
        let mut doc = Json::new();
        doc.obj_open();
        doc.key("path");
        doc.str(&path.display().to_string());
        doc.key("nca_count");
        doc.num(report.nca_count as u64);
        doc.key("cnmt_nca_count");
        doc.num(report.cnmt_nca_count as u64);
        doc.key("total_size");
        doc.num(report.total_size);
        doc.key("version_hint");
        doc.opt_str(report.version_hint.as_deref());
        doc.key("looks_valid");
        doc.bool(report.looks_valid);
        doc.obj_close();
        print_json_document(doc);
    } else {
        println!("firmware: {}", path.display());
        println!("  NCA files:     {}", report.nca_count);
        println!("  CNMT NCAs:     {}", report.cnmt_nca_count);
        println!("  total size:    {}", fmt_bytes(report.total_size));
        println!(
            "  version hint:  {}",
            report.version_hint.as_deref().unwrap_or("none")
        );
        println!(
            "  looks valid:   {}",
            if report.looks_valid { "yes" } else { "no" }
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// entry point
// ---------------------------------------------------------------------------

fn usage() -> &'static str {
    "\
d8-nx — Nintendo Switch package handling for the Deluxe 8 launcher

Usage:
  d8-nx inspect   [--json] <file.nsp|file.nsz>...
  d8-nx nsz2nsp   <in.nsz> <out.nsp>
  d8-nx keys      [--json] <prod.keys> [title.keys]
  d8-nx firmware  [--json] <dir|zip>

The `keys` command prints counts and validation results only — never key values."
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = run(&args);
    match result {
        Ok(()) => {}
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let Some(cmd) = args.first() else {
        println!("{}", usage());
        return Ok(());
    };
    let rest = &args[1..];
    match cmd.as_str() {
        "inspect" => {
            let (json, files) = split_json_flag(rest);
            if files.is_empty() {
                bail!("inspect: at least one file is required");
            }
            let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
            if json {
                cmd_inspect(&paths)
            } else {
                cmd_inspect_text(&paths)
            }
        }
        "nsz2nsp" => {
            if rest.len() != 2 {
                bail!("nsz2nsp: expected <in.nsz> <out.nsp>");
            }
            cmd_nsz2nsp(Path::new(&rest[0]), Path::new(&rest[1]))
        }
        "keys" => {
            let (json, files) = split_json_flag(rest);
            if files.is_empty() || files.len() > 2 {
                bail!("keys: expected <prod.keys> [title.keys]");
            }
            let title = files.get(1).map(Path::new);
            cmd_keys(Path::new(&files[0]), title, json)
        }
        "firmware" => {
            let (json, files) = split_json_flag(rest);
            if files.len() != 1 {
                bail!("firmware: expected <dir|zip>");
            }
            cmd_firmware(Path::new(&files[0]), json)
        }
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        other => Err(anyhow!("unknown command '{other}' (see `d8-nx help`)")),
    }
}

/// Splits a leading `--json` flag from the rest of the arguments.
fn split_json_flag(args: &[String]) -> (bool, &[String]) {
    if args.first().is_some_and(|a| a == "--json") {
        (true, &args[1..])
    } else {
        (false, args)
    }
}
