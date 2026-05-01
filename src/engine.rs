use crate::grammar::{conjugate_verb, resolve_article, resolve_pronoun};
use crate::models::RenderContext;
use unicode_segmentation::UnicodeSegmentation;

/// Represents a parsed unit of a template string.
#[derive(Debug)]
pub enum Token {
    /// Plain text that is inserted exactly as-is.
    Literal(String),
    /// e.g., {source}, {a:source}, or {the:target}
    EntityRef {
        /// The key of the entity in the `RenderContext`.
        key: String,
        /// An optional article to precede the entity name (e.g., "a", "an", "the").
        article: Option<String>,
        /// A flag indicating if the entity key was capitalized (e.g. {Source}).
        is_capitalized: bool,
    },
    /// e.g., {source:poss}
    PronounRef {
        /// The key of the entity in the `RenderContext`.
        key: String,
        /// The type of pronoun requested (e.g., "subj", "obj", "poss", "abs_poss", "reflex").
        p_type: String,
        /// A flag indicating if the pronoun type was capitalized (e.g. {source:Subj}).
        is_capitalized: bool,
    },
    /// e.g., [source:pulse]
    VerbRef {
        /// The optional subject key to bind the verb to for correct conjugation.
        subject_key: Option<String>,
        /// The original, un-processed form of the verb from the template.
        original_verb: String,
        /// The lowercased form of the verb, for dictionary lookups.
        lower_verb: String,
        /// A pre-calculated flag indicating if the original verb was capitalized.
        is_capitalized: bool,
    },
}

/// The compiled Abstract Syntax Tree (AST) representation of a raw template string.
///
/// `Template` owns its string data. This incurs a one-time allocation cost during compilation,
/// but allows the AST to be fully decoupled from the lifetime of the original input string,
/// making it ideal for caching dynamically loaded database content.
#[derive(Debug)]
pub struct Template {
    /// The sequence of tokens that make up this compiled template.
    pub tokens: Vec<Token>,
    /// A heuristic estimation of the rendered string's length, used for buffer pre-allocation.
    pub estimated_length: usize,
}

impl Template {
    /// Compiles a raw text string into a `Template` AST.
    ///
    /// Compiling templates ahead-of-time ensures that the parsing overhead is
    /// isolated from the heavily trafficked rendering loop.
    ///
    /// # Arguments
    /// * `raw` - The raw template string containing markup tags.
    ///
    /// # Errors
    /// Returns a `String` describing the syntax error if the template is malformed.
    pub fn compile(raw: &str) -> Result<Self, String> {
        let mut tokens = Vec::new();
        let mut chars = raw.char_indices().peekable();
        let mut last_literal_start = 0;

        while let Some(&(i, c)) = chars.peek() {
            #[allow(unused_variables)]
            let remainder = &raw[i..];

            #[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
            if let Some(end_offset) = find_skipped_tag_end(remainder) {
                advance_chars_until(&mut chars, i + end_offset);
                continue;
            }

            if c == '\\' {
                chars.next();
                if let Some(&(next_i, next_c)) = chars.peek()
                    && (next_c == '{'
                        || next_c == '['
                        || next_c == '}'
                        || next_c == ']'
                        || next_c == '\\')
                {
                    if i > last_literal_start {
                        tokens.push(Token::Literal(raw[last_literal_start..i].to_string()));
                    }
                    last_literal_start = next_i;
                    chars.next();
                }
                continue;
            }

            if c == '{' || c == '[' {
                // Push any preceding literal text
                if i > last_literal_start {
                    tokens.push(Token::Literal(raw[last_literal_start..i].to_string()));
                }
                chars.next(); // Consume the opening brace or bracket

                let is_entity = c == '{';
                let close_char = if is_entity { '}' } else { ']' };
                let tag_name = if is_entity { "entity tag" } else { "verb tag" };

                let end_idx = consume_until_closed(&mut chars, i, close_char, tag_name)?;

                let content = &raw[i + 1..end_idx];
                let mut parts = content.split(':');
                let p1 = parts.next().unwrap();

                if is_entity {
                    if let Some(p2) = parts.next() {
                        if parts.next().is_some() {
                            return Err(validation_error("Malformed entity tag", content, '{'));
                        }

                        // 2-part case
                        if p1.eq_ignore_ascii_case("a")
                            || p1.eq_ignore_ascii_case("an")
                            || p1.eq_ignore_ascii_case("the")
                        {
                            if p2.is_empty() {
                                return Err(validation_error(
                                    "Entity tag has an article but an empty key",
                                    content,
                                    '{',
                                ));
                            }
                            tokens.push(Token::EntityRef {
                                key: p2.to_lowercase(),
                                article: Some(p1.to_string()),
                                is_capitalized: p2.chars().next().is_some_and(|c| c.is_uppercase()),
                            });
                        } else {
                            if p1.is_empty() || p2.is_empty() {
                                return Err(validation_error(
                                    "Pronoun tag has an empty key or type",
                                    content,
                                    '{',
                                ));
                            }
                            tokens.push(Token::PronounRef {
                                key: p1.to_lowercase(),
                                p_type: p2.to_lowercase(),
                                is_capitalized: p2.chars().next().is_some_and(|c| c.is_uppercase()),
                            });
                        }
                    } else {
                        // 1-part case
                        if p1.is_empty() {
                            return Err(validation_error(
                                "Entity tag has an empty key",
                                content,
                                '{',
                            ));
                        }
                        tokens.push(Token::EntityRef {
                            key: p1.to_lowercase(),
                            article: None,
                            is_capitalized: p1.chars().next().is_some_and(|c| c.is_uppercase()),
                        });
                    }
                } else {
                    let (subject_key, base_verb) = if let Some(p2) = parts.next() {
                        if parts.next().is_some() {
                            return Err(validation_error("Malformed verb tag", content, '['));
                        }
                        if p1.is_empty() {
                            return Err(validation_error(
                                "Verb tag has an empty subject key",
                                content,
                                '[',
                            ));
                        }
                        // p2 (base_verb) can be empty, we warn for that later.
                        (Some(p1.to_lowercase()), p2)
                    } else {
                        (None, p1)
                    };

                    let original_verb = base_verb.to_string();
                    let is_capitalized = original_verb
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_uppercase());
                    let lower_verb = original_verb.to_lowercase();

                    if original_verb.is_empty() {
                        tracing::warn!(
                            "Parsed an empty verb tag in template. This will conjugate to just 's'."
                        );
                    }

                    tokens.push(Token::VerbRef {
                        subject_key,
                        original_verb,
                        lower_verb,
                        is_capitalized,
                    });
                }
                last_literal_start = end_idx + 1;
            } else {
                // Move to the next character if it's not a special tag
                chars.next();
            }
        }

