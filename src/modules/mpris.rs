//! Now-playing block via the **event-driven** MPRIS2 D-Bus interface — modeled
//! on `bluetooth.rs`. One snapshot of every `org.mpris.MediaPlayer2.*` name on
//! the session bus, then block on `PropertiesChanged` (track/playback changes)
//! and `NameOwnerChanged` (players appearing/vanishing) signals and re-snapshot.
//! Zero forks and zero CPU between track changes.

use std::collections::HashMap;
use std::thread::{self, JoinHandle};

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::message::Type;
use zbus::zvariant::OwnedValue;

use crate::state::{Block, Ctx};

#[derive(Clone)]
struct PlayerInfo {
    status: String,
    title: String,
    artist: String,
}

pub fn spawn(ctx: Ctx) -> JoinHandle<()> {
    thread::spawn(move || run_dbus(&ctx))
}

fn run_dbus(ctx: &Ctx) {
    let Ok(conn) = Connection::session() else {
        eprintln!("ferrite: mpris: cannot reach session D-Bus; block disabled");
        return;
    };
    set_block(&conn, ctx);

    // Player appear/disappear (a bus name is acquired or released) → re-snapshot.
    let ctx_owner = ctx.clone();
    thread::spawn(move || watch_signals(&ctx_owner, "NameOwnerChanged"));

    // Playback/metadata property changes → re-snapshot. This thread runs the loop.
    watch_signals(ctx, "PropertiesChanged");
}

/// Subscribe to one signal `member` on a fresh connection and re-snapshot on
/// every fire. A second connection per member keeps the proven `for_match_rule`
/// pattern (one rule per iterator) without a hand-rolled multi-rule loop.
fn watch_signals(ctx: &Ctx, member: &'static str) {
    let Ok(conn) = Connection::session() else {
        return;
    };
    let Some(rule) = build_rule(member) else {
        eprintln!("ferrite: mpris: cannot build D-Bus match rule for {member}");
        return;
    };
    let mut iter = match MessageIterator::for_match_rule(rule, &conn, Some(64)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("ferrite: mpris: cannot subscribe to {member} ({e})");
            return;
        }
    };
    while let Some(Ok(_)) = iter.next() {
        set_block(&conn, ctx);
    }
}

fn build_rule(member: &'static str) -> Option<MatchRule<'static>> {
    let builder = MatchRule::builder().msg_type(Type::Signal);
    let builder = builder.member(member).ok()?;
    Some(builder.build())
}

fn set_block(conn: &Connection, ctx: &Ctx) {
    match snapshot(conn, ctx) {
        Some(b) => ctx.state.set("mpris", b),
        // Empty block → snapshot_map hides it (clears a stale track on player quit).
        None => ctx.state.set("mpris", Block::text("")),
    }
}

/// Re-snapshot all MPRIS players and produce a block, or `None` when there is
/// nothing worth showing (no player, or a stopped player with `show_when_stopped`
/// off). A `Playing` player wins; otherwise the first readable player is used.
fn snapshot(conn: &Connection, ctx: &Ctx) -> Option<Block> {
    let dbus = Proxy::new(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .ok()?;
    let names: Vec<String> = dbus.call("ListNames", &()).ok()?;
    let players: Vec<String> = names
        .into_iter()
        .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
        .collect();
    if players.is_empty() {
        return None;
    }

    let mut first: Option<PlayerInfo> = None;
    let mut playing: Option<PlayerInfo> = None;
    for bus in &players {
        if let Some(info) = read_player(conn, bus) {
            if first.is_none() {
                first = Some(info.clone());
            }
            if info.status == "Playing" {
                playing = Some(info);
                break;
            }
        }
    }
    let info = playing.or(first)?;

    if info.status != "Playing" {
        // Stopped/Paused: only an idle icon when configured, else hide.
        return ctx
            .config
            .modules
            .mpris
            .show_when_stopped
            .then_some(Block::text(ctx.icons.get("music").to_owned()).with_name("mpris"));
    }

    let text = build_text(&info, ctx);
    let block = if text.is_empty() {
        Block::text(ctx.icons.get("music").to_owned())
    } else {
        Block::icon_text(ctx.icons.get("music"), &text)
    };
    Some(block.with_name("mpris"))
}

fn read_player(conn: &Connection, bus: &str) -> Option<PlayerInfo> {
    let player = Proxy::new(
        conn,
        bus,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .ok()?;
    let status: String = player.get_property("PlaybackStatus").ok()?;
    let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").unwrap_or_default();
    let title = metadata
        .get("xesam:title")
        .and_then(|v| <&str>::try_from(v).ok().map(str::to_owned))
        .unwrap_or_default();
    let artist = metadata
        .get("xesam:artist")
        .and_then(extract_artist)
        .unwrap_or_default();
    Some(PlayerInfo {
        status,
        title,
        artist,
    })
}

/// `xesam:artist` is an array of strings (`as`); some players send a bare
/// string. Handle both, taking the first artist.
fn extract_artist(v: &OwnedValue) -> Option<String> {
    if let Ok(s) = <&str>::try_from(v) {
        return Some(s.to_owned());
    }
    let arr: Vec<String> = Vec::try_from(v.clone()).ok()?;
    arr.into_iter().next()
}

/// Render the now-playing text from `format`. `{artist}` / `{title}` are literal
/// `str::replace` tokens, *not* `format!` arguments — the brace lints are silenced
/// deliberately. A bare title is used when no artist is present.
#[allow(clippy::literal_string_with_formatting_args)]
fn build_text(info: &PlayerInfo, ctx: &Ctx) -> String {
    let artist = info.artist.trim();
    let title = info.title.trim();
    let raw = if artist.is_empty() {
        title.to_owned()
    } else {
        ctx.config
            .modules
            .mpris
            .format
            .replace("{artist}", artist)
            .replace("{title}", title)
    };
    truncate(&raw, ctx.config.modules.mpris.max_len)
}

/// Truncate to `max` visible chars with a trailing `…` (0 = no limit).
fn truncate(s: &str, max: usize) -> String {
    if max == 0 || s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}
