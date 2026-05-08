use super::{
    MOD_ALLOW_AMBIGUOUS_YOU, MOD_DROP_POSSESSIVE, MOD_EXTRACT_GROUP_MEMBER, MOD_FORCE_3RD_PERSON,
    MOD_FORCE_SINGULAR, MOD_NO_SMART, MOD_POSSESSIVE, MOD_PREFER_NOUN, TAG_ENTITY_CLOSE,
    TAG_ENTITY_OPEN, TAG_ESCAPE, TAG_PROPERTY_SEP, TAG_SEPARATOR, TAG_VERB_CLOSE, TAG_VERB_OPEN,
    TagFlags, Token, VERB_FORM_SEP, VERB_TENSE_SEP,
};

#[cold]
pub(crate) fn validation_error(message: &str, content: &str, open_char: char) -> String {
    let close_char = if open_char == TAG_ENTITY_OPEN {
        TAG_ENTITY_CLOSE
    } else {
        TAG_VERB_CLOSE
    };
    let formatted_message = format!("{message}: {open_char}{content}{close_char}");
    tracing::error!("{}", formatted_message);
    formatted_message
}

#[inline]
pub(crate) fn is_p_type(s: &str) -> bool {
    let (clean, _) = parse_stance_prefixes(s);
    let lower = clean.trim().to_lowercase();
    matches!(
        lower.as_str(),
        "subj" | "obj" | "poss" | "abs_poss" | "reflex"
    )
}

#[inline]
pub(crate) fn consume_until_closed(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    start_idx: usize,
    close_char: char,
    tag_type: &str,
) -> Result<usize, String> {
    let mut end_idx = start_idx + 1;
    let mut closed = false;
    let mut escaped = false;
    let mut in_quote: Option<char> = None;
    let mut prev_ch = '\0';
    while let Some(&(j, ch)) = chars.peek() {
        chars.next();
        if escaped {
            escaped = false;
            prev_ch = ch;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            prev_ch = ch;
            continue;
        }
        if let Some(q) = in_quote {
            if ch == q {
                in_quote = None;
            }
            prev_ch = ch;
            continue;
        }
        if ch == '\'' {
            if !prev_ch.is_alphanumeric() {
                in_quote = Some(ch);
            }
        } else if matches!(ch, '"' | '`') {
            in_quote = Some(ch);
        }
        if ch == close_char {
            end_idx = j;
            closed = true;
            break;
        }
        prev_ch = ch;
    }

    if !closed {
        tracing::error!("Unclosed {} starting at index {}", tag_type, start_idx);
        return Err(format!("Unclosed {tag_type} starting at index {start_idx}"));
    }

    Ok(end_idx)
}

#[inline]
pub(crate) fn consume_control_tag(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    start_idx: usize,
) -> Result<usize, String> {
    let mut closed = false;
    let mut escaped = false;
    let mut in_quote: Option<char> = None;
    let mut prev_ch = '\0';
    let mut end_idx = 0;

    while let Some(&(j, ch)) = chars.peek() {
        chars.next();
        if escaped {
            escaped = false;
            prev_ch = ch;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            prev_ch = ch;
            continue;
        }
        if let Some(q) = in_quote {
            if ch == q {
                in_quote = None;
            }
            prev_ch = ch;
            continue;
        }
        if ch == '\'' {
            if !prev_ch.is_alphanumeric() {
                in_quote = Some(ch);
            }
        } else if matches!(ch, '"' | '`') {
            in_quote = Some(ch);
        }
        if ch == '%'
            && let Some(&(_, '}')) = chars.peek()
        {
            end_idx = j;
            closed = true;
            break;
        }
        prev_ch = ch;
    }

    if !closed {
        tracing::error!("Unclosed control tag starting at index {}", start_idx);
        return Err(format!(
            "Unclosed control tag starting at index {start_idx}"
        ));
    }

    Ok(end_idx)
}

