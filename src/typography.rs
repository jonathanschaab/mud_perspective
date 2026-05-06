use unicode_segmentation::UnicodeSegmentation;

pub(crate) const SENTENCE_BREAK_SENTINEL: char = '\u{E000}';
pub(crate) const NO_SENTENCE_BREAK_SENTINEL: char = '\u{E001}';

/// Segments the text by true sentence boundaries and capitalizes the first letter.
pub(crate) fn post_process_typography(input: &str) -> String {
    let mut output = String::with_capacity(input.len());

    let mut bounds = input.split_sentence_bound_indices();
    bounds.next(); // Skip the first bound (which is always 0)
    let mut next_sentence_start = bounds.next().map(|(i, _)| i);
    let mut capitalized = false;
    let mut last_real_char = None;
    let mut suppress_next_break = false;

    let mut catch_up_bounds = |target_idx: usize, next_start: &mut Option<usize>| -> bool {
        let mut advanced = false;
        while let Some(ns) = *next_start {
            if ns <= target_idx {
                *next_start = bounds.next().map(|(idx, _)| idx);
                advanced = true;
            } else {
                break;
            }
        }
        advanced
    };

    let mut chars = input.char_indices().peekable();

    #[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
    let has_tags = has_protocol_tags(input);

    while let Some(&(i, c)) = chars.peek() {
        if c == SENTENCE_BREAK_SENTINEL {
            chars.next();
            capitalized = false;
            continue;
        }
        if c == NO_SENTENCE_BREAK_SENTINEL {
            chars.next();
            suppress_next_break = true;
            continue;
        }

        // If we cross into a new sentence boundary, reset the capitalization flag
        if catch_up_bounds(i, &mut next_sentence_start) {
            if suppress_next_break {
                suppress_next_break = false;
            } else {
                capitalized = false;
            }
        }

        #[allow(unused_mut)]
        let mut skipped_tag = false;

        // 1, 2, & 3. Skip MXP Tags, MSP Triggers, and ANSI Escape Sequences
        #[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
        if has_tags {
            let remainder = input.get(i..).unwrap_or_default();
            if let Some(end_offset) = skip_protocol_tags(&mut chars, remainder, i) {
                if let Some(skipped) = remainder.get(..=end_offset) {
                    output.push_str(skipped);
                }
                skipped_tag = true;
            }
        }

        if skipped_tag {
            // Discard any sentence boundaries that fell inside the tag we just skipped.
            if let Some(&(curr_i, _)) = chars.peek() {
                catch_up_bounds(curr_i, &mut next_sentence_start);
            }

            if matches!(last_real_char, Some('.' | '!' | '?')) {
                if suppress_next_break {
                    suppress_next_break = false;
                } else {
                    capitalized = false;
                }
            }
            continue;
        }

        // 4. Normal Character Processing
        chars.next(); // Consume the character

        if !capitalized && c.is_alphabetic() {
            for uc in c.to_uppercase() {
                output.push(uc);
            }
            capitalized = true;
            last_real_char = Some(c);
        } else {
            output.push(c);
            if !c.is_whitespace() {
                last_real_char = Some(c);
            }
        }
    }

    output
}

/// Uppercases the string while preserving the casing of protocol tags.
pub(crate) fn apply_all_caps(input: &str, output: &mut String) {
    #[cfg(not(any(feature = "mxp", feature = "msp", feature = "ansi")))]
    {
        for c in input.chars() {
            output.extend(c.to_uppercase());
        }
    }

    #[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
    {
        if !has_protocol_tags(input) {
            for c in input.chars() {
                output.extend(c.to_uppercase());
            }
            return;
        }

        let mut chars = input.char_indices().peekable();

        while let Some(&(i, c)) = chars.peek() {
            let remainder = input.get(i..).unwrap_or_default();
            if let Some(end_offset) = skip_protocol_tags(&mut chars, remainder, i) {
                if let Some(skipped) = remainder.get(..=end_offset) {
                    output.push_str(skipped);
                }
                continue;
            }

            chars.next();
            output.extend(c.to_uppercase());
        }
    }
}

#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
#[inline]
pub(crate) fn advance_chars_until(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    target_i: usize,
) {
    while let Some(&(curr_i, _)) = chars.peek() {
        if curr_i <= target_i {
            chars.next();
        } else {
            break;
        }
    }
}

#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
#[inline]
pub(crate) fn skip_protocol_tags(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    remainder: &str,
    i: usize,
) -> Option<usize> {
    if let Some(end_offset) = find_skipped_tag_end(remainder) {
        advance_chars_until(chars, i + end_offset);
        Some(end_offset)
    } else {
        None
    }
}

pub(crate) fn strip_all_protocol_tags(input: &str) -> String {
    #[cfg(not(any(feature = "mxp", feature = "msp", feature = "ansi")))]
    {
        input.to_string()
    }
    #[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
    {
        if !has_protocol_tags(input) {
            return input.to_string();
        }

        let mut output = String::with_capacity(input.len());
        let mut chars = input.char_indices().peekable();

        while let Some(&(i, c)) = chars.peek() {
            let remainder = input.get(i..).unwrap_or_default();
            if skip_protocol_tags(&mut chars, remainder, i).is_some() {
                continue;
            }
            chars.next();
            output.push(c);
        }
        output
    }
}

#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
#[inline]
pub(crate) fn find_skipped_tag_end(remainder: &str) -> Option<usize> {
    #[cfg(feature = "mxp")]
    if remainder.starts_with('<') {
        return remainder.find('>');
    }

    #[cfg(feature = "msp")]
    if remainder.starts_with("!!SOUND(") || remainder.starts_with("!!MUSIC(") {
        return remainder.find(')');
    }

    #[cfg(feature = "ansi")]
    if remainder.starts_with('\x1b') {
        let mut chars = remainder.char_indices();
        chars.next(); // Skip \x1b
        if let Some((idx, next_c)) = chars.next() {
            match next_c {
                '[' => {
                    for (idx, csi_c) in chars {
                        if (0x40..=0x7E).contains(&(csi_c as u8)) {
                            return Some(idx);
                        }
                    }
                }
                ']' | 'P' | 'X' | '^' | '_' => {
                    let mut last_char = next_c;
                    for (idx, osc_c) in chars {
                        if osc_c == '\x07' || (last_char == '\x1b' && osc_c == '\\') {
                            return Some(idx);
                        }
                        last_char = osc_c;
                    }
                }
                '(' | ')' | '*' | '+' | '-' | '.' | '/' => {
                    if let Some((idx, _)) = chars.next() {
                        return Some(idx);
                    }
                }
                _ => return Some(idx), // Simple 2-character escape
            }
        }
        return None;
    }

    None
}

/// Performs a SIMD pre-scan to detect the presence of protocol triggers.
#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
#[inline]
pub(crate) fn has_protocol_tags(input: &str) -> bool {
    let mut has_tags = false;
    #[cfg(feature = "ansi")]
    {
        has_tags |= input.contains('\x1b');
    }
    #[cfg(feature = "mxp")]
    {
        has_tags |= input.contains('<');
    }
    #[cfg(feature = "msp")]
    {
        has_tags |= input.contains("!!");
    }
    has_tags
}
