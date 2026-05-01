use crate::grammar::{conjugate_verb, get_indefinite_article, resolve_pronoun};
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
    },
    /// e.g., {source:poss}
    PronounRef {
        /// The key of the entity in the `RenderContext`.
        key: String,
        /// The type of pronoun requested (e.g., "subj", "obj", "poss", "abs_poss", "reflex").
        p_type: String,
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
            if c == '\x1b' {
                chars.next();
                if let Some(&(_, next_c)) = chars.peek()
                    && next_c == '['
                {
                    chars.next(); // Consume the `[` so it isn't mistakenly parsed as a verb tag
                }
            } else if c == '{' {
                // Push any preceding literal text
                if i > last_literal_start {
                    tokens.push(Token::Literal(raw[last_literal_start..i].to_string()));
                }
                chars.next(); // Consume '{'

                let mut end_idx = i + 1;
                let mut closed = false;
                while let Some(&(j, ch)) = chars.peek() {
                    chars.next();
                    if ch == '}' {
                        end_idx = j;
                        closed = true;
                        break;
                    }
                }

                if !closed {
                    return Err(format!("Unclosed entity tag starting at index {}", i));
                }

                let content = &raw[i + 1..end_idx];
                let mut parts = content.split(':');

                let p1 = parts.next().unwrap();
                if let Some(p2) = parts.next() {
                    if parts.next().is_some() {
                        return Err(format!("Malformed entity tag: {{{}}}", content));
                    }

                    // 2-part case
                    if p1 == "a" || p1 == "an" {
                        tokens.push(Token::EntityRef {
                            key: p2.to_string(),
                            article: Some("a".to_string()),
                        });
                    } else if p1 == "the" {
                        tokens.push(Token::EntityRef {
                            key: p2.to_string(),
                            article: Some("the".to_string()),
                        });
                    } else {
                        // Otherwise, it's a pronoun like {source:poss}
                        tokens.push(Token::PronounRef {
                            key: p1.to_string(),
                            p_type: p2.to_string(),
                        });
                    }
                } else {
                    // 1-part case
                    tokens.push(Token::EntityRef {
                        key: p1.to_string(),
                        article: None,
                    });
                }
                last_literal_start = end_idx + 1;
            } else if c == '[' {
                // Push any preceding literal text
                if i > last_literal_start {
                    tokens.push(Token::Literal(raw[last_literal_start..i].to_string()));
                }
                chars.next(); // Consume '['

                let mut end_idx = i + 1;
                let mut closed = false;
                while let Some(&(j, ch)) = chars.peek() {
                    chars.next();
                    if ch == ']' {
                        end_idx = j;
                        closed = true;
                        break;
                    }
                }

                if !closed {
                    return Err(format!("Unclosed verb tag starting at index {}", i));
                }

                let content = &raw[i + 1..end_idx];
                let mut parts = content.split(':');

                let p1 = parts.next().unwrap();
                let (subject_key, base_verb) = if let Some(p2) = parts.next() {
                    if parts.next().is_some() {
                        return Err(format!("Malformed verb tag: [{}]", content));
                    }
                    (Some(p1.to_string()), p2)
                } else {
                    (None, p1)
                };

                let original_verb = base_verb.to_string();
                let is_capitalized = original_verb
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase());
                let lower_verb = original_verb.to_lowercase();

                tokens.push(Token::VerbRef {
                    subject_key,
                    original_verb,
                    lower_verb,
                    is_capitalized,
                });
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
    pub fn render(template: &Template, ctx: &RenderContext) -> Result<String, String> {
        // 1. Pre-allocate buffer to prevent continuous heap allocations
        let mut raw_output = String::with_capacity(template.estimated_length);

        for token in &template.tokens {
            match token {
                Token::Literal(text) => {
                    raw_output.push_str(text);
                }
                Token::EntityRef { key, article } => {
                    let entity = ctx
                        .entities
                        .get(key.as_str())
                        .ok_or_else(|| format!("Missing entity for key: {}", key))?;

                    let is_viewer = entity.contains_viewer(ctx.viewer_id);
                    let name = entity.display_name_for(ctx.viewer_id);

                    // Handle dynamic "a" or "an" injection
                    if let Some(art) = article {
                        // Suppress articles if the viewer is part of this group
                        // OR if it's a proper noun
                        if is_viewer || entity.is_proper_noun_for(ctx.viewer_id) {
                            raw_output.push_str(&name);
                        } else if art == "a" || art == "an" {
                            let indefinite = get_indefinite_article(&name);
                            raw_output.push_str(indefinite);
                            raw_output.push(' ');
                            raw_output.push_str(&name);
                        } else if art == "the" {
                            raw_output.push_str("the ");
                            raw_output.push_str(&name);
                        }
                    } else {
                        raw_output.push_str(&name);
                    }
                }
                Token::PronounRef { key, p_type } => {
                    let entity = ctx
                        .entities
                        .get(key.as_str())
                        .ok_or_else(|| format!("Missing entity for key: {}", key))?;

                    let is_viewer = entity.contains_viewer(ctx.viewer_id);
                    let pronoun =
                        resolve_pronoun(entity.gender(), p_type, is_viewer, entity.is_plural())?;
                    raw_output.push_str(pronoun);
                }
                Token::VerbRef {
                    subject_key,
                    original_verb,
                    lower_verb,
                    is_capitalized,
                } => {
                    // Explicitly bind the verb to its subject to solve passive voice / compound subjects
                    let (is_viewer, is_plural) = if let Some(key) = subject_key {
                        let entity = ctx
                            .entities
                            .get(key.as_str())
                            .ok_or_else(|| format!("Missing entity for key: {}", key))?;
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

        let mut chars = input.char_indices().peekable();

        while let Some(&(i, c)) = chars.peek() {
            // If we cross into a new sentence boundary, reset the capitalization flag
            while let Some(next_start) = next_sentence_start {
                if i >= next_start {
                    capitalized = false;
                    next_sentence_start = bounds.next().map(|(idx, _)| idx);
                } else {
                    break;
                }
            }

            let remainder = &input[i..];
            let mut skipped_tag = false;

            // 1. Skip MXP Tags (e.g., <SEND HREF="..."> or <COLOR red>)
            if c == '<'
                && let Some(end_offset) = remainder.find('>')
            {
                output.push_str(&remainder[..=end_offset]);
                let target_i = i + end_offset;
                while let Some(&(curr_i, _)) = chars.peek() {
                    if curr_i <= target_i {
                        chars.next();
                    } else {
                        break;
                    }
                }
                skipped_tag = true;
            } else if (remainder.starts_with("!!SOUND(") || remainder.starts_with("!!MUSIC("))
                && let Some(end_offset) = remainder.find(')')
            {
                // 2. Skip MSP Triggers (e.g., !!SOUND(...) or !!MUSIC(...))
                output.push_str(&remainder[..=end_offset]);
                let target_i = i + end_offset;
                while let Some(&(curr_i, _)) = chars.peek() {
                    if curr_i <= target_i {
                        chars.next();
                    } else {
                        break;
                    }
                }
                skipped_tag = true;
            } else if c == '\x1b' {
                // 3. Skip ANSI Escape Sequences
                chars.next(); // Consume ESC
                output.push('\x1b');

                if let Some(&(_, next_c)) = chars.peek() {
                    output.push(next_c);
                    chars.next();

                    match next_c {
                        '[' => {
                            // CSI Sequences: Read until a final character (0x40-0x7E)
                            while let Some(&(_, csi_c)) = chars.peek() {
                                output.push(csi_c);
                                chars.next();
                                if (0x40..=0x7E).contains(&(csi_c as u8)) {
                                    break;
                                }
                            }
                        }
                        ']' | 'P' | 'X' | '^' | '_' => {
                            // OSC / DCS Sequences: Read until BEL (\x07) or ST (\x1b\)
                            let mut last_char = next_c;
                            while let Some(&(_, osc_c)) = chars.peek() {
                                output.push(osc_c);
                                chars.next();
                                if osc_c == '\x07' || (last_char == '\x1b' && osc_c == '\\') {
                                    break;
                                }
                                last_char = osc_c;
                            }
                        }
                        '(' | ')' | '*' | '+' | '-' | '.' | '/' => {
                            // Charset designators: Read exactly one more character
                            if let Some(&(_, char_c)) = chars.peek() {
                                output.push(char_c);
                                chars.next();
                            }
                        }
                        _ => {
                            // Simple 2-character escape (already consumed)
                        }
                    }
                }
                skipped_tag = true;
            }

            if skipped_tag {
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
