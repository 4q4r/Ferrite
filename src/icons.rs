//! Icon resolution: built-in packs (nerd/emoji/unicode/none) + custom TOML packs
//! from `~/.config/ferrite/packs/<name>.toml`, overlaid by `[icons.overrides]`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::config::{IconsConfig, config_dir};

/// Resolved icon table — every logical key maps to a glyph (empty if unset).
#[derive(Debug)]
pub struct Icons {
    map: HashMap<String, String>,
}

impl Icons {
    /// Build the table: built-in/custom pack first, then per-key overrides.
    pub fn build(cfg: &IconsConfig) -> Self {
        let mut map = pack_map(&cfg.pack);
        for (k, v) in &cfg.overrides {
            map.insert(k.clone(), v.clone());
        }
        Self { map }
    }

    /// Glyph for `key`, or `""` when unset (renders as nothing).
    pub fn get(&self, key: &str) -> &str {
        self.map.get(key).map_or("", String::as_str)
    }
}

/// Pack file shape: a single `[icons]` table.
#[derive(Deserialize)]
struct PackFile {
    #[serde(default)]
    icons: HashMap<String, String>,
}

/// Look up a pack by name: built-ins first, then `~/.config/ferrite/packs/<name>.toml`.
fn pack_map(name: &str) -> HashMap<String, String> {
    match name {
        "nerd" => nerd_pack(),
        "emoji" => emoji_pack(),
        "unicode" => unicode_pack(),
        "none" => HashMap::new(),
        custom => {
            let path = config_dir().join("packs").join(format!("{custom}.toml"));
            match std::fs::read_to_string(&path) {
                Ok(text) => toml::from_str::<PackFile>(&text).map_or_else(
                    |e| {
                        eprintln!("ferrite: icon pack {custom:?} parse error: {e}");
                        HashMap::new()
                    },
                    |p| p.icons,
                ),
                Err(e) => {
                    eprintln!("ferrite: icon pack {custom:?} unreadable: {e}");
                    HashMap::new()
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in packs
// ---------------------------------------------------------------------------

fn nerd_pack() -> HashMap<String, String> {
    // Glyphs are written as `\u{...}` escapes (not raw PUA chars) so the source
    // survives any editor/transfer that would otherwise strip little-used BMP
    // Private-Use-Area code points — which is how `cpu`/`mem`/`temp`/`bt`/`vol_*`
    // previously ended up empty. Code points match the original bash bar exactly
    // so ferrite renders byte-for-byte like `~/.config/sway/status.sh`.
    pack(&[
        ("cpu", "\u{F2DB}"),
        ("mem", "\u{EFC5}"),
        ("temp", "\u{F2C9}"),
        ("bat_charging", "\u{F0E7}"),
        ("bat_full", "\u{F0079}"),
        ("bat_high", "\u{F0082}"),
        ("bat_mid", "\u{F007E}"),
        ("bat_low", "\u{F007C}"),
        ("bat_empty", "\u{F007A}"),
        ("vol_mute", "\u{F0581}"),
        ("vol_low", "\u{F026}"),
        ("vol_mid", "\u{F027}"),
        ("vol_high", "\u{F028}"),
        ("bri_low", "\u{F00DE}"),
        ("bri_mid", "\u{F00DF}"),
        ("bri_high", "\u{F00E0}"),
        ("wifi_excellent", "\u{F0928}"),
        ("wifi_good", "\u{F0925}"),
        ("wifi_fair", "\u{F0922}"),
        ("wifi_weak", "\u{F091F}"),
        ("wifi_off", "\u{F092F}"),
        ("wired", "\u{F0200}"),
        ("bt_off", "\u{F00B2}"),
        ("bt_on", "\u{F293}"),
        ("bt_connected", "\u{F293}"),
        ("lang", ""),
        ("net_down", "\u{2193}"),
        ("net_up", "\u{2191}"),
        // Code points verified against the Nerd Font material icon index
        // (i3status-rust files/icons/material-nf.toml) — do not "fix" by eye.
        ("packages", "\u{F04D3}"),
        ("vpn", "\u{F0E9D}"),
        ("music", "\u{F075A}"),
    ])
}

fn emoji_pack() -> HashMap<String, String> {
    pack(&[
        ("cpu", "💻"),
        ("mem", "🧠"),
        ("temp", "🌡"),
        ("bat_charging", "🔌"),
        ("bat_full", "🔋"),
        ("bat_high", "🔋"),
        ("bat_mid", "🔋"),
        ("bat_low", "🪫"),
        ("bat_empty", "🪫"),
        ("vol_mute", "🔇"),
        ("vol_low", "🔈"),
        ("vol_mid", "🔉"),
        ("vol_high", "🔊"),
        ("bri_low", "🌙"),
        ("bri_mid", "⛅"),
        ("bri_high", "☀️"),
        ("wifi_excellent", "📶"),
        ("wifi_good", "📶"),
        ("wifi_fair", "📶"),
        ("wifi_weak", "📶"),
        ("wifi_off", "📴"),
        ("wired", "🌐"),
        ("bt_off", "⚪"),
        ("bt_on", "🔵"),
        ("bt_connected", "🟦"),
        ("lang", "⌨ "),
        ("net_down", "⬇"),
        ("net_up", "⬆"),
        ("packages", "📦"),
        ("vpn", "🔒"),
        ("music", "🎵"),
    ])
}

fn unicode_pack() -> HashMap<String, String> {
    pack(&[
        ("cpu", "CPU "),
        ("mem", "MEM "),
        ("temp", "T:"),
        ("bat_charging", "⚡"),
        ("bat_full", "▓"),
        ("bat_high", "▓"),
        ("bat_mid", "▒"),
        ("bat_low", "░"),
        ("bat_empty", "·"),
        ("vol_mute", "✗"),
        ("vol_low", "▁"),
        ("vol_mid", "▂"),
        ("vol_high", "▃"),
        ("bri_low", "◐"),
        ("bri_mid", "◑"),
        ("bri_high", "●"),
        ("wifi_excellent", "▆"),
        ("wifi_good", "▄"),
        ("wifi_fair", "▃"),
        ("wifi_weak", "▁"),
        ("wifi_off", "✗"),
        ("wired", "≡"),
        ("bt_off", "BT"),
        ("bt_on", "◉"),
        ("bt_connected", "●"),
        ("lang", ""),
        ("net_down", "↓"),
        ("net_up", "↑"),
        ("packages", "PKG "),
        ("vpn", "VPN "),
        ("music", "♪"),
    ])
}

/// Build a pack map from a `(&str, &str)` slice — both sides owned on insert,
/// so the `&str` literals never leave the surrounding inference ambiguous.
fn pack(entries: &[(&str, &str)]) -> HashMap<String, String> {
    let mut m = HashMap::with_capacity(entries.len());
    for (k, v) in entries {
        m.insert((*k).to_owned(), (*v).to_owned());
    }
    m
}
