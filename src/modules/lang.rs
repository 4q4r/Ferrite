//! Keyboard layout block, read from sway IPC `get_inputs`.
//!
//! **Why poll, not events?** Sway fires **no** Input event when the layout
//! changes via xkb options (`grp:win_space_toggle`, `grp:alt_shift_toggle`,
//! …) or via `xkb_switch_pattern next` — the active layout lives inside the
//! compositor and is only reachable through `get_inputs`. (Verified against
//! sway 1.12: a subscribe to `input` yields zero events across several
//! `xkb_switch_pattern next` calls, while `workspace` events stream fine.)
//! So the block re-queries `get_inputs` on `interval_ms` (default 1 s).
//! One IPC round-trip per tick, asleep the rest; clamped to ≥250 ms so a
//! misconfigured timer can never spin. 0-CPU-between-events is impossible
//! here because no event source exists — raise `interval_ms` to trade
//! freshness for fewer round-trips.

use std::thread::{self, JoinHandle};
use std::time::Duration;

use swayipc::Connection;

use crate::config::Config;
use crate::icons::Icons;
use crate::state::{Block, Ctx};

pub fn spawn(ctx: Ctx) -> JoinHandle<()> {
    let interval_ms = ctx.config.modules.lang.interval_ms;
    thread::spawn(move || run(&ctx.config, &ctx.icons, &ctx.state, interval_ms))
}

fn run(
    cfg: &Config,
    icons: &Icons,
    state: &std::sync::Arc<crate::state::BarState>,
    interval_ms: u64,
) {
    let interval = Duration::from_millis(interval_ms.max(250));
    // One persistent connection reused for every `get_inputs` round-trip — no
    // reconnect-per-tick churn. sway IPC is a unix socket, so this is cheap.
    let Ok(mut conn) = Connection::new() else {
        eprintln!("ferrite: lang: cannot connect to sway IPC (is sway running?)");
        return;
    };

    loop {
        if let Ok(inputs) = conn.get_inputs() {
            // First input that reports an active layout wins; else the fallback.
            let mut found = false;
            for input in &inputs {
                if let Some(layout) = &input.xkb_active_layout_name {
                    state.set("lang", block_for(cfg, icons, layout));
                    found = true;
                    break;
                }
            }
            if !found {
                state.set("lang", block_for(cfg, icons, &cfg.modules.lang.fallback));
            }
        }
        thread::sleep(interval);
    }
}

fn block_for(cfg: &Config, icons: &Icons, layout: &str) -> Block {
    let label = shorten(layout, cfg.modules.lang.shorten);
    let icon = icons.get("lang");
    Block::icon_text(icon, &label).with_name("lang")
}

/// Take the first `n` chars uppercased; `KB`-style fallback when `n == 0`.
fn shorten(layout: &str, n: usize) -> String {
    if n == 0 {
        return layout.to_owned();
    }
    let upper: String = layout.to_uppercase();
    upper.chars().take(n).collect()
}
