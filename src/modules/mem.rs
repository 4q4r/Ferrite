//! Memory block from `/proc/meminfo` (zero-fork). `used = total - available`.

use crate::modules::poller;
use crate::state::{Block, Ctx};
use crate::util;

pub fn spawn(ctx: Ctx) -> std::thread::JoinHandle<()> {
    let interval = ctx.config.modules.mem.interval_ms;
    poller(ctx, "mem", interval, produce)
}

fn produce(cfg: &crate::config::Config, icons: &crate::icons::Icons) -> Option<Block> {
    let text = util::read_to_string("/proc/meminfo")?;
    let mut total_kb: Option<u64> = None;
    let mut avail_kb: Option<u64> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = rest.split_whitespace().next().and_then(|n| n.parse().ok());
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail_kb = rest.split_whitespace().next().and_then(|n| n.parse().ok());
        }
    }
    let total = total_kb?;
    let avail = avail_kb.unwrap_or(0);
    let used_kb = total.saturating_sub(avail);
    let used = util::human_bytes(used_kb * 1024);

    let body = match cfg.modules.mem.format {
        crate::config::MemFormat::Used => used,
        crate::config::MemFormat::UsedTotal => {
            format!("{used}/{}", util::human_bytes(total * 1024))
        }
    };
    let icon = if cfg.modules.mem.icon.is_empty() {
        icons.get("mem")
    } else {
        cfg.modules.mem.icon.as_str()
    };
    Some(Block::icon_text(icon, &body))
}