        // Push any remaining literal text at the end of the string
        if last_literal_start < raw.len() {
            tokens.push(Token::Literal(raw[last_literal_start..].to_string()));
        }

        Ok(Template {
            tokens,
            estimated_length: raw.len() + (raw.len() / 5),
        })
    }
}

/// The core processor responsible for evaluating compiled templates against contexts.
pub struct PerspectiveEngine;

impl PerspectiveEngine {
    /// Renders a compiled `Template` into a final perspective-aware string.
    ///
    /// This method evaluates all pronouns, verbs, and articles based on the `RenderContext`.
    /// It utilizes a single pre-allocated string buffer and performs typography
    /// post-processing to guarantee proper sentence capitalization.
    ///
    /// # Arguments
    /// * `template` - A reference to the pre-compiled AST.
    /// * `ctx` - The perspective context containing the viewer ID and entity mappings.
    ///
    /// # Errors
    /// Returns a `String` error if the template references a key not provided in the `ctx`.
    #[tracing::instrument(level = "trace", skip_all, fields(viewer_id = %ctx.viewer_id))]
    pub fn render(template: &Template, ctx: &RenderContext) -> Result<String, String> {
        // 1. Pre-allocate buffer to prevent continuous heap allocations
        let mut raw_output = String::with_capacity(template.estimated_length);

        let get_entity = |key: &str| {
            ctx.entities.get(key).copied().ok_or_else(|| {
                tracing::error!(
                    "Failed to render template: Missing entity for key '{}'",
                    key
                );
                format!("Missing entity for key: {}", key)
            })
        };

        for token in &template.tokens {
            match token {
                Token::Literal(text) => {
                    raw_output.push_str(text);
                }
                Token::EntityRef {
                    key,
                    article,
                    is_capitalized,
                } => {
                    let entity = get_entity(key)?;

                    let is_viewer = entity.contains_viewer(ctx.viewer_id);
                    let name = entity.display_name_for(ctx.viewer_id);

                    // Capitalize the name if explicitly requested and it isn't already
                    let name_buf;
                    let name_str = if *is_capitalized
                        && name.chars().next().is_some_and(|c| c.is_lowercase())
                    {
                        name_buf = crate::grammar::capitalize_first(&name);
                        name_buf.as_str()
                    } else {
                        name.as_ref()
                    };

                    // Handle dynamic "a" or "an" injection
                    if let Some(art) = article {
                        if let Some(resolved_art) = resolve_article(
                            art,
                            name_str,
                            is_viewer,
                            entity.is_proper_noun_for(ctx.viewer_id),
                            entity.is_plural(),
                        ) {
                            raw_output.push_str(resolved_art);
                        }
                    }

                    raw_output.push_str(name_str);
                }
                Token::PronounRef {
                    key,
                    p_type,
                    is_capitalized,
                } => {
                    let entity = get_entity(key)?;

                    let is_viewer = entity.contains_viewer(ctx.viewer_id);
                    let pronoun =
                        resolve_pronoun(entity.gender(), p_type, is_viewer, entity.is_plural())?;

                    if *is_capitalized {
                        raw_output.push_str(&crate::grammar::capitalize_first(pronoun));
                    } else {
                        raw_output.push_str(pronoun);
                    }
                }
                Token::VerbRef {
                    subject_key,
                    original_verb,
                    lower_verb,
                    is_capitalized,
                } => {
                    // Explicitly bind the verb to its subject to solve passive voice / compound subjects
                    let (is_viewer, is_plural) = if let Some(key) = subject_key {
                        let entity = get_entity(key)?;
                        (entity.contains_viewer(ctx.viewer_id), entity.is_plural())
                    } else {
                        // Safe default to 3rd-person singular if no subject is bound
                        (false, false)
                    };

                    let conjugated = conjugate_verb(
                        original_verb,
                        lower_verb,
                        *is_capitalized,
                        is_viewer,
                        is_plural,
                    );
                    raw_output.push_str(&conjugated);
                }
            }
        }

        // 2. Pass the fully assembled base-case string to the typography post-processor
        Ok(Self::post_process_typography(raw_output))
    }

