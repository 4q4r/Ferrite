//! Layout template: the bar is a single string of `{module}` placeholders
//! interleaved with arbitrary literal text. The literal text between two
//! placeholders is the separator — so the user can place as many *different*
//! separators in one line as they like (`" | "`, `" "`, `" · "`, even `""`).
//!
//! Example: `"{lang} | {cpu} {mem} {temp} {bat} | {net} {bt} {vol} {bri} | {date}"`
//! renders as `lang | cpu mem temp bat | net bt vol bri | date` — the group
//! separator is `" | "` and the within-group separator is `" "`.

/// One parsed piece of the layout: a module reference or a literal separator.
#[derive(Clone, Debug)]
pub enum Token {
    /// Literal text between (or around) module placeholders — rendered verbatim.
    Sep(String),
    /// `{name}` — substituted with the named module's block when present.
    Module(String),
}

/// Parse a layout template into tokens. `{name}` becomes `Module(name)`; any
/// text between placeholders becomes `Sep(text)`. An unterminated `{` is treated
/// as literal text rather than swallowing the rest of the string, and an empty
/// `{}` is dropped so stray braces don't ghost a module. Parsing is char-based
/// so non-ASCII separators (`│`, `·`, …) are handled correctly.
pub fn parse(layout: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut lit = String::new();
    let mut chars = layout.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            let mut closed = false;
            for nc in chars.by_ref() {
                if nc == '}' {
                    closed = true;
                    break;
                }
                name.push(nc);
            }
            if closed {
                let name = name.trim().to_owned();
                if !name.is_empty() {
                    if !lit.is_empty() {
                        tokens.push(Token::Sep(std::mem::take(&mut lit)));
                    }
                    tokens.push(Token::Module(name));
                }
                // empty `{}` is dropped without breaking the surrounding literal,
                // so `a{}b` parses to a single `Sep("ab")`.
            } else {
                // No closing brace: treat '{' + collected as literal text.
                lit.push('{');
                lit.push_str(&name);
            }
        } else {
            lit.push(c);
        }
    }
    if !lit.is_empty() {
        tokens.push(Token::Sep(lit));
    }
    tokens
}

/// Module names referenced by the template, in first-occurrence order, deduped.
/// The render loop spawns exactly these module threads.
pub fn module_names(tokens: &[Token]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for token in tokens {
        if let Token::Module(name) = token
            && seen.insert(name.clone())
        {
            out.push(name.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_placeholders_and_separators() {
        let toks = parse("{lang} | {cpu} {mem}");
        assert!(matches!(&toks[0], Token::Module(n) if n == "lang"));
        assert!(matches!(&toks[1], Token::Sep(s) if s == " | "));
        assert!(matches!(&toks[2], Token::Module(n) if n == "cpu"));
        assert!(matches!(&toks[3], Token::Sep(s) if s == " "));
        assert!(matches!(&toks[4], Token::Module(n) if n == "mem"));
    }

    #[test]
    fn empty_braces_dropped() {
        let toks = parse("a{}b");
        assert_eq!(toks.len(), 1);
        assert!(matches!(&toks[0], Token::Sep(s) if s == "ab"));
    }

    #[test]
    fn unterminated_brace_is_literal() {
        let toks = parse("{lang} {oops");
        assert_eq!(toks.len(), 2);
        assert!(matches!(&toks[0], Token::Module(n) if n == "lang"));
        assert!(matches!(&toks[1], Token::Sep(s) if s == " {oops"));
    }

    #[test]
    fn module_names_dedupe_in_order() {
        let toks = parse("{a} {b} {a} {c}");
        assert_eq!(
            module_names(&toks),
            vec!["a".to_owned(), "b".into(), "c".into()]
        );
    }

    #[test]
    fn unicode_separator_preserved() {
        let toks = parse("{cpu} · {mem}");
        assert!(matches!(&toks[1], Token::Sep(s) if s == " · "));
    }
}