/// Parses prefix modifiers (like `+`) used to force perspectives, returning the
/// stripped string alongside the boolean flags for `force_3rd_person`/`force_article`, `no_smart`, and `force_singular`.
#[inline]
pub(crate) fn parse_stance_prefixes(mut s: &str) -> (&str, TagFlags) {
    let mut flags = TagFlags::empty();
    loop {
        s = s.trim_start();
        if let Some(stripped) = s.strip_prefix(MOD_FORCE_3RD_PERSON) {
            flags.insert(TagFlags::FORCE_3RD_PERSON);
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix(MOD_NO_SMART) {
            flags.insert(TagFlags::NO_SMART);
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix(MOD_FORCE_SINGULAR) {
            flags.insert(TagFlags::FORCE_SINGULAR);
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix(MOD_PREFER_NOUN) {
            flags.insert(TagFlags::PREFER_NOUN);
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix(MOD_ALLOW_AMBIGUOUS_YOU) {
            flags.insert(TagFlags::ALLOW_AMBIGUOUS_YOU);
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix(MOD_EXTRACT_GROUP_MEMBER) {
            flags.insert(TagFlags::EXTRACT_GROUP_MEMBER);
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix(MOD_DROP_POSSESSIVE) {
            flags.insert(TagFlags::DROP_POSSESSIVE);
            s = stripped;
        } else {
            break;
        }
    }
    (s, flags)
}

pub(crate) type ParsedForcedConjugations = (Option<Vec<String>>, Option<Vec<String>>);

#[inline]
pub(crate) fn parse_forced_conjugations(
    forced: &str,
    content: &str,
) -> Result<ParsedForcedConjugations, String> {
    let mut forced_present = None;
    let mut forced_past = None;

    let (pres_str, past_str) =
        if let Some((present_overrides, past_overrides)) = forced.split_once(VERB_TENSE_SEP) {
            (present_overrides, Some(past_overrides))
        } else {
            (forced, None)
        };

    if !pres_str.is_empty() {
        let parts: Vec<String> = pres_str
            .split(VERB_FORM_SEP)
            .map(|s| s.trim().to_string())
            .collect();
        for part in &parts {
            reject_if(
                part.is_empty(),
                "Verb tag has an empty forced present conjugation segment",
                content,
                TAG_VERB_OPEN,
            )?;
        }
        reject_if(
            parts.len() > 3,
            "Verb tag has too many forced present conjugation segments",
            content,
            TAG_VERB_OPEN,
        )?;
        forced_present = Some(parts);
    }

    if let Some(past_overrides_str) = past_str {
        reject_if(
            past_overrides_str.is_empty(),
            "Verb tag has an empty forced past conjugation segment",
            content,
            TAG_VERB_OPEN,
        )?;
        let parts: Vec<String> = past_overrides_str
            .split(VERB_FORM_SEP)
            .map(|s| s.trim().to_string())
            .collect();
        for part in &parts {
            reject_if(
                part.is_empty(),
                "Verb tag has an empty forced past conjugation segment",
                content,
                TAG_VERB_OPEN,
            )?;
        }
        reject_if(
            parts.len() > 3,
            "Verb tag has too many forced past conjugation segments",
            content,
            TAG_VERB_OPEN,
        )?;
        forced_past = Some(parts);
    }

    Ok((forced_present, forced_past))
}

#[inline]
pub(crate) fn split_tag<'a>(
    content: &'a str,
    open_char: char,
    malformed_msg: &str,
) -> Result<(&'a str, Option<&'a str>), String> {
    let mut parts = content.split(TAG_SEPARATOR);
    let p1 = parts.next().unwrap_or_default().trim();
    let p2 = parts.next().map(str::trim);
    reject_if(parts.next().is_some(), malformed_msg, content, open_char)?;
    Ok((p1, p2))
}

#[inline]
pub(crate) fn push_literal(tokens: &mut Vec<Token>, raw: &str, start: usize, end: usize) {
    if end > start
        && let Some(slice) = raw.get(start..end)
    {
        if let Some(Token::Literal(last)) = tokens.last_mut() {
            last.push_str(slice);
        } else {
            tokens.push(Token::Literal(slice.to_string()));
        }
    }
}

#[inline]
pub(crate) fn unescape_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, escaped)) = chars.next() {
                match escaped {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => process_unicode_escape(&mut chars, &mut out),
                    _ => out.push(escaped),
                }
            } else {
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[inline]
pub(crate) fn process_unicode_escape(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    out: &mut String,
) {
    if let Some(&(_, '{')) = chars.peek() {
        chars.next(); // Consume '{'
        let mut hex = String::new();
        while let Some(&(_, hc)) = chars.peek() {
            chars.next();
            if hc == '}' {
                break;
            }
            hex.push(hc);
        }
        if let Ok(code) = u32::from_str_radix(&hex, 16) {
            out.push(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER));
        } else {
            out.push(char::REPLACEMENT_CHARACTER);
        }
    } else {
        out.push('u');
    }
}

/// Safely strips exactly one layer of outer quotes if they match.
#[inline]
pub(crate) fn strip_outer_quotes(s: &str) -> &str {
    let s = s.trim();
    if let Some(first) = s.chars().next()
        && matches!(first, '"' | '\'' | '`')
        && s.ends_with(first)
        && s.len() >= 2
    {
        return &s[1..s.len() - 1];
    }
    s
}

/// Extracts a dynamic variable key and its optional fallback string (e.g., `key ?? "fallback"`).
#[inline]
pub(crate) fn extract_variable_fallback(s: &str) -> (&str, Option<String>) {
    if let Some((k, f)) = s.split_once("??") {
        (k.trim(), Some(unescape_string(strip_outer_quotes(f))))
    } else {
        (s.trim(), None)
    }
}

#[inline]
pub(crate) fn is_article(s: &str) -> bool {
    const ARTICLES: &[&str] = &[
        "a",
        "an",
        "the",
        "this",
        "that",
        "another",
        "one",
        "one of",
        "one of the",
        "some",
    ];
    ARTICLES.iter().any(|&art| s.eq_ignore_ascii_case(art))
}

#[inline]
pub(crate) fn is_indefinite_article(s: &str) -> bool {
    s.eq_ignore_ascii_case("a") || s.eq_ignore_ascii_case("an")
}

#[inline]
pub(crate) const fn is_escapable_char(c: char) -> bool {
    matches!(
        c,
        TAG_ENTITY_OPEN | TAG_VERB_OPEN | TAG_ENTITY_CLOSE | TAG_VERB_CLOSE | TAG_ESCAPE
    )
}

#[inline]
pub(crate) fn is_capitalized(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_uppercase)
}

#[inline]
pub(crate) fn is_all_caps(s: &str) -> bool {
    let has_letters = s.chars().any(char::is_alphabetic);
    has_letters
        && s.chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase)
}

#[inline]
pub(crate) fn reject_if(
    condition: bool,
    error_msg: &str,
    content: &str,
    open_char: char,
) -> Result<(), String> {
    if condition {
        Err(validation_error(error_msg, content, open_char))
    } else {
        Ok(())
    }
}

#[inline]
pub(crate) fn validate_property_segments(
    path: &str,
    error_msg: &str,
    content: &str,
    open_char: char,
) -> Result<(), String> {
    reject_if(
        path.split(TAG_PROPERTY_SEP).any(str::is_empty),
        error_msg,
        content,
        open_char,
    )
}

/// Parses possessive suffix `'s`, returning the stripped string and a boolean flag.
#[inline]
pub(crate) fn parse_possessive_suffix(s: &str) -> (&str, bool) {
    if let Some(stripped) = s.strip_suffix(MOD_POSSESSIVE) {
        (stripped, true)
    } else if let Some(stripped) = s.strip_suffix("'S") {
        (stripped, true)
    } else if let Some(stripped) = s.strip_suffix("’s") {
        (stripped, true)
    } else if let Some(stripped) = s.strip_suffix("’S") {
        (stripped, true)
    } else if let Some(stripped) = s.strip_suffix('\'') {
        (stripped, true)
    } else if let Some(stripped) = s.strip_suffix('’') {
        (stripped, true)
    } else {
        (s, false)
    }
}

/// Finds a possessive suffix with a trailing space, returning the index and the byte length of the match.
#[inline]
pub(crate) fn find_spaced_possessive(s: &str) -> Option<(usize, usize)> {
    if let Some(idx) = s.find("'s ") {
        Some((idx, 3))
    } else if let Some(idx) = s.find("'S ") {
        Some((idx, 3))
    } else if let Some(idx) = s.find("’s ") {
        Some((idx, 5))
    } else if let Some(idx) = s.find("’S ") {
        Some((idx, 5))
    } else if let Some(idx) = s.find("' ") {
        Some((idx, 2))
    } else {
        s.find("’ ").map(|idx| (idx, 4))
    }
}
