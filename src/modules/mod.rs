//! Module registry: `spawn(name, ctx)` dispatches to a per-module thread.
//!
//! Three shapes live here:
//!   * **stateless pollers** — sysfs readers on a timer (mem/temp/bat/bri/disk/load/date/vpn);
//!   * **throttled forks** — volume/packages fork an external helper at most once per interval;
//!   * **stateful pollers** — cpu/net keep previous samples for Δ/rate math;
//!   * **event-driven** — bluetooth (zbus) and mpris (zbus session) block until input changes.
//!   * **polled IPC** — lang re-queries sway `get_inputs` on a timer: sway fires no event on xkb layout change.

pub mod bluetooth;
pub mod brightness;
pub mod cpu;
pub mod date;
pub mod disk;
pub mod lang;
pub mod load;
pub mod mem;
pub mod mpris;
pub mod net;
pub mod packages;
pub mod temp;
pub mod volume;
pub mod vpn;

use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::config::Config;
use crate::icons::Icons;
use crate::state::{Block, Ctx};

/// Spawn a module thread by name. `None` means the name is unknown — the render
/// loop then simply has no block for it (order entries are best-effort).
pub fn spawn(name: &str, ctx: Ctx) -> Option<JoinHandle<()>> {
    let handle = match name {
        "lang" => lang::spawn(ctx),
        "net" => net::spawn(ctx),
        "bt" => bluetooth::spawn(ctx),
        "vol" => volume::spawn(ctx),
        "bri" => brightness::spawn(ctx),
        "cpu" => cpu::spawn(ctx),
        "mem" => mem::spawn(ctx),
        "temp" => temp::spawn(ctx),
        "bat" => disk::spawn_battery(ctx),
        "load" => load::spawn(ctx),
        "disk" => disk::spawn_disk(ctx),
        "date" => date::spawn(ctx),
        "packages" => packages::spawn(ctx),
        "vpn" => vpn::spawn(ctx),
        "mpris" => mpris::spawn(ctx),
        _ => return None,
    };
    Some(handle)
}

/// Run `produce` every `interval_ms`, publishing its block under `name`.
/// A `None` return leaves the previous block untouched (anti-flicker on transient
/// read failures). `interval_ms` is clamped to ≥50 ms so a misconfigured timer
/// can never spin the CPU.
pub fn poller<F>(ctx: Ctx, name: &str, interval_ms: u64, produce: F) -> JoinHandle<()>
where
    F: Fn(&Config, &Icons) -> Option<Block> + Send + Sync + 'static,
{
    let name = name.to_owned();
    let interval = Duration::from_millis(interval_ms.max(50));
    thread::spawn(move || {
        loop {
            if let Some(b) = produce(&ctx.config, &ctx.icons) {
                ctx.state.set(&name, b);
            }
            thread::sleep(interval);
        }
    })
}
