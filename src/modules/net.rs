//! Network block. Enumerates **every connected physical link** (Wi-Fi and/or
//! wired LAN), filtered by `modules.net.interfaces` (`["wifi"]`, `["lan"]`, or
//! `["wifi","lan"]` by default). rx/tx rate is zero-fork from `/proc/net/dev`,
//! tracked per-device; SSID/signal and the IPv4 address come from throttled,
//! timeout-guarded `iw`/`ip` forks (once per `interval_ms`, default 3 s).

use std::collections::HashMap;
use std::fs;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::config::{Config, NetKind, SignalFmt};
use crate::icons::Icons;
use crate::state::{Block, Ctx};
use crate::util;

/// Per-device rx/tx counters + last-sample instant, keyed by interface name.
struct Prev {
    rx: u64,
    tx: u64,
    at: Instant,
}

pub fn spawn(ctx: Ctx) -> JoinHandle<()> {
    let interval = Duration::from_millis(ctx.config.modules.net.interval_ms.max(50));
    thread::spawn(move || {
        let mut prev: HashMap<String, Prev> = HashMap::new();
        loop {
            if let Some(b) = build(&ctx.config, &ctx.icons, &mut prev) {
                ctx.state.set("net", b);
            }
            thread::sleep(interval);
        }
    })
}

fn build(cfg: &Config, icons: &Icons, prev: &mut HashMap<String, Prev>) -> Option<Block> {
    let kinds = &cfg.modules.net.interfaces;
    let want_wifi = kinds.contains(&NetKind::Wifi);
    let want_lan = kinds.contains(&NetKind::Lan);

    let mut segments: Vec<String> = Vec::new();
    let mut wifi_pct: Option<i64> = None;

    for (dev, wireless) in physical_interfaces() {
        if !(if wireless { want_wifi } else { want_lan }) {
            continue;
        }

        // rx/tx throughput from /proc/net/dev (zero-fork), Δ over wall-clock.
        let (rate_down, rate_up) = dev_rate(&dev, prev).unwrap_or((0, 0));
        let ip = if cfg.modules.net.show_ip {
            ip4(&dev).unwrap_or_default()
        } else {
            String::new()
        };

        if wireless {
            let (ssid, signal) = iw_link(&dev);
            let (pct, icon, sig_text) = signal_display(signal, cfg, icons);
            wifi_pct = Some(pct);
            let mut parts: Vec<String> = Vec::new();
            if !ssid.is_empty() {
                parts.push(ssid);
            }
            parts.push(sig_text);
            if !ip.is_empty() {
                parts.push(ip);
            }
            if cfg.modules.net.show_rate {
                parts.push(format!(
                    "↓{} ↑{}",
                    rate_str(rate_down, threshold_bps(cfg)),
                    rate_str(rate_up, threshold_bps(cfg))
                ));
            }
            segments.push(icon_text_str(icon, &parts.join("  ")));
        } else {
            let mut parts: Vec<String> = vec![dev.clone()];
            if !ip.is_empty() {
                parts.push(ip);
            }
            if cfg.modules.net.show_rate {
                parts.push(format!(
                    "↓{} ↑{}",
                    rate_str(rate_down, threshold_bps(cfg)),
                    rate_str(rate_up, threshold_bps(cfg))
                ));
            }
            segments.push(icon_text_str(icons.get("wired"), &parts.join("  ")));
        }
    }

    if segments.is_empty() {
        return None;
    }
    // Join interface segments with two spaces, matching the intra-block spacing
    // already used for SSID/signal/IP/rate — so "wifi  lan" reads as one block.
    let mut block = Block::text(segments.join("  "));
    // Color follows the Wi-Fi signal when present (the link worth a glance);
    // a pure-LAN block keeps the default bar color.
    if let Some(pct) = wifi_pct {
        block = block.with_color(color_for(pct, cfg));
    }
    Some(block)
}

/// Every physical interface that is currently connected: `(name, is_wireless)`.
/// "Physical" = has a `/sys/class/net/<dev>/device` symlink that resolves outside
/// `/sys/devices/virtual/...` — this excludes `lo`, bridges, `veth`, `docker*`,
/// and tun/tap VPNs while keeping PCI/USB NICs (wired and wireless). "Connected"
/// = `operstate == up` OR `carrier == 1`. Sorted for stable bar ordering.
fn physical_interfaces() -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    for name in util::list_dir("/sys/class/net") {
        if name == "lo" {
            continue;
        }
        let base = format!("/sys/class/net/{name}");
        let operstate = util::read_first_line(&format!("{base}/operstate")).unwrap_or_default();
        let carrier = util::read_int(&format!("{base}/carrier")).unwrap_or(0);
        if operstate != "up" && carrier != 1 {
            continue;
        }
        if !is_physical(&base) {
            continue;
        }
        let wireless = util::is_dir(&format!("{base}/wireless"));
        out.push((name, wireless));
    }
    out.sort();
    out
}

/// True when `<base>/device` resolves to a real bus device (PCI/USB), i.e. not
/// under `/sys/devices/virtual/`. Bridges/veth/tun either lack the symlink or
/// resolve into the virtual tree.
fn is_physical(base: &str) -> bool {
    fs::canonicalize(format!("{base}/device"))
        .map(|p| !p.to_string_lossy().contains("/virtual/"))
        .unwrap_or(false)
}

