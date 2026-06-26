//! Brightness block from `/sys/class/backlight/<dev>/{brightness,max_brightness}`.

use crate::config::Config;
use crate::icons::Icons;
use crate::modules::poller;
use crate::state::{Block, Ctx};
use crate::util;

pub fn spawn(ctx: Ctx) -> std::thread::JoinHandle<()> {
    let interval = ctx.config.modules.brightness.interval_ms;
    poller(ctx, "bri", interval, produce)
}

fn device_dir(cfg: &Config) -> Option<String> {
    if cfg.modules.brightness.device == "auto" {
        util::glob_class("backlight", "*").into_iter().next()
    } else {
        Some(format!(
            "/sys/class/backlight/{}",
            cfg.modules.brightness.device
        ))
    }
}

fn produce(cfg: &Config, icons: &Icons) -> Option<Block> {
    let dir = device_dir(cfg)?;
    let cur = util::read_int(&format!("{dir}/brightness"))?;
    let max = util::read_int(&format!("{dir}/max_brightness"))?;
    if max <= 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let pct = ((cur as f64 / max as f64) * 100.0).round() as i64;
    let icon = match pct {
        0..=29 => icons.get("bri_low"),
        30..=69 => icons.get("bri_mid"),
        _ => icons.get("bri_high"),
    };
    Some(Block::icon_text(icon, &format!("{pct}%")))
}
