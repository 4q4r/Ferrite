//! Battery (`bat`) and filesystem (`disk`) blocks — both live here because they
//! share the `glob_class`/`statvfs` helpers and are simple stateless pollers.

use crate::config::{Config, TimeFmt};
use crate::icons::Icons;
use crate::modules::poller;
use crate::state::{Block, Ctx};
use crate::util;

// ---------------------------------------------------------------------------
// Battery
// ---------------------------------------------------------------------------

pub fn spawn_battery(ctx: Ctx) -> std::thread::JoinHandle<()> {
    let interval = ctx.config.modules.battery.interval_ms;
    poller(ctx, "bat", interval, battery_block)
}

fn battery_dir(cfg: &Config) -> Option<String> {
    if cfg.modules.battery.device == "auto" {
        util::glob_class("power_supply", "BAT*").into_iter().next()
    } else {
        Some(format!(
            "/sys/class/power_supply/{}",
            cfg.modules.battery.device
        ))
    }
}

fn battery_block(cfg: &Config, icons: &Icons) -> Option<Block> {
    let dir = battery_dir(cfg)?;
    let status = util::read_first_line(&format!("{dir}/status")).unwrap_or_default();
    let pct = util::read_int(&format!("{dir}/capacity")).or_else(|| {
        // Fallback for batteries without `capacity`: energy_now / energy_full.
        let now = util::read_milli(&format!("{dir}/energy_now"))?;
        let full = util::read_milli(&format!("{dir}/energy_full"))?;
        if full <= 0 {
            return None;
        }
        Some((now * 100) / full)
    })?;
    let charging = status == "Charging";
    let icon = if charging {
        icons.get("bat_charging")
    } else {
        match pct {
            90.. => icons.get("bat_full"),
            70.. => icons.get("bat_high"),
            50.. => icons.get("bat_mid"),
            30.. => icons.get("bat_low"),
            _ => icons.get("bat_empty"),
        }
    };
    let mut text = format!("{pct}%");
    if cfg.modules.battery.show_time {
        if let Some(t) = time_remaining(&dir, charging) {
            text.push(' ');
            text.push_str(&format_time(t, cfg.modules.battery.time_format));
        }
    }
    let mut block = Block::icon_text(icon, &text);
    // Warn/crit only while discharging — a charging battery is recovering.
    if !charging && status != "Full" {
        if pct <= cfg.modules.battery.crit {
            block = block.with_color(cfg.colors.urgent()).with_urgent();
        } else if pct <= cfg.modules.battery.warn {
            block = block.with_color(cfg.colors.warn());
        }
    }
    Some(block)
}

/// `(hours, minutes)` of battery time remaining, zero-fork from sysfs. Tries the
/// `energy`/`power` (µWh/µW) pair first, then `charge`/`current` (µAh/µA). The
/// micro-units cancel, so `remaining / rate` is directly hours. Returns `None`
/// when there is no rate file or the rate is zero (no estimate available).
fn time_remaining(dir: &str, charging: bool) -> Option<(i64, i64)> {
    let rate = util::read_int(&format!("{dir}/power_now"))
        .or_else(|| util::read_int(&format!("{dir}/current_now")))?;
    if rate <= 0 {
        return None;
    }
    let (now, full) = util::read_int(&format!("{dir}/energy_now"))
        .and_then(|n| util::read_int(&format!("{dir}/energy_full")).map(|f| (n, f)))
        .or_else(|| {
            let n = util::read_int(&format!("{dir}/charge_now"))?;
            let f = util::read_int(&format!("{dir}/charge_full"))?;
            Some((n, f))
        })?;
    // Remaining charge: how much is left to drain (discharging) or to fill
    // (charging). Both `now`/`full` and `rate` share the same micro-unit, so the
    // division yields hours directly; minutes = hours * 60.
    let remaining = if charging {
        (full - now).max(0)
    } else {
        now.max(0)
    };
    let total_minutes = remaining.saturating_mul(60) / rate;
    Some((total_minutes / 60, total_minutes % 60))
}

/// `Auto` → `2h15` (≥1 h) or `45m` (<1 h); `HM` → always `H:MM`.
fn format_time((h, m): (i64, i64), fmt: TimeFmt) -> String {
    match fmt {
        TimeFmt::Auto if h >= 1 => format!("{h}h{m}"),
        TimeFmt::Auto => format!("{m}m"),
        TimeFmt::HM => format!("{h}:{m:02}"),
    }
}

// ---------------------------------------------------------------------------
// Disk
// ---------------------------------------------------------------------------

pub fn spawn_disk(ctx: Ctx) -> std::thread::JoinHandle<()> {
    let interval = ctx.config.modules.disk.interval_ms;
    poller(ctx, "disk", interval, disk_block)
}

fn disk_block(cfg: &Config, _icons: &Icons) -> Option<Block> {
    let mut parts: Vec<String> = Vec::new();
    for mount in &cfg.modules.disk.mounts {
        let Some((total, avail)) = util::statvfs(mount) else {
            continue;
        };
        let used = total.saturating_sub(avail);
        let pct = used_pct(used, total);
        parts.push(format!("{mount} {} ({}%)", util::human_bytes(used), pct));
    }
    if parts.is_empty() {
        return None;
    }
    Some(Block::text(parts.join("  ")))
}

/// `used / total` as whole percent (0 when `total == 0`). The f64 round-trip is
/// intentional: integer division would flatten small deltas to zero.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn used_pct(used: u64, total: u64) -> i64 {
    if total == 0 {
        0
    } else {
        ((used as f64 / total as f64) * 100.0).round() as i64
    }
}
