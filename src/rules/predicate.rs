//! Value predicates with cross-language normalized semantics.
//!
//! Four normalization points (spec §6): full match, no anchors in the
//! pattern, restricted syntax, own number grammar.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredicateError {
    ForbiddenSyntax(String),
    BadRegex(String),
}

#[derive(Debug, Clone)]
pub enum Predicate {
    Exists,
    Equals(String),
    Prefix(String),
    Regex(regex::Regex),
    NumericRange { min: f64, max: f64 },
}

impl Predicate {
    pub fn parse_regex(pattern: &str) -> Result<Predicate, PredicateError> {
        // The forbidden-syntax check runs on `pattern`, the raw,
        // not-yet-framed pattern. If it ran on the framed string
        // (`anchored`, see below), the framing itself could get
        // caught by the check -- that's why the framing happens only
        // at the very end of this function (see test
        // `forbidden_check_runs_on_the_raw_pattern`).
        if let Some(bad) = find_forbidden_syntax(pattern) {
            return Err(PredicateError::ForbiddenSyntax(bad));
        }
        // Full match: the pattern is framed, not the caller.
        let anchored = format!(r"\A(?:{pattern})\z");
        regex::Regex::new(&anchored)
            .map(Predicate::Regex)
            .map_err(|e| PredicateError::BadRegex(e.to_string()))
    }

    pub fn matches(&self, value: Option<&str>) -> bool {
        match self {
            Self::Exists => value.is_some(),
            Self::Equals(e) => value == Some(e.as_str()),
            Self::Prefix(p) => value.is_some_and(|v| v.starts_with(p.as_str())),
            Self::Regex(r) => value.is_some_and(|v| r.is_match(v)),
            Self::NumericRange { min, max } => value
                .and_then(parse_number)
                .is_some_and(|n| n >= *min && n <= *max),
        }
    }
}

