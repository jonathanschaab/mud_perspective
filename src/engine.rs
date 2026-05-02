use crate::grammar::{conjugate_verb, resolve_article, resolve_pronoun};
use crate::models::{NULL_VIEWER, RenderContext, TemplateEntity};
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
        /// A flag indicating if the builder explicitly forced the article to render (e.g. {+the:source}).
        force_article: bool,
        /// A flag indicating if the builder explicitly forced the 3rd-person stance (e.g. {+source}).
        force_3rd_person: bool,
        /// A flag indicating if the builder explicitly forced the possessive form (e.g. {source's}).
        is_possessive: bool,
    },
    /// e.g., {source:poss}
    PronounRef {
        /// The key of the entity in the `RenderContext`.
        key: String,
        /// The type of pronoun requested (e.g., `"subj"`, `"obj"`, `"poss"`, `"abs_poss"`, `"reflex"`).
        p_type: String,
        /// A flag indicating if the pronoun type was capitalized (e.g. {source:Subj}).
        is_capitalized: bool,
        /// A flag indicating if the builder explicitly forced the 3rd-person stance (e.g. {+source:poss}).
        force_3rd_person: bool,
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
        /// A flag indicating if the builder explicitly forced 3rd-person conjugation (e.g. [+source:pulse]).
        force_3rd_person: bool,
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
                    push_literal(&mut tokens, raw, last_literal_start, i);
                    last_literal_start = next_i;
                    chars.next();
                }
                continue;
            }

            if c == '{' || c == '[' {
                // Push any preceding literal text
                push_literal(&mut tokens, raw, last_literal_start, i);
                chars.next(); // Consume the opening brace or bracket

                let is_entity = c == '{';
                let close_char = if is_entity { '}' } else { ']' };
                let tag_name = if is_entity { "entity tag" } else { "verb tag" };

                let end_idx = consume_until_closed(&mut chars, i, close_char, tag_name)?;
                let content = &raw[i + 1..end_idx];

                let token = if is_entity {
                    Self::parse_entity_or_pronoun(content)?
                } else {
                    Self::parse_verb(content)?
                };

                tokens.push(token);
                last_literal_start = end_idx + 1;
            } else {
                // Move to the next character if it's not a special tag
                chars.next();
            }
        }

        // Push any remaining literal text at the end of the string
        push_literal(&mut tokens, raw, last_literal_start, raw.len());

        Ok(Template {
            tokens,
            estimated_length: raw.len() + (raw.len() / 5),
        })
    }

    fn parse_entity_or_pronoun(content: &str) -> Result<Token, String> {
        let (p1, p2_opt) = split_tag(content, '{', "Malformed entity tag")?;

        if let Some(p2) = p2_opt {
            let (p1_str, force_article) = parse_stance_prefixes(p1);

            // 2-part case: {article:key}
            if is_article(p1_str) {
                let (p2_str, force_3rd_person, is_possessive) = parse_entity_modifiers(p2);

                if p2_str.is_empty() {
                    return Err(validation_error(
                        "Entity tag has an article but an empty key",
                        content,
                        '{',
                    ));
                }
                create_entity_ref(
                    p2_str,
                    Some(p1_str),
                    force_article,
                    force_3rd_person,
                    is_possessive,
                    content,
                )
            } else {
                // 2-part case: {key:pronoun}
                // Strip trailing 's in case someone made a typo like {source's:subj}
                let (p1_str, force_3rd_person, _) = parse_entity_modifiers(p1);

                if p1_str.is_empty() || p2.is_empty() {
                    return Err(validation_error(
                        "Pronoun tag has an empty key or type",
                        content,
                        '{',
                    ));
                }
                validate_property_segments(
                    p1_str,
                    "Pronoun tag has an empty property segment",
                    content,
                    '{',
                )?;
                Ok(Token::PronounRef {
                    key: p1_str.to_lowercase(),
                    p_type: p2.to_lowercase(),
                    is_capitalized: is_capitalized(p2),
                    force_3rd_person,
                })
            }
        } else {
            // 1-part case: {key}
            let (p1_str, force_3rd_person, is_possessive) = parse_entity_modifiers(p1);

            if p1_str.is_empty() {
                return Err(validation_error(
                    "Entity tag has an empty key",
                    content,
                    '{',
                ));
            }
            create_entity_ref(
                p1_str,
                None,
                false,
                force_3rd_person,
                is_possessive,
                content,
            )
        }
    }

    fn parse_verb(content: &str) -> Result<Token, String> {
        let (p1, p2_opt) = split_tag(content, '[', "Malformed verb tag")?;
        let (p1_str, force_3rd_person) = parse_stance_prefixes(p1);

        let (subject_key, base_verb) = if let Some(p2) = p2_opt {
            if p1_str.is_empty() {
                return Err(validation_error(
                    "Verb tag has an empty subject key",
                    content,
                    '[',
                ));
            }
            validate_property_segments(
                p1_str,
                "Verb tag has an empty property segment",
                content,
                '[',
            )?;
            (Some(p1_str.to_lowercase()), p2)
        } else {
            (None, p1_str)
        };

        let original_verb = base_verb.to_string();
        let is_capitalized = is_capitalized(&original_verb);
        let lower_verb = original_verb.to_lowercase();

        if original_verb.is_empty() {
            tracing::warn!(
                "Parsed an empty verb tag in template. This will conjugate to just 's'."
            );
        }

        Ok(Token::VerbRef {
            subject_key,
            original_verb,
            lower_verb,
            is_capitalized,
            force_3rd_person,
        })
    }
}

