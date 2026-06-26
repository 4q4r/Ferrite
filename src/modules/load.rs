//! Load average (1-min) + humanized uptime from `/proc/loadavg` and `/proc/uptime`.

use crate::modules::poller;
use crate::state::{Block, Ctx};
use crate::util;

pub fn spawn(ctx: Ctx) -> std::thread::JoinHandle<()> {
    let interval = ctx.config.modules.load.interval_ms;
    poller(ctx, "load", interval, produce)
}

fn produce(_cfg: &crate::config::Config, _icons: &crate::icons::Icons) -> Option<Block> {
    let loadavg = util::read_first_line("/proc/loadavg")?;
    let one = loadavg.split_whitespace().next()?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let up_secs = util::read_first_line("/proc/uptime")?
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()? as u64;
    Some(Block::text(format!("{one}  up {}", human_uptime(up_secs))))
}

/// `1d 2h 3m` — largest non-zero units only, e.g. `2h 3m` or `5m`.
fn human_uptime(secs: u64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}