/// `"<icon> <text>"` with a single space between when the icon is non-empty —
/// same rule as `Block::icon_text`, but returns the raw string so several
/// interface segments can be joined into one block.
fn icon_text_str(icon: &str, text: &str) -> String {
    if icon.is_empty() {
        text.to_owned()
    } else {
        format!("{icon} {text}")
    }
}

/// `(rx_bytes, tx_bytes)` for `dev` from `/proc/net/dev`.
fn dev_bytes(dev: &str) -> Option<(u64, u64)> {
    let text = util::read_to_string("/proc/net/dev")?;
    for line in text.lines().skip(2) {
        let (lhs, rhs) = line.split_once(':')?;
        if lhs.trim() != dev {
            continue;
        }
        let n: Vec<u64> = rhs
            .split_whitespace()
            .filter_map(|f| f.parse::<u64>().ok())
            .collect();
        // 16 rx fields then 16 tx fields; rx_bytes=[0], tx_bytes=[8].
        let rx = n.first().copied()?;
        let tx = n.get(8).copied()?;
        return Some((rx, tx));
    }
    None
}

/// Δ rate `(down_bps, up_bps)` since the last sample of `dev`; updates `prev`.
/// First sample of a device yields `(0, 0)` (no elapsed baseline yet).
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]
fn dev_rate(dev: &str, prev: &mut HashMap<String, Prev>) -> Option<(u64, u64)> {
    let (rx, tx) = dev_bytes(dev)?;
    let now = Instant::now();
    let Some(p) = prev.get(dev) else {
        prev.insert(dev.to_owned(), Prev { rx, tx, at: now });
        return Some((0, 0));
    };
    let elapsed = now.duration_since(p.at).as_secs_f64();
    let down = if elapsed > 0.0 {
        ((rx.saturating_sub(p.rx) as f64) / elapsed) as u64
    } else {
        0
    };
    let up = if elapsed > 0.0 {
        ((tx.saturating_sub(p.tx) as f64) / elapsed) as u64
    } else {
        0
    };
    prev.insert(dev.to_owned(), Prev { rx, tx, at: now });
    Some((down, up))
}

/// `iw dev <dev> link` → `(ssid, signal_dbm)`. Both optional (ap of unknown SSID).
fn iw_link(dev: &str) -> (String, Option<i64>) {
    let out = util::run_with_timeout("iw", &["dev", dev, "link"], 800);
    let Some(out) = out else {
        return (String::new(), None);
    };
    let mut ssid = String::new();
    let mut signal: Option<i64> = None;
    for line in out.lines() {
        let t = line.trim();
        if let Some(idx) = t.find("SSID: ") {
            t[idx + "SSID: ".len()..]
                .trim()
                .trim_matches('"')
                .clone_into(&mut ssid);
        } else if let Some(idx) = t.find("signal:") {
            let rest = t[idx + "signal:".len()..].trim();
            // e.g. "-52 dBm" — take the leading integer.
            let num: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '-')
                .collect();
            if let Ok(dbm) = num.parse::<i64>() {
                signal = Some(dbm);
            }
        }
    }
    (ssid, signal)
}

/// `ip -4 -o addr show dev <dev>` → first IPv4 with prefix trimmed.
fn ip4(dev: &str) -> Option<String> {
    let out = util::run_with_timeout("ip", &["-4", "-o", "addr", "show", "dev", dev], 800)?;
    for line in out.lines() {
        if let Some(idx) = line.find("inet ") {
            let rest = &line[idx + "inet ".len()..];
            let addr: String = rest.chars().take_while(|c| *c != '/').collect();
            if !addr.is_empty() {
                return Some(addr);
            }
        }
    }
    None
}

/// `(signal_percent, wifi_icon, display_text)` from a dBm reading.
fn signal_display<'a>(
    signal: Option<i64>,
    cfg: &Config,
    icons: &'a Icons,
) -> (i64, &'a str, String) {
    signal.map_or_else(
        || (0, icons.get("wifi_off"), "—".to_owned()),
        |dbm| {
            let pct = ((dbm + 100) * 100 / 60).clamp(0, 100);
            let icon = match pct {
                80.. => icons.get("wifi_excellent"),
                60.. => icons.get("wifi_good"),
                40.. => icons.get("wifi_fair"),
                20.. => icons.get("wifi_weak"),
                _ => icons.get("wifi_off"),
            };
            let text = match cfg.modules.net.signal {
                SignalFmt::Percent => format!("{pct}%"),
                SignalFmt::Dbm => format!("{dbm}dBm"),
            };
            (pct, icon, text)
        },
    )
}

/// Weak signal is worth a glance (`colors.warn`); an excellent link keeps the
/// default bar color.
fn color_for(pct: i64, cfg: &Config) -> &str {
    if pct > 0 && pct < 25 {
        cfg.colors.warn()
    } else {
        cfg.colors.default_color()
    }
}

/// Bytes/sec below which we show `0B/s` instead of flickering sub-KB decimals.
const fn threshold_bps(cfg: &Config) -> u64 {
    cfg.modules.net.rate_threshold_kb.saturating_mul(1024)
}

/// Below the configured threshold we show `0B/s` instead of flickering decimals.
fn rate_str(bps: u64, threshold: u64) -> String {
    if bps < threshold {
        "0B/s".to_owned()
    } else {
        util::human_rate(bps)
    }
}