/// Parameters extracted from a token or fallback logic to render an entity.
#[allow(clippy::struct_excessive_bools)]
struct EntityRefParams<'a> {
    key: &'a str,
    article: Option<&'a str>,
    is_capitalized: bool,
    force_article: bool,
    force_3rd_person: bool,
    is_possessive: bool,
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

        for token in &template.tokens {
            match token {
                Token::Literal(text) => raw_output.push_str(text),
                Token::EntityRef {
                    key,
                    article,
                    is_capitalized,
                    force_article,
                    force_3rd_person,
                    is_possessive,
                } => Self::render_entity_ref(
                    ctx,
                    &mut raw_output,
                    &EntityRefParams {
                        key,
                        article: article.as_deref(),
                        is_capitalized: *is_capitalized,
                        force_article: *force_article,
                        force_3rd_person: *force_3rd_person,
                        is_possessive: *is_possessive,
                    },
                )?,
                Token::PronounRef { .. } => Self::render_pronoun_ref(ctx, &mut raw_output, token)?,
                Token::VerbRef { .. } => Self::render_verb_ref(ctx, &mut raw_output, token)?,
            }
        }

        // 2. Pass the fully assembled base-case string to the typography post-processor
        Ok(Self::post_process_typography(&raw_output))
    }

    #[inline]
    fn get_entity<'a>(ctx: &'a RenderContext, key: &str) -> Result<&'a dyn TemplateEntity, String> {
        // 1. Try exact match first (e.g., "source")
        if let Some(entity) = ctx.entities.get(key).copied() {
            return Ok(entity);
        }

        // 2. Try dot notation traversal (e.g., "source.left_arm.weapon")
        if let Some((root_key, remainder)) = key.split_once('.')
            && let Some(mut current) = ctx.entities.get(root_key).copied()
        {
            let mut current_path = root_key;
            for prop in remainder.split('.') {
                current = current.get_property(prop).ok_or_else(|| {
                    format!("Missing property '{prop}' on entity '{current_path}'")
                })?;
                // Accumulate the traversed path slice by extending it over the dot and property
                current_path = &key[..current_path.len() + 1 + prop.len()];
            }
            return Ok(current);
        }

        tracing::error!("Failed to render template: Missing entity for key '{key}'");
        Err(format!("Missing entity for key: {key}"))
    }

    fn render_entity_ref(
        ctx: &RenderContext,
        raw_output: &mut String,
        params: &EntityRefParams<'_>,
    ) -> Result<(), String> {
        update_memory(&ctx.last_mentioned, params.key);

        let entity = Self::get_entity(ctx, params.key)?;
        let effective_viewer = effective_viewer_id(ctx, params.force_3rd_person);

        // --- Handle Groups / Distributed Lists ---
        if let Some(members) = entity.group_members() {
            Self::render_group_entity(raw_output, entity, members, effective_viewer, params);
            return Ok(());
        }

        // --- Handle Single Entity Viewers ---
        if entity.contains_viewer(effective_viewer) {
            raw_output.push_str(viewer_name(params.is_possessive, params.is_capitalized));
            return Ok(());
        }

        // --- Single Entity Fallback ---
        let name = entity.display_name_for(effective_viewer);

        // Capitalize the name if explicitly requested and it isn't already
        let name_cow = capitalize_cow(name, params.is_capitalized);
        let name_str = name_cow.as_ref();

        // Handle dynamic "a" or "an" injection
        if let Some(resolved_art) = params.article.and_then(|art| {
            resolve_article(
                art,
                name_str,
                entity.is_proper_noun_for(effective_viewer),
                entity.is_plural(),
                params.force_article,
            )
        }) {
            raw_output.push_str(resolved_art);
        }

        raw_output.push_str(name_str);

        if params.is_possessive {
            raw_output.push_str(Self::get_possessive_suffix(name_str, entity.is_plural()));
        }
        Ok(())
    }

    fn render_group_entity(
        raw_output: &mut String,
        entity: &dyn TemplateEntity,
        members: &[&dyn TemplateEntity],
        effective_viewer: &str,
        params: &EntityRefParams<'_>,
    ) {
        let (viewer_entity, visible) = crate::models::partition_group(members, effective_viewer);

        let has_viewer = viewer_entity.is_some();
        let total_visible = visible.len() + usize::from(has_viewer);
        if total_visible == 0 {
            return;
        }

        let mut formatted_names = Vec::with_capacity(total_visible);
        if has_viewer {
            formatted_names.push(std::borrow::Cow::Borrowed("you"));
        }

        for (m, name) in visible {
            if let Some(resolved_art) = params.article.and_then(|art| {
                resolve_article(
                    art,
                    &name,
                    m.is_proper_noun_for(effective_viewer),
                    m.is_plural(),
                    params.force_article,
                )
            }) {
                formatted_names.push(std::borrow::Cow::Owned(format!("{resolved_art}{name}")));
                continue;
            }
            formatted_names.push(name);
        }

        let list_str = crate::grammar::format_oxford_list(formatted_names);

        let mut final_str = list_str.into_owned();
        if params.is_possessive {
            final_str.push_str(Self::get_possessive_suffix(&final_str, entity.is_plural()));
        }

        push_capitalized_if(raw_output, &final_str, params.is_capitalized);
    }

    #[inline]
    fn get_last_visible_char(input: &str) -> Option<char> {
        let mut chars = input.char_indices().peekable();
        let mut last_visible = None;

        while let Some(&(i, c)) = chars.peek() {
            #[allow(unused_variables)]
            let remainder = &input[i..];

            #[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
            if let Some(end_offset) = find_skipped_tag_end(remainder) {
                advance_chars_until(&mut chars, i + end_offset);
                continue;
            }

            chars.next();
            if !c.is_whitespace() {
                last_visible = Some(c);
            }
        }

        last_visible
    }

    #[inline]
    fn get_possessive_suffix(name: &str, is_plural: bool) -> &'static str {
        if matches!(Self::get_last_visible_char(name), Some('s' | 'S')) && is_plural {
            return "'";
        }
        "'s"
    }

    fn render_pronoun_ref(
        ctx: &RenderContext,
        raw_output: &mut String,
        token: &Token,
    ) -> Result<(), String> {
        let Token::PronounRef {
            key,
            p_type,
            is_capitalized,
            force_3rd_person,
        } = token
        else {
            return Ok(());
        };

        let entity = Self::get_entity(ctx, key)?;
        let effective_viewer = effective_viewer_id(ctx, *force_3rd_person);

        let is_viewer = entity.contains_viewer(effective_viewer);

        let is_active_subject = ctx.active_subject.borrow().as_deref() == Some(key.as_str());

        // Check if this entity is the current subject of the narrative.
        let already_seen = ctx.last_mentioned.borrow().as_deref() == Some(key.as_str());

        let is_reflexive = p_type == "reflex";

        let mut can_use_pronoun = is_active_subject || is_viewer || is_reflexive;

        if !can_use_pronoun && already_seen {
            let unambiguous = if let Some(subj_key) = ctx.active_subject.borrow().as_deref() {
                if let Ok(subj_entity) = Self::get_entity(ctx, subj_key) {
                    let subj_is_viewer = subj_entity.contains_viewer(effective_viewer);
                    is_viewer != subj_is_viewer
                        || entity.gender() != subj_entity.gender()
                        || entity.is_plural() != subj_entity.is_plural()
                } else {
                    true
                }
            } else {
                true
            };

            if unambiguous {
                can_use_pronoun = true;
            }
        }

        if can_use_pronoun {
            if !already_seen {
                update_memory(&ctx.last_mentioned, key);
            }

            let pronoun = resolve_pronoun(entity.gender(), p_type, is_viewer, entity.is_plural())?;
            push_capitalized_if(raw_output, pronoun, *is_capitalized);
        } else {
            // Smart Anaphora Resolution: The entity hasn't been introduced yet, or a pronoun would be ambiguous!
            // Evaluate it as if the builder had written `{the:key}` instead.
            let is_possessive = p_type == "poss" || p_type == "abs_poss";
            let fallback_params = EntityRefParams {
                key,
                article: Some(if *is_capitalized { "The" } else { "the" }),
                // We set `is_capitalized: false` here because the capitalization requested by the pronoun
                // (e.g. `{target:Subj}`) applies to the *first word* of the substitution (the article "The").
                // We do not want to force-capitalize common nouns (yielding "The Goblin" instead of "The goblin").
                // Proper nouns (like "Aldran") naturally return capitalized strings and are unaffected.
                is_capitalized: false,
                force_article: false,
                force_3rd_person: *force_3rd_person,
                is_possessive,
            };
            Self::render_entity_ref(ctx, raw_output, &fallback_params)?;
        }
        Ok(())
    }

    fn render_verb_ref(
        ctx: &RenderContext,
        raw_output: &mut String,
        token: &Token,
    ) -> Result<(), String> {
        let Token::VerbRef {
            subject_key,
            original_verb,
            lower_verb,
            is_capitalized,
            force_3rd_person,
        } = token
        else {
            return Ok(());
        };

        // Explicitly bind the verb to its subject to solve passive voice / compound subjects
        let (is_viewer, is_plural) = if let Some(key) = subject_key {
            let entity = Self::get_entity(ctx, key)?;
            let effective_viewer = effective_viewer_id(ctx, *force_3rd_person);
            update_memory(&ctx.active_subject, key);
            (entity.contains_viewer(effective_viewer), entity.is_plural())
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
        Ok(())
    }

    /// Segments the text by true sentence boundaries and capitalizes the first letter.
    fn post_process_typography(input: &str) -> String {
        let mut output = String::with_capacity(input.len());

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
                if matches!(last_real_char, Some('.' | '!' | '?')) {
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
        return Err(format!("Unclosed {tag_type} starting at index {start_idx}"));
    }

    Ok(end_idx)
}

#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
#[inline]
fn find_skipped_tag_end(remainder: &str) -> Option<usize> {
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

#[cold]
fn validation_error(message: &str, content: &str, open_char: char) -> String {
    let close_char = if open_char == '{' { '}' } else { ']' };
    let formatted_message = format!("{message}: {open_char}{content}{close_char}");
    tracing::error!("{}", formatted_message);
    formatted_message
}

/// Parses prefix modifiers (like `+`) used to force perspectives, returning the
/// stripped string alongside the boolean flag for `force_3rd_person`/`force_article`.
#[inline]
fn parse_stance_prefixes(s: &str) -> (&str, bool) {
    if let Some(stripped) = s.strip_prefix('+') {
        (stripped, true)
    } else {
        (s, false)
    }
}

#[inline]
fn parse_entity_modifiers(s: &str) -> (&str, bool, bool) {
    let (s, force_3rd_person) = parse_stance_prefixes(s);
    let (s, is_possessive) = parse_possessive_suffix(s);
    (s, force_3rd_person, is_possessive)
}

#[inline]
fn split_tag<'a>(
    content: &'a str,
    open_char: char,
    malformed_msg: &str,
) -> Result<(&'a str, Option<&'a str>), String> {
    let mut parts = content.split(':');
    let p1 = parts.next().unwrap_or_default();
    let p2 = parts.next();
    if parts.next().is_some() {
        return Err(validation_error(malformed_msg, content, open_char));
    }
    Ok((p1, p2))
}

#[inline]
fn update_memory(memory: &std::cell::RefCell<Option<String>>, key: &str) {
    if memory.borrow().as_deref() != Some(key) {
        *memory.borrow_mut() = Some(key.to_string());
    }
}

#[inline]
fn push_capitalized_if(output: &mut String, text: &str, should_capitalize: bool) {
    if should_capitalize && text.chars().next().is_some_and(char::is_lowercase) {
        output.push_str(&crate::grammar::capitalize_first(text));
    } else {
        output.push_str(text);
    }
}

#[inline]
fn capitalize_cow(
    text: std::borrow::Cow<'_, str>,
    should_capitalize: bool,
) -> std::borrow::Cow<'_, str> {
    if should_capitalize && text.chars().next().is_some_and(char::is_lowercase) {
        std::borrow::Cow::Owned(crate::grammar::capitalize_first(&text))
    } else {
        text
    }
}