    /// Segments the text by true sentence boundaries and capitalizes the first letter.
    fn post_process_typography(input: String) -> String {
        let mut output = String::with_capacity(input.capacity());

        let mut bounds = input.split_sentence_bound_indices();
        bounds.next(); // Skip the first bound (which is always 0)
        let mut next_sentence_start = bounds.next().map(|(i, _)| i);
        let mut capitalized = false;
        let mut last_real_char = None;

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

        while let Some(&(i, c)) = chars.peek() {
            // If we cross into a new sentence boundary, reset the capitalization flag
            if catch_up_bounds(i, &mut next_sentence_start) {
                capitalized = false;
            }

            #[allow(unused_variables)]
            let remainder = &input[i..];
            #[allow(unused_mut)]
            let mut skipped_tag = false;

            // 1, 2, & 3. Skip MXP Tags, MSP Triggers, and ANSI Escape Sequences
            #[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
            if let Some(end_offset) = find_skipped_tag_end(remainder) {
                output.push_str(&remainder[..=end_offset]);
                advance_chars_until(&mut chars, i + end_offset);
                skipped_tag = true;
            }

            if skipped_tag {
                // Discard any sentence boundaries that fell inside the tag we just skipped.
                // Otherwise, crossing them will falsely trigger the capitalization flag
                // on the next visible word.
                if let Some(&(curr_i, _)) = chars.peek() {
                    catch_up_bounds(curr_i, &mut next_sentence_start);
                }

                // If the last real character was a sentence terminator, the tag might have
                // hidden the sentence boundary from the unicode segmenter. Force a reset.
                if let Some(lrc) = last_real_char
                    && (lrc == '.' || lrc == '!' || lrc == '?')
                {
                    capitalized = false;
                }
                continue;
            }

            // 4. Normal Character Processing
            chars.next(); // Consume the character

            if !capitalized && c.is_alphabetic() {
                // We found the first real letter outside of any tags! Capitalize it.
                for uc in c.to_uppercase() {
                    output.push(uc);
                }
                capitalized = true;
                last_real_char = Some(c);
            } else {
                // It's whitespace, punctuation, or numbers. Push it and keep looking.
                output.push(c);
                if !c.is_whitespace() {
                    last_real_char = Some(c);
                }
            }
        }

        output
    }
}

#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
#[inline]
fn advance_chars_until(
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

#[inline]
fn consume_until_closed(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    start_idx: usize,
    close_char: char,
    tag_type: &str,
) -> Result<usize, String> {
    let mut end_idx = start_idx + 1;
    let mut closed = false;
    while let Some(&(j, ch)) = chars.peek() {
        chars.next();
        if ch == close_char {
            end_idx = j;
            closed = true;
            break;
        }
    }

    if !closed {
        tracing::error!("Unclosed {} starting at index {}", tag_type, start_idx);
        return Err(format!(
            "Unclosed {} starting at index {}",
            tag_type, start_idx
        ));
    }

    Ok(end_idx)
}

#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
#[inline]
fn find_skipped_tag_end(remainder: &str) -> Option<usize> {
    #[cfg(feature = "mxp")]
    if remainder.starts_with('<')
        && let Some(end_offset) = remainder.find('>')
    {
        return Some(end_offset);
    }

    #[cfg(feature = "msp")]
    if (remainder.starts_with("!!SOUND(") || remainder.starts_with("!!MUSIC("))
        && let Some(end_offset) = remainder.find(')')
    {
        return Some(end_offset);
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

#[cold]
fn validation_error(message: &str, content: &str, open_char: char) -> String {
    let close_char = if open_char == '{' { '}' } else { ']' };
    let formatted_message = format!("{}: {}{}{}", message, open_char, content, close_char);
    tracing::error!("{}", formatted_message);
    formatted_message
}
