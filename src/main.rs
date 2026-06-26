//! Ferrite — a magnetically attractive, low-CPU sway status bar.
//!
//! Architecture: one writer thread renders a snapshot of shared `BarState` to
//! stdout at `bar.render_interval_ms`; every module runs on its own thread and
//! updates only its own block. Event-driven modules (bluetooth, mpris) block until
//! the thing they watch actually changes. The result: the bar sleeps between
//! ticks and needs **no `timeout` in `status_command`**.

// `cargo_common_metadata` wants a `repository` URL, keywords, categories, and a
// readme. Keywords/categories/readme are set in Cargo.toml; a public `repository`
// is deliberately omitted (no canonical repo yet — do not fabricate one), so we
// silence just that one metadata lint rather than invent a URL.
#![allow(clippy::cargo_common_metadata)]

mod colors;
mod config;
mod icons;
mod layout;
mod modules;
mod protocol;
mod state;
mod util;

use std::io::{self, BufRead, Write};
use std::process::Stdio;
use std::thread;
use std::time::Duration;

use serde::Deserialize;

use crate::config::Protocol;
use crate::layout::Token;
use crate::state::{BarState, Ctx};

fn main() {
    if let Err(e) = run() {
        eprintln!("ferrite: {e}");
        std::process::exit(1);
    }
}

type AnyError = Box<dyn std::error::Error + Send + Sync>;

struct Args {
    plain: bool,
    print_config: bool,
    config_path: Option<String>,
}

fn parse_args() -> Result<Args, AnyError> {
    let mut args = Args {
        plain: false,
        print_config: false,
        config_path: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--plain" => args.plain = true,
            "--print-config" => args.print_config = true,
            "--config" => {
                args.config_path = Some(
                    it.next()
                        .ok_or_else(|| -> AnyError { "--config needs a path".into() })?,
                );
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other:?}").into()),
        }
    }
    Ok(args)
}

fn print_help() {
    eprintln!(
        "ferrite {version} — low-CPU configurable sway status bar\n\
         \n\
         Usage: ferrite [--plain] [--config <path>] [--print-config]\n\
         \n\
         --plain         text output (like the old bash bar), no JSON/colors\n\
         --config <path> load config from <path> instead of ~/.config/ferrite/config.toml\n\
         --print-config  dump the built-in default config to stdout and exit\n\
         \n\
         Put `ferrite` in your sway config as `status_command = \"ferrite\"` (no timeout needed).",
        version = env!("CARGO_PKG_VERSION")
    );
}

fn run() -> Result<(), AnyError> {
    let args = parse_args()?;

    if args.print_config {
        io::stdout().write_all(config::DEFAULT.as_bytes())?;
        return Ok(());
    }

    let mut cfg = match args.config_path {
        Some(p) => config::load_from(&p)?,
        None => config::load_or_init()?,
    };
    if args.plain {
        cfg.bar.protocol = Protocol::Plain;
    }

    // Resolve the color palette once (pywal/static) before the config is shared.
    colors::resolve(&mut cfg.colors);

    let icons = crate::icons::Icons::build(&cfg.icons);
    let state = BarState::default();
    let ctx = Ctx::new(cfg, icons, state);

    // The layout template names the modules to spawn. Parse once, spawn each
    // referenced module (unknown names are skipped — no block for them).
    let tokens = layout::parse(&ctx.config.bar.layout);
    for name in layout::module_names(&tokens) {
        if modules::spawn(&name, ctx.clone()).is_none() {
            eprintln!("ferrite: unknown module {name:?} in layout (skipped)");
        }
    }

    let stdout = io::stdout();
    let plain = protocol::is_plain(&ctx.config);
    if plain {
        render_loop_plain(stdout.lock(), &ctx, &tokens);
    } else {
        let mut out = stdout.lock();
        protocol::write_header(&mut out, ctx.config.bar.click_events)?;
        if ctx.config.bar.click_events {
            let click_ctx = ctx.clone();
            thread::spawn(move || click_loop(&click_ctx));
        }
        render_loop_i3bar(out, &ctx, &tokens);
    }
    Ok(())
}

