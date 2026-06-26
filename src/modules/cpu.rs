//! CPU usage block from `/proc/stat` (zero-fork). Keeps the previous jiffie sample
//! to compute a Δ percentage — the first tick has no baseline and is skipped.

use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::state::{Block, Ctx};

pub fn spawn(ctx: Ctx) -> JoinHandle<()> {
    let interval = Duration::from_millis(ctx.config.modules.cpu.interval_ms.max(50));
    thread::spawn(move || {
        let mut prev: Option<(u64, u64)> = None; // (total, busy)
        loop {
            if let Some((total, busy)) = read_jiffies() {
                if let Some((pt, pb)) = prev {
                    let dt = total.saturating_sub(pt);
                    let du = busy.saturating_sub(pb);
                    if dt > 0 {
                        // Round-to-nearest: (du*100 + dt/2) / dt, integer arithmetic.
                        let pct = (du * 100 + dt / 2) / dt;
                        let icon = if ctx.config.modules.cpu.icon.is_empty() {
                            ctx.icons.get("cpu")
                        } else {
                            ctx.config.modules.cpu.icon.as_str()
                        };
                        ctx.state
                            .set("cpu", Block::icon_text(icon, &format!("{pct}%")));
                    }
                }
                prev = Some((total, busy));
            }
            thread::sleep(interval);
        }
    })
}

/// Aggregate `cpu` line → `(total_jiffies, busy_jiffies)`.
/// Busy = user+nice+system+irq+softirq+steal (everything that isn't idle/iowait).
fn read_jiffies() -> Option<(u64, u64)> {
    let line = crate::util::read_first_line("/proc/stat")?;
    let mut it = line.split_whitespace();
    it.next()?; // "cpu"
    let fields: Vec<u64> = it.filter_map(|f| f.parse::<u64>().ok()).collect();
    // user nice system idle iowait irq softirq steal guest guest_nice
    let user = fields.first().copied().unwrap_or(0);
    let nice = fields.get(1).copied().unwrap_or(0);
    let system = fields.get(2).copied().unwrap_or(0);
    let idle = fields.get(3).copied().unwrap_or(0);
    let iowait = fields.get(4).copied().unwrap_or(0);
    let irq = fields.get(5).copied().unwrap_or(0);
    let softirq = fields.get(6).copied().unwrap_or(0);
    let steal = fields.get(7).copied().unwrap_or(0);
    let busy = user + nice + system + irq + softirq + steal;
    let total = busy + idle + iowait;
    Some((total, busy))
}
