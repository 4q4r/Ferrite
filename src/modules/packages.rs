//! Pending-updates block via a throttled, timeout-guarded fork of an external
//! counter (`checkupdates` by default). Forks at most once per `interval_ms`
//! (default 30 min) on its own thread, so a slow/hung counter never freezes a
//! bar tick. A machine without the counter simply never sets a block (hidden).

use crate::config::Config;
use crate::icons::Icons;
use crate::modules::poller;
use crate::state::{Block, Ctx};
use crate::util;

pub fn spawn(ctx: Ctx) -> std::thread::JoinHandle<()> {
    let interval = ctx.config.modules.packages.interval_ms;
    poller(ctx, "packages", interval, produce)
}

fn produce(cfg: &Config, icons: &Icons) -> Option<Block> {
    let timeout = cfg.modules.packages.timeout_ms.max(50);
    // `checkupdates` takes no arguments; a missing/timeout helper yields `None`
    // (anti-flicker: keep the last good count rather than flashing empty).
    let out = util::run_with_timeout(&cfg.modules.packages.command, &[], timeout)?;
    let n = out.lines().filter(|l| !l.trim().is_empty()).count();
    if n == 0 && cfg.modules.packages.hide_when_zero {
        // Drop to an empty block so snapshot_map hides it (clears a stale count).
        return Some(Block::text(""));
    }
    Some(Block::icon_text(icons.get("packages"), &n.to_string()).with_name("packages"))
}