/// Searches the raw pattern for forbidden regex syntax: anchors
/// (`^`/`$`) outside a character class, lookaround (`(?=`, `(?!`,
/// `(?<=`, `(?<!`), named groups in both spellings accepted by Rust's
/// `regex` crate (`(?<name>...)` AND `(?P<name>...)`), `\b` `\B`
/// `\p{...}` `\P{...}`, backreferences (`\1` etc.) and possessive
/// quantifiers in all four forms (`*+`, `++`, `?+`, `{n,m}+`).
///
/// Context-sensitive rather than a plain substring search -- the same
/// failure class as the original `^`/`$` check also affects these
/// constructs:
/// - `^` in `[^a]` (character-class negation) is not an anchor,
/// - `\*+`/`\++`/`\?+` (escaped literal quantifier character with a
///   simple, non-possessive `+` quantifier) is not possessive syntax,
/// - `\\b` (escaped backslash, followed by the literal letter `b`) is
///   not `\b` (word boundary).
///
/// A single sequential scan with two states carries this: "inside a
/// character class" (metacharacters are literals there) and "the most
/// recently visited token was a quantifier that just closed" (for
/// possessive detection: only a second `+` immediately following an
/// already-closed quantifier makes it possessive).
///
/// Limitation: nested character-class set operations
/// (`[a-z&&[^aeiou]]`, a Rust regex extension) are not recognized --
/// the narrow syntax subset from spec §6 point 3 doesn't provide for
/// them anyway.
fn find_forbidden_syntax(pattern: &str) -> Option<String> {
    let b = pattern.as_bytes();
    let mut i = 0;
    let mut in_class = false;
    // The most recently visited token outside a class was a quantifier
    // that just closed (*, +, ?, or a {n,m} interval) -- for possessive
    // detection: only a directly following second `+` turns it into a
    // possessive quantifier.
    let mut prev_was_quantifier_close = false;

    while i < b.len() {
        let c = b[i];

        if c == b'\\' {
            if i + 1 >= b.len() {
                // Truncated escape: invalid syntax, leave the error to
                // regex::Regex::new (BadRegex).
                return None;
            }
            let next = b[i + 1];
            match next {
                b'b' | b'B' | b'p' | b'P' => {
                    return Some(format!("\\{}", next as char));
                }
                // Class shorthands. Forbidden because they do NOT mean
                // the same thing in the two languages: this crate's
                // regex is in Unicode mode, so `\d` covers every
                // Unicode decimal digit, while Java's `\d` without
                // `UNICODE_CHARACTER_CLASS` is only `[0-9]`. Turning
                // that Java flag on gets close but not provably equal
                // (`\w` is defined over `Alpha` there against
                // `Alphabetic` here). Whoever means digits writes
                // `[0-9]`, and then both languages agree.
                b'd' | b'D' | b'w' | b'W' | b's' | b'S' => {
                    return Some(format!(
                        "\\{} (class shorthand, use an explicit character class such as [0-9])",
                        next as char
                    ));
                }
                d if d.is_ascii_digit() => {
                    return Some("backreference".to_string());
                }
                _ => {}
            }
            // Escaped literals (\^ \$ \* \+ \? \\ \[ \] \{ \} \. etc.)
            // as well as class shorthands (\w \d \s ...): neutralized,
            // not an anchor/quantifier character. A directly following
            // simple quantifier (e.g. `\*+` = one-or-more literal `*`)
            // is normal quantifier use, not a possessive double
            // quantifier.
            i += 2;
            prev_was_quantifier_close = false;
            continue;
        }

        // Character-class boundaries.
        if !in_class && c == b'[' {
            in_class = true;
            i += 1;
            if i < b.len() && b[i] == b'^' {
                i += 1; // Character-class negation, not an anchor.
            }
            if i < b.len() && b[i] == b']' {
                i += 1; // ']' in first position is a literal (POSIX).
            }
            prev_was_quantifier_close = false;
            continue;
        }
        if in_class && c == b']' {
            in_class = false;
            i += 1;
            prev_was_quantifier_close = false;
            continue;
        }
        if in_class {
            // Inside the class, metacharacters are literals.
            i += 1;
            prev_was_quantifier_close = false;
            continue;
        }

        // Group markers "(?...": handled before the generic quantifier
        // detection, so the '?' inside isn't itself read as a
        // quantifier character.
        if c == b'(' && i + 1 < b.len() && b[i + 1] == b'?' {
            if b[i..].starts_with(b"(?=") {
                return Some("(?=".to_string());
            }
            if b[i..].starts_with(b"(?!") {
                return Some("(?!".to_string());
            }
            if b[i..].starts_with(b"(?P<") {
                return Some("(?P<".to_string());
            }
            if b[i..].starts_with(b"(?<") {
                // covers both (?<name>...) (named group, short form)
                // and lookbehind (?<=...)/(?<!...).
                return Some("(?<".to_string());
            }
            // Allowed markers, e.g. "(?:" (non-capturing group) or
            // "(?i)"/"(?i:...)" (inline flags): consume "(?".
            i += 2;
            prev_was_quantifier_close = false;
            continue;
        }

        match c {
            b'^' | b'$' => return Some((c as char).to_string()),
            b'*' | b'+' | b'?' => {
                if prev_was_quantifier_close && c == b'+' {
                    return Some("possessive quantifier".to_string());
                }
                prev_was_quantifier_close = true;
            }
            b'{' => {
                // {n,m} interval: skip up to the closing '}'.
                if let Some(rel) = b[i..].iter().position(|&x| x == b'}') {
                    i += rel + 1;
                    prev_was_quantifier_close = true;
                    continue;
                }
                // No closing '}': invalid syntax, leave the error to
                // regex::Regex::new.
                prev_was_quantifier_close = false;
            }
            _ => {
                prev_was_quantifier_close = false;
            }
        }
        i += 1;
    }
    None
}

