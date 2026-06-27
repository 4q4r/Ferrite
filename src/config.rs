//! Configuration: serde model, default seeding into `~/.config/ferrite/`, and loading.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Embedded default — also written to disk on first run and dumped by `--print-config`.
pub const DEFAULT: &str = include_str!("../assets/default-config.toml");

type AnyError = Box<dyn std::error::Error + Send + Sync>;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Output protocol: i3bar JSON (colors/clicks/pango) or plain text (like the old bash).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    I3bar,
    Plain,
}

/// Per-block markup: none or pango (sway supports pango).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Markup {
    #[serde(rename = "none")]
    None,
    #[default]
    #[serde(rename = "pango")]
    Pango,
}

/// Bluetooth backend: event-driven D-Bus (0 forks) or throttled `bluetoothctl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BtBackend {
    #[default]
    Dbus,
    Bluetoothctl,
}

/// Wireless signal presentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalFmt {
    #[default]
    Percent,
    Dbm,
}

/// Which kinds of network interface the `net` block shows. Selectable in config
/// as `interfaces = ["wifi"]`, `["lan"]`, or `["wifi", "lan"]` (the default —
/// show every connected physical link, not just the default-route one).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetKind {
    Wifi,
    Lan,
}

/// Memory block content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemFormat {
    #[default]
    Used,
    #[serde(rename = "used_total")]
    UsedTotal,
}

/// Color palette source: literal `[colors]` values, or pywal's
/// `~/.cache/wal/colors.json` (with explicit `[colors]` keys overriding pywal).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorSource {
    #[default]
    Static,
    Pywal,
}

/// Battery time-remaining presentation. `Auto` shows `2h15` / `45m`; `HM` always
/// shows `H:MM` (e.g. `3:20`, `0:45`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeFmt {
    #[default]
    Auto,
    #[serde(rename = "h:m")]
    HM,
}

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// `click_actions` = `{ "<module>" = { "<button>" = "<shell cmd>" } }`. Button
/// keys are strings so the `*` wildcard (any unlisted button) works without a
/// separate int-keyed table. No `deny_unknown_fields` — module names are
/// arbitrary and serde collects them as entries.
pub type ClickActionsConfig =
    std::collections::HashMap<String, std::collections::HashMap<String, String>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub bar: BarConfig,
    #[serde(default)]
    pub icons: IconsConfig,
    #[serde(default)]
    pub colors: ColorsConfig,
    #[serde(default)]
    pub click_actions: ClickActionsConfig,
    #[serde(default)]
    pub modules: ModulesConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarConfig {
    #[serde(default = "default_render_interval")]
    pub render_interval_ms: u64,
    #[serde(default)]
    pub protocol: Protocol,
    #[serde(default)]
    pub markup: Markup,
    /// Layout template: `{module}` placeholders interleaved with arbitrary
    /// literal text. The literal text between placeholders is the separator,
    /// so any number of different separators can appear in one line. Defaults
    /// to the bash layout: groups separated by `" | "`, items within a group
    /// by `" "`.
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default = "default_color")]
    pub default_color: String,
    #[serde(default = "default_sbw")]
    pub separator_block_width: u32,
    #[serde(default = "default_click_events")]
    pub click_events: bool,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            render_interval_ms: default_render_interval(),
            protocol: Protocol::default(),
            markup: Markup::default(),
            layout: default_layout(),
            default_color: default_color(),
            separator_block_width: default_sbw(),
            click_events: default_click_events(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IconsConfig {
    #[serde(default = "default_pack")]
    pub pack: String,
    #[serde(default)]
    pub overrides: std::collections::HashMap<String, String>,
}

impl Default for IconsConfig {
    fn default() -> Self {
        Self {
            pack: default_pack(),
            overrides: std::collections::HashMap::new(),
        }
    }
}

/// Bar color palette. Fields are `Option` so `colors::resolve` can tell an
/// explicit TOML value (which wins) from an unset one (filled from pywal or the
/// static default). After `resolve`, every field is `Some` and the accessors
/// never return the fallback — it is only a safety net for unresolved configs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorsConfig {
    #[serde(default)]
    pub source: ColorSource,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub urgent: Option<String>,
    #[serde(default)]
    pub warn: Option<String>,
    #[serde(default)]
    pub good: Option<String>,
    #[serde(default)]
    pub mute: Option<String>,
}