fn render_loop_i3bar(mut out: io::StdoutLock<'_>, ctx: &Ctx, tokens: &[Token]) {
    let interval = Duration::from_millis(ctx.config.bar.render_interval_ms.max(20));
    loop {
        let map = ctx.state.snapshot_map();
        let blocks = protocol::build_blocks(tokens, &map, &ctx.config);
        if protocol::write_tick(&mut out, &blocks).is_err() {
            // stdout closed (sway exited) — stop quietly.
            break;
        }
        thread::sleep(interval);
    }
}

fn render_loop_plain(mut out: io::StdoutLock<'_>, ctx: &Ctx, tokens: &[Token]) {
    let interval = Duration::from_millis(ctx.config.bar.render_interval_ms.max(20));
    loop {
        let map = ctx.state.snapshot_map();
        let blocks = protocol::build_blocks(tokens, &map, &ctx.config);
        if protocol::write_plain(&mut out, &blocks).is_err() {
            break;
        }
        thread::sleep(interval);
    }
}

// ---------------------------------------------------------------------------
// Click events (stdin)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Click {
    name: Option<String>,
    button: Option<u32>,
}

/// Read i3bar click objects from stdin and route them through the config table.
/// The stream is a JSON array opened with `[` and continued with `,{...}` lines;
/// we strip the leading `[`/`,` and trailing `,` and parse each line as a click.
fn click_loop(ctx: &Ctx) {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if let Some(click) = parse_click_line(&line) {
            handle_click(&click, ctx);
        }
    }
}

/// Parse one stdin line of the i3bar click stream. The stream is an "infinite
/// array": it opens with `[` and every event is comma-prefixed (`,{...}`), with
/// sway putting the first event on the same line as the `[` (`[{...}`). We strip
/// both leading `[`/`,` (and a trailing `,`) so each line becomes a bare object.
/// Empty lines (a lone `[` opener) and malformed lines are skipped, not fatal.
fn parse_click_line(line: &str) -> Option<Click> {
    let trimmed = line.trim_start_matches(['[', ',']).trim();
    let trimmed = trimmed.trim_end_matches(',').trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str::<Click>(trimmed).ok()
}

/// Route a click via `[click_actions.<name>]`: `<button> = "<shell cmd>"`. The
/// button key is matched as a string, with `"*"` as a fallback for any unlisted
/// button. The command runs under `sh -c` (pipes, `$term`, quotes all work) and
/// is waited on so we reap it instead of accumulating zombies.
fn handle_click(click: &Click, ctx: &Ctx) {
    let Some(name) = click.name.as_deref() else {
        return;
    };
    let Some(table) = ctx.config.click_actions.get(name) else {
        return;
    };
    let button = click.button.unwrap_or(0).to_string();
    let Some(cmd) = table.get(button.as_str()).or_else(|| table.get("*")) else {
        return;
    };
    fire_shell(cmd);
}

/// Fire-and-forget a shell command on a throwaway thread that waits on the
/// child. Running on its own thread means a long-running helper never blocks
/// click dispatch (the next click is read immediately); the thread still reaps
/// its child, so we don't accumulate zombies. Output is discarded; a wedged
/// helper is bounded by the OS, not by the bar.
fn fire_shell(cmd: &str) {
    let cmd = cmd.to_owned();
    thread::spawn(move || {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .and_then(|mut c| c.wait());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_first_click_opens_array_with_bracket() {
        // sway puts the first event on the same line as the opening `[`.
        let c = parse_click_line("[{\"name\":\"temp\",\"button\":1}").unwrap();
        assert_eq!(c.name.as_deref(), Some("temp"));
        assert_eq!(c.button, Some(1));
    }

    #[test]
    fn parse_subsequent_click_is_comma_prefixed() {
        // The regression: every event after the first is `,{...}`. The leading
        // comma must be stripped, or only the first click ever routes.
        let c = parse_click_line(",{\"name\":\"vol\",\"button\":4}").unwrap();
        assert_eq!(c.name.as_deref(), Some("vol"));
        assert_eq!(c.button, Some(4));
    }

    #[test]
    fn parse_lone_array_opener_is_skipped() {
        // A bare `[` (opener on its own line) carries no event.
        assert!(parse_click_line("[").is_none());
        assert!(parse_click_line("   ").is_none());
    }

    #[test]
    fn parse_malformed_is_skipped_not_panicked() {
        assert!(parse_click_line(",{bad json").is_none());
    }
}