#[inline]
fn push_literal(tokens: &mut Vec<Token>, raw: &str, start: usize, end: usize) {
    if end > start {
        tokens.push(Token::Literal(raw[start..end].to_string()));
    }
}

#[inline]
fn is_article(s: &str) -> bool {
    s.eq_ignore_ascii_case("a")
        || s.eq_ignore_ascii_case("an")
        || s.eq_ignore_ascii_case("the")
        || s.eq_ignore_ascii_case("this")
        || s.eq_ignore_ascii_case("that")
}

#[inline]
fn create_entity_ref(
    key: &str,
    article: Option<&str>,
    force_article: bool,
    force_3rd_person: bool,
    is_possessive: bool,
    content: &str,
) -> Result<Token, String> {
    validate_property_segments(
        key,
        "Entity tag has an empty property segment",
        content,
        '{',
    )?;
    Ok(Token::EntityRef {
        key: key.to_lowercase(),
        article: article.map(ToString::to_string),
        is_capitalized: is_capitalized(key),
        force_article,
        force_3rd_person,
        is_possessive,
    })
}

#[inline]
const fn viewer_name(is_possessive: bool, is_capitalized: bool) -> &'static str {
    match (is_possessive, is_capitalized) {
        (true, true) => "Your",
        (true, false) => "your",
        (false, true) => "You",
        (false, false) => "you",
    }
}

#[inline]
fn is_capitalized(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_uppercase)
}

#[inline]
fn validate_property_segments(
    path: &str,
    error_msg: &str,
    content: &str,
    open_char: char,
) -> Result<(), String> {
    if path.split('.').any(str::is_empty) {
        Err(validation_error(error_msg, content, open_char))
    } else {
        Ok(())
    }
}

#[inline]
fn effective_viewer_id<'a>(ctx: &RenderContext<'a>, force_3rd_person: bool) -> &'a str {
    if force_3rd_person {
        NULL_VIEWER
    } else {
        ctx.viewer_id
    }
}

/// Parses possessive suffix `'s`, returning the stripped string and a boolean flag.
#[inline]
fn parse_possessive_suffix(s: &str) -> (&str, bool) {
    if let Some(stripped) = s.strip_suffix("'s") {
        (stripped, true)
    } else {
        (s, false)
    }
}