/// Own grammar: [+-]? digits [. digits] [eE [+-]? digits].
/// No hex, no suffixes, no inf, no NaN.
pub fn parse_number(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let d0 = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == d0 {
        return None;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let d1 = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == d1 {
            return None;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let d2 = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == d2 {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    s.parse::<f64>().ok()
}

/// Cross-language table: predicate, value, expectation.
///
/// Written by the `#[ignore]`d test below and read by BOTH languages.
/// A predicate that decides differently on either side shows up here
/// and nowhere else, because the table is the only place where the two
/// evaluations meet on identical input.
#[cfg(test)]
const PREDICATE_TABLE: &str = "../../seesaw-java/src/test/resources/fixtures/predicate_table.json";

#[cfg(test)]
fn predicate_table_json() -> String {
    use serde_json::json;
    // (declaration, value, expected). `null` as the value means: the
    // node carries no value at all.
    let accepted = [
        (json!({"kind": "exists"}), json!(""), true),
        (json!({"kind": "exists"}), json!("x"), true),
        (json!({"kind": "exists"}), json!(null), false),
        (json!({"kind": "equals", "value": "x"}), json!("x"), true),
        (json!({"kind": "equals", "value": "x"}), json!("y"), false),
        (json!({"kind": "equals", "value": "x"}), json!(null), false),
        (json!({"kind": "equals", "value": ""}), json!(""), true),
        (json!({"kind": "prefix", "value": "ab"}), json!("abc"), true),
        (json!({"kind": "prefix", "value": "ab"}), json!("ab"), true),
        (json!({"kind": "prefix", "value": "ab"}), json!("a"), false),
        (json!({"kind": "prefix", "value": ""}), json!("x"), true),
        (json!({"kind": "regex", "pattern": "ab"}), json!("ab"), true),
        (
            json!({"kind": "regex", "pattern": "ab"}),
            json!("xaby"),
            false,
        ),
        (json!({"kind": "regex", "pattern": "a|b"}), json!("a"), true),
        (
            json!({"kind": "regex", "pattern": "a|b"}),
            json!("ab"),
            false,
        ),
        (
            json!({"kind": "regex", "pattern": "[0-9]+"}),
            json!("123"),
            true,
        ),
        (
            json!({"kind": "regex", "pattern": "[0-9]+"}),
            json!("12a"),
            false,
        ),
        (
            json!({"kind": "regex", "pattern": "[^a]"}),
            json!("b"),
            true,
        ),
        (
            json!({"kind": "regex", "pattern": "[^a]"}),
            json!("a"),
            false,
        ),
        (
            json!({"kind": "regex", "pattern": "a\\^b"}),
            json!("a^b"),
            true,
        ),
        (
            json!({"kind": "regex", "pattern": "\\*+"}),
            json!("**"),
            true,
        ),
        (
            json!({"kind": "regex", "pattern": "\\\\b"}),
            json!("\\b"),
            true,
        ),
        (
            json!({"kind": "regex", "pattern": "a.c"}),
            json!("abc"),
            true,
        ),
        (
            json!({"kind": "regex", "pattern": "a.c"}),
            json!("a\nc"),
            false,
        ),
        (
            json!({"kind": "regex", "pattern": "(?:ab)+"}),
            json!("abab"),
            true,
        ),
        (
            json!({"kind": "regex", "pattern": "a{2,3}"}),
            json!("aa"),
            true,
        ),
        (
            json!({"kind": "regex", "pattern": "a{2,3}"}),
            json!("a"),
            false,
        ),
        (
            json!({"kind": "regex", "pattern": "ab"}),
            json!(null),
            false,
        ),
        (
            json!({"kind": "numeric_range", "min": 1.0, "max": 2.0}),
            json!("1"),
            true,
        ),
        (
            json!({"kind": "numeric_range", "min": 1.0, "max": 2.0}),
            json!("2"),
            true,
        ),
        (
            json!({"kind": "numeric_range", "min": 1.0, "max": 2.0}),
            json!("1.5"),
            true,
        ),
        (
            json!({"kind": "numeric_range", "min": 1.0, "max": 2.0}),
            json!("2.1"),
            false,
        ),
        (
            json!({"kind": "numeric_range", "min": -1.0, "max": 1.0}),
            json!("-1"),
            true,
        ),
        (
            json!({"kind": "numeric_range", "min": 0.0, "max": 2000.0}),
            json!("-1.5e3"),
            false,
        ),
        (
            json!({"kind": "numeric_range", "min": -2000.0, "max": 0.0}),
            json!("-1.5e3"),
            true,
        ),
        (
            json!({"kind": "numeric_range", "min": 0.0, "max": 9.0}),
            json!("1d"),
            false,
        ),
        (
            json!({"kind": "numeric_range", "min": 0.0, "max": 9.0}),
            json!("0x1p3"),
            false,
        ),
        (
            json!({"kind": "numeric_range", "min": 0.0, "max": 9.0}),
            json!("inf"),
            false,
        ),
        (
            json!({"kind": "numeric_range", "min": 0.0, "max": 9.0}),
            json!("NaN"),
            false,
        ),
        (
            json!({"kind": "numeric_range", "min": 0.0, "max": 9.0}),
            json!("Infinity"),
            false,
        ),
        (
            json!({"kind": "numeric_range", "min": 0.0, "max": 9.0}),
            json!(" 4"),
            false,
        ),
        (
            json!({"kind": "numeric_range", "min": 0.0, "max": 9.0}),
            json!(null),
            false,
        ),
    ];
    // Muster, die BEIDE Sprachen ablehnen muessen.
    let rejected = [
        "^ab", "ab$", "[a]^", "(?=a)", "(?!a)", "(?<=a)", "(?<!a)", "(?P<n>a)", "(?<n>a)",
        "(\\w)\\1", "\\bx", "\\Bx", "\\p{L}", "\\P{L}", "a*+", "a++", "a?+", "a{2,3}+", "\\d+",
        "\\D", "\\w", "\\W", "\\s", "\\S",
    ];
    let accepted: Vec<serde_json::Value> = accepted
        .iter()
        .map(|(d, v, e)| json!({"predicate": d, "value": v, "expected": e}))
        .collect();
    serde_json::to_string_pretty(&json!({
        "accepted": accepted,
        "rejected_patterns": rejected,
    }))
    .expect("table serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Schreibt die sprachuebergreifende Praedikat-Tabelle in den
    /// Java-Baum. Beide Sprachen fahren sie danach gegen dieselben
    /// Erwartungen.
    #[test]
    #[ignore = "manual: writes the cross-language predicate table"]
    fn write_predicate_table() {
        std::fs::write(PREDICATE_TABLE, predicate_table_json()).expect("table write attempt");
        eprintln!("predicate table written to {PREDICATE_TABLE}");
    }

    /// Die Rust-Seite gegen die Tabelle. Faellt hier etwas um, ist die
    /// Tabelle veraltet oder die Auswertung hat sich geaendert.
    #[test]
    fn predicate_table_holds_on_this_side() {
        let table: serde_json::Value =
            serde_json::from_str(&predicate_table_json()).expect("table parses");
        for case in table["accepted"].as_array().expect("accepted is an array") {
            let decl = &case["predicate"];
            let p = predicate_from_json(decl).expect("declaration is accepted");
            let value = case["value"].as_str();
            let expected = case["expected"].as_bool().expect("expected is a bool");
            assert_eq!(
                p.matches(value),
                expected,
                "{decl} against {:?}",
                case["value"]
            );
        }
        for pat in table["rejected_patterns"]
            .as_array()
            .expect("rejected is an array")
        {
            let pat = pat.as_str().expect("pattern is a string");
            assert!(
                Predicate::parse_regex(pat).is_err(),
                "{pat} must be rejected"
            );
        }
    }

    /// Eine Deklaration aus der Tabelle in ein Praedikat. Kleiner
    /// Umweg, weil `rules::format` sie sonst ueber serde liest.
    fn predicate_from_json(d: &serde_json::Value) -> Result<Predicate, PredicateError> {
        Ok(match d["kind"].as_str().expect("kind is a string") {
            "exists" => Predicate::Exists,
            "equals" => Predicate::Equals(d["value"].as_str().expect("value").to_string()),
            "prefix" => Predicate::Prefix(d["value"].as_str().expect("value").to_string()),
            "regex" => Predicate::parse_regex(d["pattern"].as_str().expect("pattern"))?,
            "numeric_range" => Predicate::NumericRange {
                min: d["min"].as_f64().expect("min"),
                max: d["max"].as_f64().expect("max"),
            },
            other => panic!("unknown predicate kind {other}"),
        })
    }

    #[test]
    fn regex_is_full_match() {
        let p = Predicate::parse_regex("ab").unwrap();
        assert!(p.matches(Some("ab")));
        assert!(!p.matches(Some("xaby")), "a partial match must not match");
    }

    #[test]
    fn regex_anchors_are_forbidden() {
        assert!(matches!(
            Predicate::parse_regex("^ab$"),
            Err(PredicateError::ForbiddenSyntax(_))
        ));
    }

    #[test]
    fn regex_lookaround_and_backreferences_are_forbidden() {
        for pat in ["(?=a)", "(?!a)", r"(\w)\1", r"\bx", r"\p{L}"] {
            assert!(
                matches!(
                    Predicate::parse_regex(pat),
                    Err(PredicateError::ForbiddenSyntax(_))
                ),
                "{pat} must be rejected"
            );
        }
    }

    #[test]
    fn number_grammar_is_narrow() {
        assert_eq!(parse_number("-1.5e3"), Some(-1500.0));
        assert_eq!(parse_number("42"), Some(42.0));
        assert_eq!(parse_number("1d"), None, "Java suffix is not allowed");
        assert_eq!(parse_number("0x1p3"), None, "hex float is not allowed");
        assert_eq!(parse_number("inf"), None);
        assert_eq!(parse_number("NaN"), None);
    }

    #[test]
    fn numeric_range_is_inclusive() {
        let p = Predicate::NumericRange { min: 1.0, max: 2.0 };
        assert!(p.matches(Some("1")));
        assert!(p.matches(Some("2")));
        assert!(!p.matches(Some("2.1")));
    }

    #[test]
    fn prefix_and_equals_and_exists() {
        assert!(Predicate::Prefix("cmd_".into()).matches(Some("cmd_go")));
        assert!(Predicate::Equals("x".into()).matches(Some("x")));
        assert!(Predicate::Exists.matches(Some("")));
        assert!(!Predicate::Exists.matches(None));
    }

    // Decision on the substring check for anchors (see FORBIDDEN
    // above): a plain `pattern.contains("^")` check would be too
    // coarse, because `[^a]` (negated character class) contains a `^`
    // but is allowed, ordinary syntax -- not anchor use. These tests
    // pin down the context-sensitive decision.
    #[test]
    fn negated_character_class_is_allowed() {
        let p = Predicate::parse_regex("[^a]").unwrap();
        assert!(p.matches(Some("b")));
        assert!(!p.matches(Some("a")));
    }

    #[test]
    fn real_anchor_stays_forbidden_even_next_to_a_character_class() {
        // "^" outside a class is still an anchor even when a class
        // occurs elsewhere in the same pattern.
        assert!(matches!(
            Predicate::parse_regex("[a]^"),
            Err(PredicateError::ForbiddenSyntax(_))
        ));
    }

    #[test]
    fn escaped_anchor_characters_are_allowed() {
        // `\^` and `\$` are literal characters, not anchors -- an
        // obvious side finding of the same context-sensitive check: a
        // plain substring search would have wrongly forbidden these
        // cases too.
        let p = Predicate::parse_regex(r"a\^b\$c").unwrap();
        assert!(p.matches(Some("a^b$c")));
    }

    #[test]
    fn named_groups_both_spellings_are_forbidden() {
        // Rust's regex crate has two spellings for named groups; both
        // are forbidden (spec §6.3: no named groups).
        for pat in [r"(?P<name>a)", r"(?<name>a)"] {
            assert!(
                matches!(
                    Predicate::parse_regex(pat),
                    Err(PredicateError::ForbiddenSyntax(_))
                ),
                "{pat} must be rejected"
            );
        }
    }

    #[test]
    fn possessive_quantifiers_all_four_forms_are_forbidden() {
        for pat in ["a*+", "a++", "a?+", "a{2,3}+"] {
            assert!(
                matches!(
                    Predicate::parse_regex(pat),
                    Err(PredicateError::ForbiddenSyntax(_))
                ),
                "{pat} must be rejected"
            );
        }
    }

    #[test]
    fn escaped_literals_before_a_simple_quantifier_are_allowed() {
        // \*+ \++ \?+ : escaped literal quantifier character, followed
        // by a simple (non-possessive) `+` quantifier. Not possessive
        // syntax -- the `+` quantifies the literal predecessor, not an
        // already-present quantifier.
        let p1 = Predicate::parse_regex(r"\*+").unwrap();
        assert!(p1.matches(Some("*")));
        assert!(p1.matches(Some("**")));
        assert!(!p1.matches(Some("")));

        let p2 = Predicate::parse_regex(r"\++").unwrap();
        assert!(p2.matches(Some("+")));

        let p3 = Predicate::parse_regex(r"\?+").unwrap();
        assert!(p3.matches(Some("?")));
    }

    #[test]
    fn forbidden_check_runs_on_the_raw_pattern() {
        // Direct evidence at the internal check function: it sees
        // exactly the raw, unframed pattern, never the full-match
        // framing `\A(?:...)\z`. For the current framing (with
        // `\A`/`\z`, no literal `^`/`$` characters) the result would
        // stay unchanged even if checked after framing -- the order is
        // still an invariant: it guards against a plausible future
        // change of the framing to `^(?:...)$`, which would otherwise
        // reject every pattern without exception.
        assert_eq!(find_forbidden_syntax("ab"), None);
        assert_eq!(find_forbidden_syntax("^ab$"), Some("^".to_string()));
    }

    /// The class shorthands are forbidden because they don't mean the
    /// same thing in both languages (see the reasoning at the scanner).
    /// The spelled-out form stays allowed and means the same on both
    /// sides.
    #[test]
    fn class_shorthands_are_forbidden() {
        for pat in [r"\d+", r"\D", r"\w", r"\W", r"\s", r"\S"] {
            assert!(
                matches!(
                    Predicate::parse_regex(pat),
                    Err(PredicateError::ForbiddenSyntax(_))
                ),
                "{pat} must be rejected"
            );
        }
        let p = Predicate::parse_regex("[0-9]+").expect("spelled out stays allowed");
        assert!(p.matches(Some("123")));
    }

    #[test]
    fn escaped_backslash_followed_by_b_is_allowed() {
        // Another finding of the same failure class (task item 3):
        // "\\b" in the raw pattern is an escaped backslash (literal
        // `\`), followed by a literal `b` -- not a `\b` word boundary.
        // The original substring search for "\\b" would have wrongly
        // forbidden this.
        let p = Predicate::parse_regex(r"\\b").unwrap();
        assert!(p.matches(Some(r"\b")));
    }
}
