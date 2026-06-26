//! Temperature block from `/sys/class/thermal/thermal_zone*/temp` (zero-fork).
//! Turns urgent/red above `critical` °C.

use crate::config::Config;
use crate::icons::Icons;
use crate::modules::poller;
use crate::state::{Block, Ctx};
use crate::util;

pub fn spawn(ctx: Ctx) -> std::thread::JoinHandle<()> {
    let interval = ctx.config.modules.temp.interval_ms;
    poller(ctx, "temp", interval, produce)
}

fn zone_path(cfg: &Config) -> Option<String> {
    if cfg.modules.temp.zone == "auto" {
        util::glob_class("thermal", "thermal_zone*")
            .into_iter()
            .next()
            .map(|p| format!("{p}/temp"))
    } else {
        Some(cfg.modules.temp.zone.clone())
    }
}

fn produce(cfg: &Config, icons: &Icons) -> Option<Block> {
    let path = zone_path(cfg)?;
    let millic = util::read_int(&path)?;
    let c = millic / 1000;
    let icon = if cfg.modules.temp.icon.is_empty() {
        icons.get("temp")
    } else {
        cfg.modules.temp.icon.as_str()
    };
    let mut block = Block::icon_text(icon, &format!("{c}°C"));
    if c >= cfg.modules.temp.critical {
        block = block.with_color(cfg.colors.urgent()).with_urgent();
    }
    Some(block)
}
