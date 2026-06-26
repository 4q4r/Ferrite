//! Volume block via throttled `pamixer` forks (timeout-guarded). No `PulseAudio`
//! Rust binding keeps the dependency surface tiny; `pamixer` is the same helper
//! the original bash used, forked at most once per `interval_ms`.

use crate::config::Config;
use crate::icons::Icons;
use crate::modules::poller;
use crate::state::{Block, Ctx};
use crate::util;

pub fn spawn(ctx: Ctx) -> std::thread::JoinHandle<()> {
    let interval = ctx.config.modules.volume.interval_ms;
    poller(ctx, "vol", interval, produce)
}

fn produce(cfg: &Config, icons: &Icons) -> Option<Block> {
    let timeout = cfg.modules.volume.timeout_ms.max(50);
    let mute = util::run_with_timeout("pamixer", &["--get-mute"], timeout)?;
    let muted = mute.trim() == "true";
    let vol = util::run_with_timeout("pamixer", &["--get-volume"], timeout)?
        .trim()
        .parse::<i64>()
        .ok()?;

    if muted {
        return Some(Block::icon_text(icons.get("vol_mute"), "Mute").with_color(cfg.colors.mute()));
    }
    let icon = match vol {
        0..=29 => icons.get("vol_low"),
        30..=69 => icons.get("vol_mid"),
        _ => icons.get("vol_high"),
    };
    Some(Block::icon_text(icon, &format!("{vol}%")))
}
