//! i3bar protocol framing: header, per-tick block-array lines, and plain-text mode.
//!
//! The per-tick block list is built from the layout template
//! ([`crate::layout`]): module placeholders are filled from the state snapshot
//! and the literal text between them becomes its own separator block, so the
//! template — not a fixed separator string — controls all spacing.

use std::collections::HashMap;
use std::io::{self, Write};

use serde::Serialize;

use crate::config::{Config, Markup, Protocol};
use crate::layout::Token;
use crate::state::Block;

/// i3bar header. `click_events` tells sway to forward mouse clicks on stdin.
#[derive(Serialize)]
struct Header {
    version: u8,
    click_events: bool,
    // sway also accepts `stop_signal`/`cont_signal`, but the defaults
    // (SIGSTOP/SIGCONT) already give us the 0-CPU-when-hidden behavior we want.
}

/// Emit the i3bar header + opening `[`. The stream is then an infinite array of
/// block-arrays separated by `,\n`.
pub fn write_header(out: &mut impl Write, click_events: bool) -> io::Result<()> {
    let header = Header {
        version: 1,
        click_events,
    };
    writeln!(out, "{}", serde_json::to_string(&header)?)?;
    writeln!(out, "[")?;
    out.flush()
}

/// Build the per-tick block list by walking the layout template.
///
/// Module placeholders are filled from `map`; literal separator text becomes a
/// [`Block::separator`] (no native separator line, no gap) so the template's
/// separators carry all spacing. A separator is held until a *present* module
/// follows it, so a hidden module cleanly drops its surrounding separators
/// rather than leaving a dangling `" | "`.
pub fn build_blocks(tokens: &[Token], map: &HashMap<String, Block>, cfg: &Config) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut pending_sep: Option<String> = None;
    for token in tokens {
        match token {
            Token::Sep(s) => pending_sep = Some(s.clone()),
            Token::Module(name) => {
                if let Some(mut b) = map.get(name).cloned() {
                    if let Some(s) = pending_sep.take()
                        && !s.is_empty()
                    {
                        out.push(Block::separator(s));
                    }
                    // Inject the layout name when the module didn't set its own,
                    // so every block is click-routable via `[click_actions.<name>]`.
                    if b.name.is_none() {
                        b.name = Some(name.clone());
                    }
                    out.push(style_block(b, cfg));
                }
            }
        }
    }
    out
}

/// Render one tick: write `[{blocks}],\n`.
pub fn write_tick(out: &mut impl Write, blocks: &[Block]) -> io::Result<()> {
    let line = serde_json::to_string(blocks)?;
    writeln!(out, "{line},")?;
    out.flush()
}

/// Apply bar-wide defaults (markup, separator width, default color) to a module
/// block. Native i3bar separator lines are disabled (`separator:false`) because
/// the layout template supplies its own text separators; `separator_block_width`
/// is the extra gap after the block (0 = separators carry all spacing, matching
/// the bash look; >0 = a little breathing room on top).
fn style_block(mut b: Block, cfg: &Config) -> Block {
    if b.color.is_none() {
        // Resolved `colors.default` wins; fall back to `bar.default_color` for
        // configs that never set a `[colors]` palette (backward compatibility).
        let dc = cfg.colors.default_color();
        if !dc.is_empty() {
            b.color = Some(dc.to_owned());
        } else if !cfg.bar.default_color.is_empty() {
            b.color = Some(cfg.bar.default_color.clone());
        }
    }
    if b.separator.is_none() {
        b.separator = Some(false);
    }
    if b.separator_block_width.is_none() {
        b.separator_block_width = Some(cfg.bar.separator_block_width);
    }
    if b.markup.is_none() {
        b.markup = match cfg.bar.markup {
            Markup::Pango => Some("pango".to_owned()),
            Markup::None => None,
        };
    }
    b
}

/// Plain-text mode (like the old bash): concatenate `full_text` of every block
/// in template order — module blocks and separator text alike — into one line.
pub fn write_plain(out: &mut impl Write, blocks: &[Block]) -> io::Result<()> {
    let line: String = blocks.iter().map(|b| b.full_text.as_str()).collect();
    writeln!(out, "{line}")?;
    out.flush()
}

/// True when the bar is configured for plain text (no JSON, no colors).
pub const fn is_plain(cfg: &Config) -> bool {
    matches!(cfg.bar.protocol, Protocol::Plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{self, Config};
    use crate::layout;

    fn cfg() -> Config {
        toml::from_str(config::DEFAULT).expect("default config must parse")
    }

    #[test]
    fn module_name_is_injected_when_unset() {
        // A block produced without an explicit `name` (the common case for
        // sysfs/poller modules) must still be click-routable: build_blocks
        // stamps it with the layout placeholder name.
        let tokens = layout::parse("{vol}");
        let mut map = HashMap::new();
        map.insert("vol".to_owned(), Block::text("50%")); // no with_name
        let blocks = build_blocks(&tokens, &map, &cfg());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name.as_deref(), Some("vol"));
    }

    #[test]
    fn explicit_name_is_preserved() {
        // A module that sets its own name (e.g. bluetooth with an instance)
        // must not be overwritten by the layout name.
        let tokens = layout::parse("{bt}");
        let mut map = HashMap::new();
        let mut b = Block::text("On").with_name("bt");
        b.instance = Some("00:11".to_owned());
        map.insert("bt".to_owned(), b);
        let blocks = build_blocks(&tokens, &map, &cfg());
        assert_eq!(blocks[0].name.as_deref(), Some("bt"));
        assert_eq!(blocks[0].instance.as_deref(), Some("00:11"));
    }
}
