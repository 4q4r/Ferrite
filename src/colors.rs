//! Color palette resolution. `[colors]` holds literal hex values; with
//! `source = "pywal"` the unset ones are filled from `~/.cache/wal/colors.json`
//! (explicit TOML keys always win). Resolution happens once at startup, so the
//! bar pays zero cost at runtime and a `wal -i` + `swaymsg reload` picks up new
//! colors without any inotify watcher.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::config::{ColorSource, ColorsConfig};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

const STATIC_DEFAULT: &str = "#cdd6f4";
const STATIC_URGENT: &str = "#f38ba8";
const STATIC_WARN: &str = "#f9e2af";
const STATIC_GOOD: &str = "#a6e3a1";
const STATIC_MUTE: &str = "#fab387";

/// `~/.cache/wal/colors.json` (honoring `$XDG_CACHE_HOME`).
fn wal_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("wal").join("colors.json");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    PathBuf::from(home)
        .join(".cache")
        .join("wal")
        .join("colors.json")
}

/// Resolve the palette in place: fill every unset field from pywal (when
/// `source = "pywal"`) and then from the static defaults, so explicit TOML
/// values always win. Called once at startup, before `Ctx::new`.
pub fn resolve(c: &mut ColorsConfig) {
    if c.source == ColorSource::Pywal {
        match load_pywal() {
            Ok(p) => apply_pywal(c, &p),
            Err(e) => eprintln!("ferrite: colors: pywal load failed ({e}); using static palette"),
        }
    }
    fill_static(c);
}

/// Fill every `None` field with the matching pywal value; leave explicit
/// (`Some`) fields untouched so user overrides win.
fn apply_pywal(c: &mut ColorsConfig, p: &Pywal) {
    if c.default.is_none() {
        c.default = p.special.get("foreground").cloned();
    }
    if c.urgent.is_none() {
        c.urgent = p.colors.get("color1").cloned();
    }
    if c.warn.is_none() {
        c.warn = p.colors.get("color3").cloned();
    }
    if c.good.is_none() {
        c.good = p.colors.get("color2").cloned();
    }
    if c.mute.is_none() {
        c.mute = p.colors.get("color5").cloned();
    }
}

/// Fill every still-`None` field with the static default palette.
fn fill_static(c: &mut ColorsConfig) {
    if c.default.is_none() {
        c.default = Some(STATIC_DEFAULT.to_owned());
    }
    if c.urgent.is_none() {
        c.urgent = Some(STATIC_URGENT.to_owned());
    }
    if c.warn.is_none() {
        c.warn = Some(STATIC_WARN.to_owned());
    }
    if c.good.is_none() {
        c.good = Some(STATIC_GOOD.to_owned());
    }
    if c.mute.is_none() {
        c.mute = Some(STATIC_MUTE.to_owned());
    }
}

#[derive(Deserialize)]
struct Pywal {
    #[serde(default)]
    special: HashMap<String, String>,
    #[serde(default)]
    colors: HashMap<String, String>,
}

fn load_pywal() -> Result<Pywal, AnyError> {
    let path = wal_path();
    let text = fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let p: Pywal =
        serde_json::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_fills_defaults() {
        let mut c = ColorsConfig::default();
        resolve(&mut c);
        assert_eq!(c.default_color(), "#cdd6f4");
        assert_eq!(c.urgent(), "#f38ba8");
        assert_eq!(c.warn(), "#f9e2af");
        assert_eq!(c.good(), "#a6e3a1");
        assert_eq!(c.mute(), "#fab387");
    }

    #[test]
    fn pywal_mapping_with_explicit_override() {
        let mut c = ColorsConfig {
            source: ColorSource::Pywal,
            default: Some("#override".to_owned()),
            urgent: None,
            warn: None,
            good: None,
            mute: None,
        };
        let p = Pywal {
            special: HashMap::from([("foreground".to_owned(), "#fg".to_owned())]),
            colors: HashMap::from([
                ("color1".to_owned(), "#u1".to_owned()),
                ("color2".to_owned(), "#g2".to_owned()),
                ("color3".to_owned(), "#w3".to_owned()),
                ("color5".to_owned(), "#m5".to_owned()),
            ]),
        };
        apply_pywal(&mut c, &p);
        fill_static(&mut c);
        // Explicit TOML value wins over pywal.
        assert_eq!(c.default_color(), "#override");
        // Unset fields take pywal values.
        assert_eq!(c.urgent(), "#u1");
        assert_eq!(c.warn(), "#w3");
        assert_eq!(c.good(), "#g2");
        assert_eq!(c.mute(), "#m5");
    }

    #[test]
    fn pywal_missing_keys_fall_back_to_static() {
        let mut c = ColorsConfig {
            source: ColorSource::Pywal,
            default: None,
            urgent: None,
            warn: None,
            good: None,
            mute: None,
        };
        let p = Pywal {
            special: HashMap::from([("foreground".to_owned(), "#fg".to_owned())]),
            colors: HashMap::new(), // no color1..color5
        };
        apply_pywal(&mut c, &p);
        fill_static(&mut c);
        assert_eq!(c.default_color(), "#fg");
        assert_eq!(c.urgent(), "#f38ba8");
        assert_eq!(c.mute(), "#fab387");
    }
}
