//! VPN block, zero-fork from sysfs. Any `/sys/class/net/<dev>` whose name
//! matches a configured glob (`zt*`, `wg*`, `tun*`, `tap*` by default) and whose
//! link is up counts as an active VPN — checked via `carrier == 1`, falling back
//! to `operstate == "up"` (ZeroTier/WireGuard/tun report `operstate = "unknown"`
//! while carrying traffic, so `carrier` is the reliable signal). Zero
//! subprocesses, a couple of micro-reads per tick.

use crate::config::Config;
use crate::icons::Icons;
use crate::modules::poller;
use crate::state::{Block, Ctx};
use crate::util;

pub fn spawn(ctx: Ctx) -> std::thread::JoinHandle<()> {
    let interval = ctx.config.modules.vpn.interval_ms;
    poller(ctx, "vpn", interval, produce)
}

/// All paths return a block, but the `poller` contract requires `Option<Block>`
/// (so a future `None` "keep last value" path fits without changing the type).
#[allow(clippy::unnecessary_wraps)]
fn produce(cfg: &Config, icons: &Icons) -> Option<Block> {
    let mut active = 0usize;
    for name in util::list_dir("/sys/class/net") {
        if !cfg
            .modules
            .vpn
            .patterns
            .iter()
            .any(|p| util::glob_match(p, &name))
        {
            continue;
        }
        // ZeroTier/WireGuard/tun interfaces report `operstate = "unknown"` even
        // when carrying traffic, so `operstate == "up"` alone misses them. The
        // link-present signal is `carrier == 1` (set for real links, 0 for a
        // down/disconnected interface); fall back to `operstate == "up"` for
        // interfaces that expose no `carrier` file.
        let operstate =
            util::read_first_line(&format!("/sys/class/net/{name}/operstate")).unwrap_or_default();
        let carrier =
            util::read_first_line(&format!("/sys/class/net/{name}/carrier")).unwrap_or_default();
        if operstate == "up" || carrier == "1" {
            active += 1;
        }
    }
    if active == 0 && !cfg.modules.vpn.show_when_off {
        // Empty block → hidden by snapshot_map (no phantom "off").
        return Some(Block::text(""));
    }
    let text = match active {
        0 => "Off".to_owned(),
        1 => "On".to_owned(),
        n => format!("On ×{n}"),
    };
    Some(Block::icon_text(icons.get("vpn"), &text).with_name("vpn"))
}
