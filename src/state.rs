//! Shared bar state: per-module blocks behind a `Mutex`, snapshotted by the render loop.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::config::Config;
use crate::icons::Icons;

/// One i3bar block. Serialized with `skip_serializing_if` so empty fields don't
/// clutter the line — sway ignores omitted fields and applies its own defaults.
#[derive(Clone, Debug, Serialize)]
pub struct Block {
    pub full_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub urgent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator_block_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markup: Option<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(b: &bool) -> bool {
    !*b
}

impl Block {
    /// A plain-text block with no styling.
    pub fn text(full_text: impl Into<String>) -> Self {
        Self {
            full_text: full_text.into(),
            short_text: None,
            color: None,
            background: None,
            name: None,
            instance: None,
            urgent: false,
            separator: None,
            separator_block_width: None,
            markup: None,
        }
    }

    /// A literal separator inserted between module blocks by the layout template.
    /// No native i3bar separator line (`separator:false`) and no gap
    /// (`separator_block_width:0`) — the text itself carries the spacing, so the
    /// template's separators reproduce the bash look exactly.
    pub fn separator(full_text: impl Into<String>) -> Self {
        let mut b = Self::text(full_text);
        b.separator = Some(false);
        b.separator_block_width = Some(0);
        b
    }

    /// `"<icon> <text>"` with a single space between when the icon is non-empty.
    pub fn icon_text(icon: &str, text: &str) -> Self {
        let full = if icon.is_empty() {
            text.to_owned()
        } else {
            format!("{icon} {text}")
        };
        Self::text(full)
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_owned());
        self
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub const fn with_urgent(mut self) -> Self {
        self.urgent = true;
        self
    }
}

/// The whole bar's current blocks, keyed by module name.
#[derive(Default)]
pub struct BarState {
    blocks: Mutex<HashMap<String, Block>>,
}

impl BarState {
    /// Publish a block. Modules call this whenever they have fresh data.
    /// Empty `full_text` is still stored so a block can intentionally clear itself.
    pub fn set(&self, name: &str, block: Block) {
        let mut map = self.blocks.lock().expect("bar state lock poisoned");
        map.insert(name.to_owned(), block);
    }

    /// Collect all non-empty blocks keyed by module name. The render loop walks
    /// the layout template and pulls each placeholder's block from this map;
    /// absent/empty modules simply aren't present here (anti-flicker: no
    /// flashing placeholders).
    pub fn snapshot_map(&self) -> HashMap<String, Block> {
        let map = self.blocks.lock().expect("bar state lock poisoned");
        map.iter()
            .filter(|(_, b)| !b.full_text.is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

/// Everything a module thread needs, cheaply shared via `Arc`.
#[derive(Clone)]
pub struct Ctx {
    pub config: Arc<Config>,
    pub icons: Arc<Icons>,
    pub state: Arc<BarState>,
}

impl Ctx {
    pub fn new(config: Config, icons: Icons, state: BarState) -> Self {
        Self {
            config: Arc::new(config),
            icons: Arc::new(icons),
            state: Arc::new(state),
        }
    }
}
