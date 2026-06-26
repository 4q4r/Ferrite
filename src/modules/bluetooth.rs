//! Bluetooth block. Default backend is **event-driven D-Bus** (`org.bluez`):
//! one `GetManagedObjects` snapshot, then block on `PropertiesChanged` signals —
//! zero forks and zero CPU between device connect/disconnect events. A degraded
//! `bluetoothctl` poll backend is available for systems without a usable D-Bus.

use std::collections::HashMap;
use std::thread::{self, JoinHandle};

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::message::Type;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::config::BtBackend;
use crate::icons::Icons;
use crate::modules::poller;
use crate::state::{Block, Ctx};
use crate::util;

/// `a{oa{sa{sv}}}` — the body of `org.freedesktop.DBus.ObjectManager.GetManagedObjects`.
type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

pub fn spawn(ctx: Ctx) -> JoinHandle<()> {
    let interval = ctx.config.modules.bluetooth.interval_ms;
    match ctx.config.modules.bluetooth.backend {
        BtBackend::Dbus => thread::spawn(move || run_dbus(&ctx)),
        BtBackend::Bluetoothctl => poller(ctx, "bt", interval, btctl_block),
    }
}

// ---------------------------------------------------------------------------
// D-Bus event-driven backend
// ---------------------------------------------------------------------------

fn run_dbus(ctx: &Ctx) {
    let conn = match Connection::system() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ferrite: bt: cannot reach system D-Bus ({e}); block disabled");
            return;
        }
    };

    // Initial state.
    if let Some(b) = snapshot(&conn, &ctx.icons) {
        ctx.state.set("bt", b);
    }

    // Match every org.bluez PropertiesChanged (any path/interface) — adapter power
    // toggles and device connect/disconnects both emit it.
    let Some(rule) = build_rule() else {
        eprintln!("ferrite: bt: cannot build D-Bus match rule");
        return;
    };
    let mut iter = match MessageIterator::for_match_rule(rule, &conn, Some(16)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("ferrite: bt: cannot subscribe to bluez ({e})");
            return;
        }
    };

    while let Some(Ok(_msg)) = iter.next() {
        if let Some(b) = snapshot(&conn, &ctx.icons) {
            ctx.state.set("bt", b);
        }
    }
}

fn build_rule() -> Option<MatchRule<'static>> {
    // Builder setters return `Result` only for name validation; "org.bluez" /
    // "PropertiesChanged" are always valid, but we still avoid panics via matches.
    let builder = MatchRule::builder().msg_type(Type::Signal);
    let builder = builder.sender("org.bluez").ok()?;
    let builder = builder.member("PropertiesChanged").ok()?;
    Some(builder.build())
}

/// Re-snapshot all `BlueZ` objects and produce a block, or `None` when there's no
/// Bluetooth adapter at all (so BT-less machines don't show a phantom "off").
fn snapshot(conn: &Connection, icons: &Icons) -> Option<Block> {
    let om = Proxy::new(conn, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager").ok()?;
    let objects: ManagedObjects = om.call("GetManagedObjects", &()).ok()?;

    let mut has_adapter = false;
    let mut powered = false;
    let mut connected_name: Option<String> = None;

    for ifaces in objects.values() {
        if let Some(adapter) = ifaces.get("org.bluez.Adapter1") {
            has_adapter = true;
            if let Some(v) = adapter.get("Powered") {
                if bool::try_from(v).unwrap_or(false) {
                    powered = true;
                }
            }
        }
        if let Some(device) = ifaces.get("org.bluez.Device1") {
            let connected = device
                .get("Connected")
                .and_then(|v| bool::try_from(v).ok())
                .unwrap_or(false);
            if connected {
                let name = device
                    .get("Name")
                    .or_else(|| device.get("Alias"))
                    .and_then(|v| <&str>::try_from(v).ok().map(str::to_owned));
                if connected_name.is_none() {
                    connected_name = name;
                }
            }
        }
    }

    if !has_adapter {
        return None;
    }

    // Match the bash bar byte-for-byte: powered-off → bare icon (no "off" text,
    // no trailing space); powered-on, no device → "<icon> On"; connected →
    // "<icon> <name>".
    let block = connected_name
        .map_or_else(
            || {
                if powered {
                    Block::icon_text(icons.get("bt_on"), "On")
                } else {
                    Block::text(icons.get("bt_off").to_owned())
                }
            },
            |name| Block::icon_text(icons.get("bt_connected"), &truncate(&name, 14)),
        )
        .with_name("bt");
    Some(block)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

// ---------------------------------------------------------------------------
// bluetoothctl fallback backend (adapter power only)
// ---------------------------------------------------------------------------

fn btctl_block(_cfg: &crate::config::Config, icons: &Icons) -> Option<Block> {
    let out = util::run_with_timeout("bluetoothctl", &["show"], 800)?;
    let mut powered = false;
    let mut has_controller = false;
    for line in out.lines() {
        let t = line.trim();
        if t.starts_with("Controller") {
            has_controller = true;
        } else if let Some(rest) = t.strip_prefix("Powered:") {
            powered = rest.trim().eq_ignore_ascii_case("yes");
        }
    }
    if !has_controller {
        return None;
    }
    // Match the bash bar: off → bare icon; on → "<icon> On".
    let block = if powered {
        Block::icon_text(icons.get("bt_on"), "On")
    } else {
        Block::text(icons.get("bt_off").to_owned())
    };
    Some(block.with_name("bt"))
}
