//! Language of the launcher's own messages (errors, preparation steps, room notices) and of the in-game overlay.
//!
//! English is the source text: messages are written in English with [`tr!`](crate::tr) and looked up in the Italian table
//! (`i18n_table.rs`) when the interface is Italian. The test below fails when a message has no Italian translation.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::i18n_table::IT;

static ITALIAN: AtomicBool = AtomicBool::new(false);

/// The launcher's message `$en` in the interface language. With arguments, `{name}` placeholders are filled from them and
/// a `String` comes back; without, a `&'static str`.
#[macro_export]
macro_rules! tr {
    ($en:literal) => {
        $crate::i18n::tr($en)
    };
    ($en:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut text = $crate::i18n::tr($en).to_string();
        $( text = text.replace(concat!("{", stringify!($name), "}"), &($value).to_string()); )+
        text
    }};
}

pub fn tr(en: &'static str) -> &'static str {
    if ITALIAN.load(Ordering::Relaxed) {
        IT.iter().find(|(e, _)| *e == en).map(|(_, it)| *it).unwrap_or(en)
    } else {
        en
    }
}

/// "en" or "it": the language of the launcher's messages, passed on to the engines' own menus.
pub fn code() -> &'static str {
    if ITALIAN.load(Ordering::Relaxed) { "it" } else { "en" }
}

/// What the operating system says about the player (for the "auto" interface language).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SystemLocale {
    /// ISO 639-1, lower case ("it").
    pub language: String,
    /// ISO 3166-1 alpha-2, upper case ("IT"); empty when unknown.
    pub country: String,
}

/// The desktop's display language and the country the player set (Windows: display language and "Country or region";
/// elsewhere: the locale environment variables).
pub fn system_locale() -> SystemLocale {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::{GetUserDefaultGeoName, GetUserDefaultUILanguage, LCIDToLocaleName};
        let mut language = String::new();
        let lang_id = unsafe { GetUserDefaultUILanguage() } as u32;
        let mut buf = [0u16; 85];
        let n = unsafe { LCIDToLocaleName(lang_id, Some(&mut buf), 0) };
        if n > 1 {
            let name = String::from_utf16_lossy(&buf[..(n as usize - 1)]); // "it-IT"
            language = name.split(['-', '_']).next().unwrap_or_default().to_lowercase();
        }
        let mut geo = [0u16; 16];
        let n = unsafe { GetUserDefaultGeoName(&mut geo) };
        let country = if n > 1 { String::from_utf16_lossy(&geo[..(n as usize - 1)]).to_uppercase() } else { String::new() };
        // GetUserDefaultGeoName answers with a numeric UN M.49 code when the region has no ISO name: not a country
        let country = if country.chars().all(|c| c.is_ascii_alphabetic()) { country } else { String::new() };
        SystemLocale { language, country }
    }
    #[cfg(not(windows))]
    {
        let value = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty() && s != "C" && s != "POSIX"))
            .unwrap_or_default(); // "it_IT.UTF-8"
        let base = value.split(['.', '@']).next().unwrap_or_default();
        let mut parts = base.split(['_', '-']);
        let language = parts.next().unwrap_or_default().to_lowercase();
        let country = parts.next().unwrap_or_default().to_uppercase();
        SystemLocale { language, country }
    }
}

/// Italian for "it", or for "auto" when the player is in Italy (San Marino, Vatican City) or uses an Italian desktop.
pub fn wants_italian(setting: &str, sys: &SystemLocale) -> bool {
    match setting {
        "it" => true,
        "en" => false,
        _ => sys.language == "it" || matches!(sys.country.as_str(), "IT" | "SM" | "VA"),
    }
}

pub fn set_italian(on: bool) {
    ITALIAN.store(on, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every literal passed to tr!(...) in the sources has an Italian translation, and no key appears twice.
    #[test]
    fn every_message_has_italian() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("i18n.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let mut rest = text.as_str();
            while let Some(at) = rest.find("tr!(") {
                rest = &rest[at + 4..];
                let trimmed = rest.trim_start();
                let Some(lit) = trimmed.strip_prefix('"') else { continue };
                let mut out = String::new();
                let mut chars = lit.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => out.push(chars.next().unwrap_or('\\')),
                        '"' => break,
                        c => out.push(c),
                    }
                }
                if !IT.iter().any(|(e, _)| *e == out) {
                    missing.push(format!("{}: {out}", path.file_name().unwrap().to_string_lossy()));
                }
            }
        }
        assert!(missing.is_empty(), "messages without Italian:\n{}", missing.join("\n"));
        let mut keys: Vec<&str> = IT.iter().map(|(e, _)| *e).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), IT.len(), "a message is translated twice");
    }

    #[test]
    fn auto_language() {
        let sys = |language: &str, country: &str| SystemLocale { language: language.into(), country: country.into() };
        assert!(wants_italian("auto", &sys("it", "CH")));
        assert!(wants_italian("auto", &sys("en", "IT")));
        assert!(!wants_italian("auto", &sys("en", "GB")));
        assert!(!wants_italian("en", &sys("it", "IT")));
        assert!(wants_italian("it", &sys("de", "DE")));
    }
}