impl ColorsConfig {
    pub fn default_color(&self) -> &str {
        self.default.as_deref().unwrap_or("#cdd6f4")
    }
    pub fn urgent(&self) -> &str {
        self.urgent.as_deref().unwrap_or("#f38ba8")
    }
    pub fn warn(&self) -> &str {
        self.warn.as_deref().unwrap_or("#f9e2af")
    }
    /// Reserved palette slot (no module uses it yet) — exposed so `[colors].good`
    /// stays a documented, TOML-settable knob instead of a magic constant.
    #[allow(dead_code)]
    pub fn good(&self) -> &str {
        self.good.as_deref().unwrap_or("#a6e3a1")
    }
    pub fn mute(&self) -> &str {
        self.mute.as_deref().unwrap_or("#fab387")
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModulesConfig {
    #[serde(default)]
    pub lang: LangConfig,
    #[serde(default)]
    pub net: NetConfig,
    #[serde(default)]
    pub bluetooth: BluetoothConfig,
    #[serde(default)]
    pub volume: VolumeConfig,
    #[serde(default)]
    pub brightness: BrightnessConfig,
    #[serde(default)]
    pub cpu: CpuConfig,
    #[serde(default)]
    pub mem: MemConfig,
    #[serde(default)]
    pub temp: TempConfig,
    #[serde(default)]
    pub battery: BatteryConfig,
    #[serde(default)]
    pub disk: DiskConfig,
    #[serde(default)]
    pub load: LoadConfig,
    #[serde(default)]
    pub date: DateConfig,
    #[serde(default)]
    pub packages: PackagesConfig,
    #[serde(default)]
    pub vpn: VpnConfig,
    #[serde(default)]
    pub mpris: MprisConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LangConfig {
    #[serde(default = "default_lang_shorten")]
    pub shorten: usize,
    #[serde(default = "default_lang_fallback")]
    pub fallback: String,
    /// Sway fires **no** Input event when the layout changes via xkb options
    /// (`grp:*toggle`) or `xkb_switch_pattern` — the active layout lives inside
    /// the compositor and is only readable through `get_inputs`. So the block
    /// re-queries sway IPC on this interval. 0-CPU between ticks is impossible
    /// here (no event source exists); raise the interval to trade freshness
    /// for fewer IPC round-trips. Clamped to ≥250 ms by the module.
    #[serde(default = "default_lang_interval")]
    pub interval_ms: u64,
}

impl Default for LangConfig {
    fn default() -> Self {
        Self {
            shorten: default_lang_shorten(),
            fallback: default_lang_fallback(),
            interval_ms: default_lang_interval(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetConfig {
    #[serde(default = "default_net_interval")]
    pub interval_ms: u64,
    #[serde(default)]
    pub signal: SignalFmt,
    #[serde(default = "default_net_interfaces")]
    pub interfaces: Vec<NetKind>,
    #[serde(default = "default_true")]
    pub show_ip: bool,
    #[serde(default = "default_true")]
    pub show_rate: bool,
    #[serde(default = "default_rate_threshold")]
    pub rate_threshold_kb: u64,
}
impl Default for NetConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_net_interval(),
            signal: SignalFmt::Percent,
            interfaces: default_net_interfaces(),
            show_ip: true,
            show_rate: true,
            rate_threshold_kb: default_rate_threshold(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BluetoothConfig {
    #[serde(default = "default_bt_interval")]
    pub interval_ms: u64,
    #[serde(default)]
    pub backend: BtBackend,
}
impl Default for BluetoothConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_bt_interval(),
            backend: BtBackend::Dbus,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolumeConfig {
    #[serde(default = "default_vol_interval")]
    pub interval_ms: u64,
    #[serde(default = "default_vol_timeout")]
    pub timeout_ms: u64,
}
impl Default for VolumeConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_vol_interval(),
            timeout_ms: default_vol_timeout(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightnessConfig {
    #[serde(default = "default_bri_interval")]
    pub interval_ms: u64,
    #[serde(default = "default_bri_device")]
    pub device: String,
}
impl Default for BrightnessConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_bri_interval(),
            device: default_bri_device(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpuConfig {
    #[serde(default = "default_cpu_interval")]
    pub interval_ms: u64,
    #[serde(default)]
    pub icon: String,
}
impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_cpu_interval(),
            icon: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemConfig {
    #[serde(default = "default_mem_interval")]
    pub interval_ms: u64,
    #[serde(default)]
    pub format: MemFormat,
    #[serde(default)]
    pub icon: String,
}
impl Default for MemConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_mem_interval(),
            format: MemFormat::Used,
            icon: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TempConfig {
    #[serde(default = "default_temp_interval")]
    pub interval_ms: u64,
    #[serde(default = "default_temp_zone")]
    pub zone: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default = "default_temp_critical")]
    pub critical: i64,
}
impl Default for TempConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_temp_interval(),
            zone: default_temp_zone(),
            icon: String::new(),
            critical: default_temp_critical(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatteryConfig {
    #[serde(default = "default_bat_interval")]
    pub interval_ms: u64,
    #[serde(default = "default_bat_device")]
    pub device: String,
    /// Append an estimated time-remaining (`79% 3:20`) computed zero-fork from
    /// `power_now`/`energy_now` (or `current_now`/`charge_now`).
    #[serde(default)]
    pub show_time: bool,
    #[serde(default)]
    pub time_format: TimeFmt,
    /// Warn/crit battery percentages (discharging only) — color the block
    /// `colors.warn` / `colors.urgent` instead of the old hard-coded ≤15.
    #[serde(default = "default_bat_warn")]
    pub warn: i64,
    #[serde(default = "default_bat_crit")]
    pub crit: i64,
}
impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_bat_interval(),
            device: default_bat_device(),
            show_time: false,
            time_format: TimeFmt::Auto,
            warn: default_bat_warn(),
            crit: default_bat_crit(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiskConfig {
    #[serde(default = "default_disk_interval")]
    pub interval_ms: u64,
    #[serde(default = "default_mounts")]
    pub mounts: Vec<String>,
}
impl Default for DiskConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_disk_interval(),
            mounts: default_mounts(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadConfig {
    #[serde(default = "default_load_interval")]
    pub interval_ms: u64,
}
impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_load_interval(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DateConfig {
    #[serde(default = "default_date_interval")]
    pub interval_ms: u64,
    #[serde(default = "default_date_format")]
    pub format: String,
    /// Locale code for localized tokens (only month names so far). "en" leaves
    /// the `time` crate's English output untouched; "ru" replaces long month
    /// names with Russian genitive forms ("26 июня").
    #[serde(default = "default_date_locale")]
    pub locale: String,
}
impl Default for DateConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_date_interval(),
            format: default_date_format(),
            locale: default_date_locale(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagesConfig {
    #[serde(default = "default_packages_interval")]
    pub interval_ms: u64,
    #[serde(default = "default_packages_timeout")]
    pub timeout_ms: u64,
    /// External command whose stdout lines are counted as pending updates.
    #[serde(default = "default_packages_command")]
    pub command: String,
    /// Hide the block entirely when there are zero pending updates.
    #[serde(default = "default_true")]
    pub hide_when_zero: bool,
}
impl Default for PackagesConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_packages_interval(),
            timeout_ms: default_packages_timeout(),
            command: default_packages_command(),
            hide_when_zero: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VpnConfig {
    #[serde(default = "default_vpn_interval")]
    pub interval_ms: u64,
    /// `/sys/class/net` name globs treated as VPN links (operstate == "up" → on).
    #[serde(default = "default_vpn_patterns")]
    pub patterns: Vec<String>,
    /// Show an "Off" block when no VPN is up (default: hide when off).
    #[serde(default)]
    pub show_when_off: bool,
}
impl Default for VpnConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_vpn_interval(),
            patterns: default_vpn_patterns(),
            show_when_off: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MprisConfig {
    /// Truncate `artist - title` to this many visible characters (0 = no limit).
    #[serde(default = "default_mpris_max_len")]
    pub max_len: usize,
    /// `format` placeholders: `{artist}`, `{title}`.
    #[serde(default = "default_mpris_format")]
    pub format: String,
    /// Show an icon-only/idle block when no player is playing.
    #[serde(default)]
    pub show_when_stopped: bool,
}
impl Default for MprisConfig {
    fn default() -> Self {
        Self {
            max_len: default_mpris_max_len(),
            format: default_mpris_format(),
            show_when_stopped: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const fn default_true() -> bool {
    true
}
const fn default_render_interval() -> u64 {
    250
}
fn default_layout() -> String {
    "{lang} | {cpu} {mem} {temp} {bat} | {net} {bt} {vol} {bri} | {date}".to_owned()
}
fn default_color() -> String {
    "#cdd6f4".to_owned()
}
const fn default_sbw() -> u32 {
    0
}
const fn default_click_events() -> bool {
    true
}
fn default_pack() -> String {
    "nerd".to_owned()
}
const fn default_lang_shorten() -> usize {
    2
}
fn default_lang_fallback() -> String {
    "KB".to_owned()
}
const fn default_lang_interval() -> u64 {
    1000
}
const fn default_net_interval() -> u64 {
    3000
}
fn default_net_interfaces() -> Vec<NetKind> {
    vec![NetKind::Wifi, NetKind::Lan]
}
const fn default_rate_threshold() -> u64 {
    1
}
const fn default_bt_interval() -> u64 {
    5000
}
const fn default_vol_interval() -> u64 {
    2000
}
const fn default_vol_timeout() -> u64 {
    800
}
const fn default_bri_interval() -> u64 {
    2000
}
fn default_bri_device() -> String {
    "auto".to_owned()
}
const fn default_cpu_interval() -> u64 {
    1000
}
const fn default_mem_interval() -> u64 {
    2000
}
const fn default_temp_interval() -> u64 {
    2000
}
fn default_temp_zone() -> String {
    "auto".to_owned()
}
const fn default_temp_critical() -> i64 {
    75
}
const fn default_bat_interval() -> u64 {
    5000
}
fn default_bat_device() -> String {
    "auto".to_owned()
}
const fn default_disk_interval() -> u64 {
    10000
}
fn default_mounts() -> Vec<String> {
    vec!["/".into(), "/home".into()]
}
const fn default_load_interval() -> u64 {
    2000
}
const fn default_date_interval() -> u64 {
    1000
}
fn default_date_format() -> String {
    "[hour]:[minute] | [day] [month repr:long]".to_owned()
}
fn default_date_locale() -> String {
    "en".to_owned()
}
const fn default_packages_interval() -> u64 {
    1_800_000
}
const fn default_packages_timeout() -> u64 {
    60_000
}
fn default_packages_command() -> String {
    "checkupdates".to_owned()
}
const fn default_vpn_interval() -> u64 {
    2000
}
fn default_vpn_patterns() -> Vec<String> {
    vec!["zt*".into(), "wg*".into(), "tun*".into(), "tap*".into()]
}
const fn default_mpris_max_len() -> usize {
    24
}
fn default_mpris_format() -> String {
    "{artist} - {title}".to_owned()
}
const fn default_bat_warn() -> i64 {
    30
}
const fn default_bat_crit() -> i64 {
    15
}

// ---------------------------------------------------------------------------
// Paths & loading
// ---------------------------------------------------------------------------

/// `~/.config/ferrite` (honoring `$XDG_CONFIG_HOME`).
pub fn config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("ferrite");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    PathBuf::from(home).join(".config").join("ferrite")
}

/// Ensure `~/.config/ferrite/config.toml` exists (writing the default) and that
/// `packs/` exists. Returns the path to load.
fn ensure_config() -> Result<PathBuf, AnyError> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let packs = dir.join("packs");
    fs::create_dir_all(&packs)?;
    let example = packs.join("example.toml");
    if !example.exists() {
        fs::write(&example, include_str!("../assets/packs/example.toml"))?;
    }
    let cfg = dir.join("config.toml");
    if !cfg.exists() {
        fs::write(&cfg, DEFAULT)?;
    }
    Ok(cfg)
}

/// Load config from `path` (must exist); used by `--config <path>`.
pub fn load_from(path: &str) -> Result<Config, AnyError> {
    let text = fs::read_to_string(path).map_err(|e| format!("reading config {path}: {e}"))?;
    let cfg: Config = toml::from_str(&text).map_err(|e| format!("parsing config {path}: {e}"))?;
    Ok(cfg)
}

/// Load (or seed then load) the user config in `~/.config/ferrite/config.toml`.
pub fn load_or_init() -> Result<Config, AnyError> {
    let path = ensure_config()?;
    load_from(path.to_str().expect("config path is utf-8"))
}
