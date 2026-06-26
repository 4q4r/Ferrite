//! Date/time block via the `time` crate. The format string is parsed once at
//! thread start and reused — formatting each second is allocation-only.
//!
//! The `time` crate ships English-only month names. For `locale = "ru"` the
//! formatted string is post-processed to swap the English long month name for
//! its Russian genitive form, so the bar reads "26 июня", not "26 June".

use std::thread::{self, JoinHandle};
use std::time::Duration;

use time::OffsetDateTime;
use time::format_description::{FormatDescriptionV3, parse_owned};

use crate::state::{Block, Ctx};

pub fn spawn(ctx: Ctx) -> JoinHandle<()> {
    let interval = Duration::from_millis(ctx.config.modules.date.interval_ms.max(50));
    let fmt_src = ctx.config.modules.date.format.clone();
    let locale = ctx.config.modules.date.locale.clone();
    thread::spawn(move || {
        // Parse the format once (owned, 'static — version 3). On parse error,
        // log and fall back to a sane default.
        let items = match parse_owned::<3>(&fmt_src) {
            Ok(items) => items,
            Err(e) => {
                eprintln!("ferrite: date format {fmt_src:?} invalid ({e}); using [hour]:[minute]");
                parse_owned::<3>("[hour]:[minute]").expect("static format parses")
            }
        };
        loop {
            if let Some(b) = produce(&items, &locale) {
                ctx.state.set("date", b);
            }
            thread::sleep(interval);
        }
    })
}

fn produce(items: &FormatDescriptionV3<'static>, locale: &str) -> Option<Block> {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let text = now.format(items).ok()?;
    let text = if locale == "ru" {
        localize_months(text)
    } else {
        text
    };
    Some(Block::text(text))
}

/// Replace the single English long month name emitted by `time` with its
/// Russian genitive form. No English month name is a substring of another, so a
/// plain `replace` is unambiguous; the `contains` guard avoids 11 throwaway
/// allocations per second when only one month can match.
fn localize_months(s: String) -> String {
    const EN: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const RU: [&str; 12] = [
        "января",
        "февраля",
        "марта",
        "апреля",
        "мая",
        "июня",
        "июля",
        "августа",
        "сентября",
        "октября",
        "ноября",
        "декабря",
    ];
    let mut out = s;
    for (en, ru) in EN.iter().zip(RU.iter()) {
        if out.contains(*en) {
            out = out.replace(*en, ru);
        }
    }
    out
}
