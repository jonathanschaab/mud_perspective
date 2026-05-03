use crate::grammar::{conjugate_verb, resolve_article, resolve_pronoun};
use crate::models::{NULL_VIEWER, RenderContext, TemplateEntity};
use std::collections::HashMap;
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
        /// A packed bitflags struct containing all formatting modifiers.
        flags: TagFlags,
    },
    /// e.g., {source:poss}
    PronounRef {
        /// The key of the entity in the `RenderContext`.
        key: String,
        /// The type of pronoun requested (e.g., `"subj"`, `"obj"`, `"poss"`, `"abs_poss"`, `"reflex"`).
        p_type: String,
        /// A packed bitflags struct containing all formatting modifiers.
        flags: TagFlags,
    },
    /// e.g., [source:pulse]
    VerbRef {
        /// The optional subject key to bind the verb to for correct conjugation.
        subject_key: Option<String>,
        /// The original, un-processed form of the verb from the template.
        original_verb: String,
        /// The lowercased form of the verb, for dictionary lookups.
        lower_verb: String,
        /// A sequence of explicit present-tense overrides that bypasses the algorithm entirely (e.g., `["am", "are", "is"]`).
        /// Note: This vector does not include the base verb itself, which is stored in `original_verb`.
        forced_present: Option<Vec<String>>,
        /// A sequence of explicit past-tense overrides that bypasses the algorithm entirely (e.g., `["was", "were", "was"]`).
        forced_past: Option<Vec<String>>,
        /// A packed bitflags struct containing all formatting modifiers.
        flags: TagFlags,
    },
    /// A tag that forces a new sentence boundary for capitalization.
    SentenceBreak,
    /// A tag that prevents the next sentence boundary from triggering capitalization.
    NoSentenceBreak,
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

        #[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
        let has_tags = has_protocol_tags(raw);

        while let Some(&(i, c)) = chars.peek() {
            #[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
            if has_tags {
                let remainder = raw.get(i..).unwrap_or_default();
                if skip_protocol_tags(&mut chars, remainder, i).is_some() {
                    continue;
                }
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
                let content = raw.get(i + 1..end_idx).unwrap_or_default();

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
            let (p1_str, force_article, no_smart_modifier, force_singular_1) =
                parse_stance_prefixes(p1);

            // 2-part case: {article:key}
            if is_article(p1_str) {
                let (p2_str, force_3rd_person, _, force_singular_2, is_possessive) =
                    parse_entity_modifiers(p2);

                if p2_str.is_empty() {
                    return Err(validation_error(
                        "Entity tag has an article but an empty key",
                        content,
                        '{',
                    ));
                }
                let flags = TagFlags::new(
                    is_capitalized(p2_str),
                    force_article,
                    force_3rd_person,
                    is_possessive,
                    no_smart_modifier,
                    force_singular_1 || force_singular_2,
                );
                create_entity_ref(p2_str, Some(p1_str), flags, content)
            } else {
                // 2-part case: {key:pronoun}
                create_pronoun_ref(p1, p2, content)
            }
        } else {
            // 1-part case: {key}
            let (p1_str, force_3rd_person, _, force_singular, is_possessive) =
                parse_entity_modifiers(p1);

            reject_if(
                p1_str.is_empty(),
                "Entity tag has an empty key",
                content,
                '{',
            )?;
            let flags = TagFlags::new(
                is_capitalized(p1_str),
                false,
                force_3rd_person,
                is_possessive,
                false,
                force_singular,
            );
            create_entity_ref(p1_str, None, flags, content)
        }
    }

    fn parse_verb(content: &str) -> Result<Token, String> {
        let (p1, p2_opt) = split_tag(content, '[', "Malformed verb tag")?;
        let (p1_str, force_3rd_person, _, force_singular_1) = parse_stance_prefixes(p1);

        let (subject_key, verb_part) = if let Some(p2) = p2_opt {
            reject_if(
                p1_str.is_empty(),
                "Verb tag has an empty subject key",
                content,
                '[',
            )?;
            validate_property_segments(
                p1_str,
                "Verb tag has an empty property segment",
                content,
                '[',
            )?;
            (Some(p1_str.to_lowercase()), p2)
        } else {
            if p1_str == "SB" {
                return Ok(Token::SentenceBreak);
            }
            if p1_str == "NO_SB" {
                return Ok(Token::NoSentenceBreak);
            }
            (None, p1_str)
        };

        let (actual_verb, forced_present, forced_past) =
            if let Some((base_verb, forced)) = verb_part.split_once('|') {
                reject_if(
                    base_verb.is_empty() || forced.is_empty(),
                    "Verb tag has an empty verb or forced conjugation segment",
                    content,
                    '[',
                )?;

                let (forced_present, forced_past) = parse_forced_conjugations(forced, content)?;
                (base_verb, forced_present, forced_past)
            } else {
                (verb_part, None, None)
            };

        let original_verb = actual_verb.to_string();
        let is_capitalized = is_capitalized(&original_verb);
        let lower_verb = original_verb.to_lowercase();

        if let Some(options) = crate::grammar::get_collision_options(&lower_verb) {
            let opt1 = options.first().copied().unwrap_or("unknown");
            let opt2 = options.get(1).copied().unwrap_or("unknown");
            tracing::warn!(
                "Ambiguous verb '{}' detected in template. In the past tense, it could shift to {}. \
                 To guarantee your intended meaning, annotate it with the correct past tense: [source:{}({})] or [source:{}({})]",
                original_verb,
                options.join(" or "),
                original_verb,
                opt1,
                original_verb,
                opt2
            );
        } else if lower_verb == "do" {
            tracing::warn!(
                "The verb 'do' drops entirely in the future tense when used as a helper verb (e.g., 'does not' -> 'will not'). \
                 If you are using it for negation or questions, annotate it as [source:{}(aux)] to enable this behavior.",
                original_verb
            );
        }

        if original_verb.is_empty() {
            tracing::warn!(
                "Parsed an empty verb tag in template. This will conjugate to just 's'."
            );
        }

        let flags = TagFlags::new(
            is_capitalized,
            false,
            force_3rd_person,
            false,
            false,
            force_singular_1,
        );

        Ok(Token::VerbRef {
            subject_key,
            original_verb,
            lower_verb,
            forced_present,
            forced_past,
            flags,
        })
    }
}

bitflags::bitflags! {
    /// A bitflags struct to pack multiple boolean formatting flags efficiently.
    #[derive(Clone, Copy, Debug)]
    pub struct TagFlags: u8 {
        /// A flag indicating if the entity key was capitalized (e.g. {Source}).
        const IS_CAPITALIZED = 1 << 0;
        /// A flag indicating if the builder explicitly forced the article to render (e.g. {+the:source}).
        const FORCE_ARTICLE = 1 << 1;
        /// A flag indicating if the builder explicitly forced the 3rd-person stance (e.g. {+source}).
        const FORCE_3RD_PERSON = 1 << 2;
        /// A flag indicating if the builder explicitly forced the possessive form (e.g. {source's}).
        const IS_POSSESSIVE = 1 << 3;
        /// A flag indicating if the builder explicitly disabled the anaphoric article upgrade (e.g. {!a:source}).
        const NO_SMART = 1 << 4;
        /// A flag indicating if the builder explicitly forced singular conjugation (e.g. {-source}).
        const FORCE_SINGULAR = 1 << 5;
    }
}

impl TagFlags {
    /// Creates a new `TagFlags` instance from individual boolean toggles.
    #[inline]
    #[must_use]
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn new(
        is_capitalized: bool,
        force_article: bool,
        force_3rd_person: bool,
        is_possessive: bool,
        no_smart: bool,
        force_singular: bool,
    ) -> Self {
        let mut flags = Self::empty();
        flags.set(Self::IS_CAPITALIZED, is_capitalized);
        flags.set(Self::FORCE_ARTICLE, force_article);
        flags.set(Self::FORCE_3RD_PERSON, force_3rd_person);
        flags.set(Self::IS_POSSESSIVE, is_possessive);
        flags.set(Self::NO_SMART, no_smart);
        flags.set(Self::FORCE_SINGULAR, force_singular);
        flags
    }

    /// Returns `true` if the capitalized flag is set.
    #[inline]
    #[must_use]
    pub const fn is_capitalized(self) -> bool {
        self.contains(Self::IS_CAPITALIZED)
    }

    /// Returns `true` if the forced article flag is set.
    #[inline]
    #[must_use]
    pub const fn force_article(self) -> bool {
        self.contains(Self::FORCE_ARTICLE)
    }

    /// Returns `true` if the forced 3rd-person flag is set.
    #[inline]
    #[must_use]
    pub const fn force_3rd_person(self) -> bool {
        self.contains(Self::FORCE_3RD_PERSON)
    }

    /// Returns `true` if the possessive flag is set.
    #[inline]
    #[must_use]
    pub const fn is_possessive(self) -> bool {
        self.contains(Self::IS_POSSESSIVE)
    }

    /// Returns `true` if the anaphora suppression flag is set.
    #[inline]
    #[must_use]
    pub const fn no_smart(self) -> bool {
        self.contains(Self::NO_SMART)
    }

    /// Returns `true` if the singular override flag is set.
    #[inline]
    #[must_use]
    pub const fn force_singular(self) -> bool {
        self.contains(Self::FORCE_SINGULAR)
    }
}

/// Parameters extracted from a token or fallback logic to render an entity.
struct EntityRefParams<'a> {
    key: &'a str,
    article: Option<&'a str>,
    flags: TagFlags,
    ordinal: Option<usize>,
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
        let mut template_keys = Vec::new();
        for token in &template.tokens {
            let k = match token {
                Token::EntityRef { key, .. }
                | Token::PronounRef { key, .. }
                | Token::VerbRef {
                    subject_key: Some(key),
                    ..
                } => Some(key.as_str()),
                _ => None,
            };
            if let Some(key) = k {
                if !template_keys.contains(&key) {
                    template_keys.push(key);
                }
            }
        }

        let future_keys = if ctx.lookahead {
            template_keys.clone()
        } else {
            Vec::new()
        };

        // Pre-resolve all entities that could possibly be checked during this render call
        // to avoid redundant string splitting and hash map lookups in the subsequent hot loops.
        let mut pre_resolved = Vec::new();
        for k in &template_keys {
            if let Ok(ent) = Self::get_entity(ctx, k) {
                pre_resolved.push((k.to_string(), ent));
            }
        }
        for r in ctx.recent_entities.borrow().iter() {
            if !template_keys.contains(&r.key.as_str()) {
                if let Ok(ent) = Self::get_entity(ctx, &r.key) {
                    pre_resolved.push((r.key.clone(), ent));
                }
            }
        }

        // 1. Pre-allocate buffer to prevent continuous heap allocations
        let mut raw_output = String::with_capacity(template.estimated_length);

        for token in &template.tokens {
            match token {
                Token::Literal(text) => raw_output.push_str(text),
                Token::EntityRef {
                    key,
                    article,
                    flags,
                } => Self::render_entity_ref(
                    ctx,
                    &mut raw_output,
                    &EntityRefParams {
                        key,
                        article: article.as_deref(),
                        flags: *flags,
                        ordinal: None,
                    },
                    &future_keys,
                    &pre_resolved,
                )?,
                Token::PronounRef { .. } => {
                    Self::render_pronoun_ref(ctx, &mut raw_output, token, &future_keys, &pre_resolved)?;
                }
                Token::VerbRef { .. } => Self::render_verb_ref(ctx, &mut raw_output, token, &pre_resolved)?,
                Token::SentenceBreak => {
                    raw_output.push('\u{E000}');
                }
                Token::NoSentenceBreak => raw_output.push('\u{E001}'),
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
                let next_len = current_path.len() + 1 + prop.len();
                current_path = key.get(..next_len).unwrap_or(current_path);
            }
            return Ok(current);
        }

        tracing::error!("Failed to render template: Missing entity for key '{key}'");
        Err(format!("Missing entity for key: {key}"))
    }

    #[inline]
    fn check_will_vacate(
        effective_viewer: &str,
        resolved_others: &[(&str, &dyn TemplateEntity)],
        other_key: &str,
        other_entity: &dyn TemplateEntity,
        name: &str,
        other_name: &str,
    ) -> bool {
        if let Some(other_long) = other_entity.long_display_name_for(effective_viewer)
            && other_long != name
        {
            let mut other_short_collisions = 0;
            let mut other_long_collisions = 0;
            for &(eval_key, eval_entity) in resolved_others {
                if eval_key != other_key {
                    let eval_short_name = eval_entity.display_name_for(effective_viewer);
                    if eval_short_name == other_name {
                        other_short_collisions += 1;
                    }

                    let mut is_long_col = eval_short_name == other_long;
                    if !is_long_col
                        && eval_short_name == name
                        && eval_entity
                            .long_display_name_for(effective_viewer)
                            .as_deref()
                            == Some(other_long.as_ref())
                    {
                        is_long_col = true;
                    }
                    if is_long_col {
                        other_long_collisions += 1;
                    }
                }
            }
            if other_long_collisions < other_short_collisions {
                return true;
            }
        }
        false
    }

    #[inline]
    fn resolve_display_name<'a>(
        ctx: &'a RenderContext,
        entity: &'a dyn TemplateEntity,
        key: &str,
        effective_viewer: &str,
        no_smart: bool,
        future_keys: &[&str],
        pre_resolved: &[(String, &'a dyn TemplateEntity)],
    ) -> (std::borrow::Cow<'a, str>, Option<usize>) {
        let mut name = entity.display_name_for(effective_viewer);
        let mut name_collision = false;

        if !no_smart {
            let mut short_collisions = 0;
            let mut unresolved_short_collisions = 0;

            let recent_borrow = ctx.recent_entities.borrow();
            let mut resolved_others = Vec::with_capacity(recent_borrow.len() + future_keys.len());

            for r in recent_borrow.iter() {
                if let Some(&(_, ent)) = pre_resolved.iter().find(|(k, _)| k == &r.key) {
                    resolved_others.push((r.key.as_str(), ent));
                }
            }
            for &fk in future_keys {
                if !recent_borrow.iter().any(|r| r.key == fk) {
                    if let Some(&(_, ent)) = pre_resolved.iter().find(|(k, _)| k == fk) {
                        resolved_others.push((fk, ent));
                    }
                }
            }

            for &(other_key, other_entity) in &resolved_others {
                if other_key != key {
                    let other_name = other_entity.display_name_for(effective_viewer);
                    if other_name == name {
                        short_collisions += 1;

                        // Determine if this other entity will vacate the short name by using its own long name
                        if !Self::check_will_vacate(
                            effective_viewer,
                            &resolved_others,
                            other_key,
                            other_entity,
                            name.as_ref(),
                            other_name.as_ref(),
                        ) {
                            unresolved_short_collisions += 1;
                        }
                    }
                }
            }

            name_collision = unresolved_short_collisions > 0;

            if short_collisions > 0
                && let Some(long_name) = entity.long_display_name_for(effective_viewer)
                && long_name != name
            {
                let mut long_collisions = 0;

                // Verify how many times the long name collides
                for &(other_key, other_entity) in &resolved_others {
                    if other_key != key {
                        let other_short = other_entity.display_name_for(effective_viewer);
                        let mut is_long_collision = other_short == long_name;

                        // Only consider the other entity's long name if its short name is in the exact
                        // same collision group as our entity's short name to prevent phantom collisions.
                        if !is_long_collision && other_short == name {
                            let other_long = other_entity.long_display_name_for(effective_viewer);
                            if other_long.as_deref() == Some(long_name.as_ref()) {
                                is_long_collision = true;
                            }
                        }

                        if is_long_collision {
                            long_collisions += 1;
                        }
                    }
                }

                // If the long name is strictly more specific (has fewer collisions), use it!
                if long_collisions < short_collisions {
                    name = long_name;
                    name_collision = long_collisions > 0;
                }
            }
        }

        let mut ordinal = None;

        if !no_smart {
            let mut ordinals = ctx.ordinals.borrow_mut();
            let state =
                ordinals
                    .entry(name.to_string())
                    .or_insert_with(|| crate::models::OrdinalState {
                        next_ordinal: 1,
                        members: HashMap::new(),
                    });

            if name_collision {
                let ord = *state.members.entry(key.to_string()).or_insert_with(|| {
                    let o = state.next_ordinal;
                    state.next_ordinal += 1;
                    o
                });
                ordinal = Some(ord);
            } else {
                state.members.clear();
                state.members.insert(key.to_string(), 1);
                state.next_ordinal = 2;
            }
        }

        (name, ordinal)
    }

    fn render_entity_ref<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        params: &EntityRefParams<'_>,
        future_keys: &[&str],
        pre_resolved: &[(String, &'a dyn TemplateEntity)],
    ) -> Result<(), String> {
        let entity = pre_resolved
            .iter()
            .find(|(k, _)| k == params.key)
            .map_or_else(|| Self::get_entity(ctx, params.key), |(_, ent)| Ok(*ent))?;
        let already_seen = ctx
            .recent_entities
            .borrow()
            .iter()
            .any(|r| r.key == params.key);

        let effective_viewer = effective_viewer_id(ctx, params.flags.force_3rd_person());

        let (name, ordinal) = Self::resolve_display_name(
            ctx,
            entity,
            params.key,
            effective_viewer,
            params.flags.no_smart(),
            future_keys,
            pre_resolved,
        );

        let mut article_to_use = params.article;

        if !params.flags.no_smart()
            && let Some(art) = article_to_use
            && is_indefinite_article(art)
        {
            if already_seen {
                article_to_use = Some(if art.starts_with(char::is_uppercase) {
                    "The"
                } else {
                    "the"
                });
            } else if let Some(ord) = ordinal
                && ord == 2
            {
                article_to_use = Some(if art.starts_with(char::is_uppercase) {
                    "Another"
                } else {
                    "another"
                });
            }
        }

        update_memory(&ctx.last_mentioned, params.key);
        track_recent_entity(ctx, params.key, entity);

        let active_params = EntityRefParams {
            key: params.key,
            article: article_to_use,
            flags: params.flags,
            ordinal,
        };
        let cap_whole = should_capitalize_whole_tag(&active_params);

        // --- Handle Groups / Distributed Lists ---
        if let Some(members) = entity.group_members() {
            Self::render_group_entity(
                raw_output,
                entity,
                members,
                effective_viewer,
                &active_params,
                ctx.stance,
                cap_whole,
            );
            return Ok(());
        }

        let is_plural = entity.is_plural() && !active_params.flags.force_singular();

        // --- Handle Single Entity Viewers ---
        if entity.contains_viewer(effective_viewer) {
            raw_output.push_str(viewer_name(
                ctx.stance,
                is_plural,
                active_params.flags.is_possessive(),
                cap_whole,
            ));
            return Ok(());
        }

        // --- Single Entity Fallback ---
        let mut article_printed = false;

        if let Some(resolved_art) = active_params.article.as_ref().and_then(|art| {
            resolve_article(
                art,
                &name,
                entity.is_proper_noun_for(effective_viewer),
                is_plural,
                active_params.flags.force_article(),
                active_params.ordinal,
                entity.collective_noun(),
            )
        }) {
            raw_output.push_str(resolved_art.as_ref());
            article_printed = true;
        }

        let should_cap_noun = if article_printed {
            active_params.flags.is_capitalized()
        } else {
            cap_whole
        };

        let name_cow = capitalize_cow(name, should_cap_noun);
        let name_str = name_cow.as_ref();

        raw_output.push_str(name_str);

        if active_params.flags.is_possessive() {
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
        stance: crate::models::ActorStance,
        cap_whole: bool,
    ) {
        let (viewer_entity, visible) = crate::models::partition_group(members, effective_viewer);

        let total_visible = visible.len() + usize::from(viewer_entity.is_some());
        if total_visible == 0 {
            return;
        }

        let mut ends_with_possessive_pronoun = false;
        let mut decomposed_we = false;
        let mut formatted_names = Vec::with_capacity(total_visible + 1);
        if let Some(viewer) = viewer_entity {
            if stance == crate::models::ActorStance::SecondPerson {
                if params.flags.is_possessive() {
                    formatted_names.push(std::borrow::Cow::Borrowed("your"));
                    if visible.is_empty() {
                        ends_with_possessive_pronoun = true;
                    }
                } else {
                    formatted_names.push(std::borrow::Cow::Borrowed("you"));
                }
            } else if stance == crate::models::ActorStance::FirstPerson && viewer.is_plural() {
                if visible.is_empty() {
                    if params.flags.is_possessive() {
                        formatted_names.push(std::borrow::Cow::Borrowed("our"));
                        ends_with_possessive_pronoun = true;
                    } else {
                        formatted_names.push(std::borrow::Cow::Borrowed("we"));
                    }
                } else {
                    decomposed_we = true;
                    if params.flags.is_possessive() {
                        formatted_names.push(std::borrow::Cow::Borrowed("your"));
                    } else {
                        formatted_names.push(std::borrow::Cow::Borrowed("you"));
                    }
                }
            }
        }

        let will_append_my = viewer_entity.is_some_and(|viewer| {
            stance == crate::models::ActorStance::FirstPerson
                && (!viewer.is_plural() || decomposed_we)
        });

        let distribute_possessives = viewer_entity.is_some() && params.flags.is_possessive();

        let mut first_visible_item = viewer_entity.is_none();
        for (member, name) in visible {
            let lower_article_storage: String;
            let article_to_use = if first_visible_item {
                params.article
            } else if let Some(art) = params.article {
                lower_article_storage = art.to_lowercase();
                Some(lower_article_storage.as_str())
            } else {
                None
            };

            let mut final_name = if let Some(resolved_art) =
                article_to_use.as_ref().and_then(|art| {
                    resolve_article(
                        art,
                        &name,
                        member.is_proper_noun_for(effective_viewer),
                        member.is_plural(),
                        params.flags.force_article(),
                        params.ordinal,
                        entity.collective_noun(),
                    )
                }) {
                std::borrow::Cow::Owned(format!("{}{name}", resolved_art.as_ref()))
            } else {
                name
            };

            if distribute_possessives {
                let suffix = Self::get_possessive_suffix(&final_name, member.is_plural());
                let mut owned = final_name.into_owned();
                owned.push_str(suffix);
                final_name = std::borrow::Cow::Owned(owned);
            }

            formatted_names.push(final_name);
            first_visible_item = false;
        }

        if will_append_my {
            if params.flags.is_possessive() {
                formatted_names.push(std::borrow::Cow::Borrowed("my"));
                ends_with_possessive_pronoun = true;
            } else {
                formatted_names.push(std::borrow::Cow::Borrowed("I"));
            }
        }

        let list_str = crate::grammar::format_oxford_list(formatted_names);

        let mut final_str = list_str.into_owned();
        if params.flags.is_possessive() && !ends_with_possessive_pronoun && !distribute_possessives
        {
            final_str.push_str(Self::get_possessive_suffix(&final_str, entity.is_plural()));
        }

        push_capitalized_if(raw_output, &final_str, cap_whole);
    }

    #[inline]
    fn get_last_visible_char(input: &str) -> Option<char> {
        #[cfg(not(any(feature = "mxp", feature = "msp", feature = "ansi")))]
        {
            input.trim_end().chars().next_back()
        }

        #[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
        {
            // Fast-path: If no protocol triggers exist, use Rust's native reverse iterator.
            // (SIMD pre-scan is highly optimized, and next_back() evaluates from the end).
            if !has_protocol_tags(input) {
                return input.trim_end().chars().next_back();
            }

            let mut chars = input.char_indices().peekable();
            let mut last_visible = None;

            while let Some(&(i, c)) = chars.peek() {
                let remainder = &input[i..];
                if skip_protocol_tags(&mut chars, remainder, i).is_some() {
                    continue;
                }

                chars.next();
                if !c.is_whitespace() {
                    last_visible = Some(c);
                }
            }

            last_visible
        }
    }

    #[inline]
    fn get_possessive_suffix(name: &str, is_plural: bool) -> &'static str {
        if matches!(Self::get_last_visible_char(name), Some('s' | 'S')) && is_plural {
            return "'";
        }
        "'s"
    }

    fn render_pronoun_ref<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        token: &Token,
        future_keys: &[&str],
        pre_resolved: &[(String, &'a dyn TemplateEntity)],
    ) -> Result<(), String> {
        let Token::PronounRef { key, p_type, flags } = token else {
            return Ok(());
        };

        let entity = pre_resolved
            .iter()
            .find(|(k, _)| k == key)
            .map_or_else(|| Self::get_entity(ctx, key), |(_, ent)| Ok(*ent))?;
        let effective_viewer = effective_viewer_id(ctx, flags.force_3rd_person());

        let is_viewer = entity.contains_viewer(effective_viewer);
        let mut is_plural = entity.is_plural();
        let mut effective_gender = entity.gender();

        if flags.force_singular() {
            is_plural = false;
            if effective_gender == crate::models::Gender::Plural {
                effective_gender = crate::models::Gender::Neutral;
            }
        }

        let is_active_subject = ctx.active_subject.borrow().as_deref() == Some(key.as_str());

        // Check if this entity has been introduced to the narrative context yet.
        let already_seen = ctx.recent_entities.borrow().iter().any(|r| r.key == *key);

        let is_reflexive = p_type == "reflex";

        // 1. Unambiguous Contexts:
        // - Active Subject: English speakers naturally bind pronouns to the subject.
        // - Viewer: "you" is never ambiguous with 3rd-person pronouns.
        // - Reflexive: "himself" unequivocally binds to the current actor/subject.
        let mut can_use_pronoun =
            is_active_subject || is_viewer || is_reflexive || flags.no_smart();

        // 2. Disambiguation Check:
        // If the entity is a general object/target, we must ensure no other recently
        // mentioned entities share the same pronoun, which would confuse the reader.
        if !can_use_pronoun && already_seen {
            let mut ambiguous = false;
            for other in ctx.recent_entities.borrow().iter() {
                if other.key != *key {
                    let other_is_viewer = if flags.force_3rd_person()
                        || ctx.stance == crate::models::ActorStance::ThirdPerson
                    {
                        other
                            .flags
                            .contains(crate::models::RecentEntityFlags::IS_VIEWER_FORCED)
                    } else {
                        other
                            .flags
                            .contains(crate::models::RecentEntityFlags::IS_VIEWER_NORMAL)
                    };

                    // If another character isn't the viewer ("you") but shares the exact
                    // same gender and plurality, a pronoun like "he" or "they" is ambiguous.
                    if !other_is_viewer
                        && effective_gender == other.gender
                        && is_plural
                            == other
                                .flags
                                .contains(crate::models::RecentEntityFlags::IS_PLURAL)
                    {
                        ambiguous = true;
                        break;
                    }
                }
            }

            // If no collisions were found in the recent memory, it's safe to use the pronoun.
            if !ambiguous {
                can_use_pronoun = true;
            }
        }

        if can_use_pronoun {
            if !already_seen {
                update_memory(&ctx.last_mentioned, key);
                track_recent_entity(ctx, key, entity);
            }

            let pronoun =
                resolve_pronoun(effective_gender, p_type, is_viewer, is_plural, ctx.stance)?;
            push_capitalized_if(raw_output, pronoun, flags.is_capitalized());
        } else {
            // Smart Anaphora Resolution: The entity hasn't been introduced yet, or a pronoun would be ambiguous!
            // Evaluate it as if the builder had written `{a:key}` instead. This allows the engine
            // to naturally upgrade it to `{the:key}` if it is a unique subsequent mention!
            let is_possessive = p_type == "poss" || p_type == "abs_poss";

            let fallback_article = if flags.force_singular() && entity.is_plural() {
                if flags.is_capitalized() {
                    "One of the"
                } else {
                    "one of the"
                }
            } else {
                if flags.is_capitalized() { "A" } else { "a" }
            };

            let fallback_params = EntityRefParams {
                key,
                article: Some(fallback_article),
                // We set `is_capitalized: false` here because the capitalization requested by the pronoun
                // (e.g. `{target:Subj}`) applies to the *first word* of the substitution (the article "A").
                // We do not want to force-capitalize common nouns (yielding "A Goblin" instead of "A goblin").
                // Proper nouns (like "Aldran") naturally return capitalized strings and are unaffected.
                flags: TagFlags::new(
                    false,
                    false,
                    flags.force_3rd_person(),
                    is_possessive,
                    false,
                    flags.force_singular(),
                ),
                ordinal: None,
            };
            Self::render_entity_ref(ctx, raw_output, &fallback_params, future_keys, pre_resolved)?;
        }
        Ok(())
    }

    fn render_verb_ref<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        token: &Token,
        pre_resolved: &[(String, &'a dyn TemplateEntity)],
    ) -> Result<(), String> {
        let Token::VerbRef {
            subject_key,
            original_verb,
            lower_verb,
            forced_present,
            forced_past,
            flags,
        } = token
        else {
            return Ok(());
        };

        // Explicitly bind the verb to its subject to solve passive voice / compound subjects
        let (is_viewer, mut is_plural) = if let Some(key) = subject_key {
            let entity = pre_resolved
                .iter()
                .find(|(k, _)| k == key)
                .map_or_else(|| Self::get_entity(ctx, key), |(_, ent)| Ok(*ent))?;
            let effective_viewer = effective_viewer_id(ctx, flags.force_3rd_person());
            update_memory(&ctx.active_subject, key);
            update_memory(&ctx.last_mentioned, key);
            track_recent_entity(ctx, key, entity);
            (entity.contains_viewer(effective_viewer), entity.is_plural())
        } else {
            // Safe default to 3rd-person singular if no subject is bound
            (false, false)
        };

        if flags.force_singular() {
            is_plural = false;
        }

        let forced_conjugation = match ctx.tense {
            crate::models::Tense::Present => forced_present.as_ref(),
            crate::models::Tense::Past => forced_past.as_ref(),
            crate::models::Tense::Future => None,
        };

        let conjugated = if let Some(forced) = forced_conjugation {
            // Note: `forced` only contains the override segments for the requested tense.
            let forced_str = match forced.as_slice() {
                [first, second] => {
                    if !is_viewer && !is_plural {
                        second
                    } else {
                        first
                    }
                }
                [first, second, third] => {
                    if is_viewer
                        && ctx.stance == crate::models::ActorStance::FirstPerson
                        && !is_plural
                    {
                        first
                    } else if !is_viewer && !is_plural {
                        third
                    } else {
                        second
                    }
                }
                [first, ..] => first,
                [] => original_verb,
            };
            crate::grammar::format_verb(forced_str, flags.is_capitalized())
        } else {
            conjugate_verb(
                original_verb,
                lower_verb,
                flags.is_capitalized(),
                is_viewer,
                is_plural,
                ctx.stance,
                ctx.tense,
            )
        };
        raw_output.push_str(&conjugated);
        Ok(())
    }

    /// Segments the text by true sentence boundaries and capitalizes the first letter.
    fn post_process_typography(input: &str) -> String {
        const SENTENCE_BREAK_SENTINEL: char = '\u{E000}';
        const NO_SENTENCE_BREAK_SENTINEL: char = '\u{E001}';

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
                // Otherwise, crossing them will falsely trigger the capitalization flag
                // on the next visible word.
                if let Some(&(curr_i, _)) = chars.peek() {
                    catch_up_bounds(curr_i, &mut next_sentence_start);
                }

                // If the last real character was a sentence terminator, the tag might have
                // hidden the sentence boundary from the unicode segmenter. Force a reset.
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

/// Attempts to identify and skip over a protocol tag starting at the current position.
///
/// **Optimization Rationale:** This function evaluates state machines for ANSI, MXP, and MSP tags.
/// To avoid creating string slices (`&str[i..]`) and executing this matching logic on every
/// single character of the hot loop, this function should only be called if a prior
/// `has_protocol_tags` SIMD pre-scan confirmed that the string actually contains tags.
#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
#[inline]
fn skip_protocol_tags(
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
/// stripped string alongside the boolean flags for `force_3rd_person`/`force_article`, `no_smart`, and `force_singular`.
#[inline]
fn parse_stance_prefixes(mut s: &str) -> (&str, bool, bool, bool) {
    let mut force_3rd_person = false;
    let mut no_smart = false;
    let mut force_singular = false;
    loop {
        if let Some(stripped) = s.strip_prefix('+') {
            force_3rd_person = true;
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix('!') {
            no_smart = true;
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix('-') {
            force_singular = true;
            s = stripped;
        } else {
            break;
        }
    }
    (s, force_3rd_person, no_smart, force_singular)
}

#[inline]
fn parse_entity_modifiers(s: &str) -> (&str, bool, bool, bool, bool) {
    let (s, force_3rd_person, no_smart, force_singular) = parse_stance_prefixes(s);
    let (s, is_possessive) = parse_possessive_suffix(s);
    (s, force_3rd_person, no_smart, force_singular, is_possessive)
}

type ParsedForcedConjugations = (Option<Vec<String>>, Option<Vec<String>>);

#[inline]
fn create_pronoun_ref(p1: &str, p2: &str, content: &str) -> Result<Token, String> {
    let (p1_str, force_3rd_person, no_smart_entity, force_singular_1, _) =
        parse_entity_modifiers(p1);
    let (p2_str, _, force_pronoun, force_singular_2) = parse_stance_prefixes(p2);

    reject_if(
        p1_str.is_empty() || p2_str.is_empty(),
        "Pronoun tag has an empty key or type",
        content,
        '{',
    )?;
    validate_property_segments(
        p1_str,
        "Pronoun tag has an empty property segment",
        content,
        '{',
    )?;
    let flags = TagFlags::new(
        is_capitalized(p2_str),
        false,
        force_3rd_person,
        false,
        force_pronoun || no_smart_entity,
        force_singular_1 || force_singular_2,
    );
    Ok(Token::PronounRef {
        key: p1_str.to_lowercase(),
        p_type: p2_str.to_lowercase(),
        flags,
    })
}

#[inline]
fn parse_forced_conjugations(
    forced: &str,
    content: &str,
) -> Result<ParsedForcedConjugations, String> {
    let mut forced_present = None;
    let mut forced_past = None;

    let (pres_str, past_str) =
        if let Some((present_overrides, past_overrides)) = forced.split_once(';') {
            (present_overrides, Some(past_overrides))
        } else {
            (forced, None)
        };

    if !pres_str.is_empty() {
        let parts: Vec<String> = pres_str.split('|').map(str::to_string).collect();
        for part in &parts {
            reject_if(
                part.is_empty(),
                "Verb tag has an empty forced present conjugation segment",
                content,
                '[',
            )?;
        }
        reject_if(
            parts.len() > 3,
            "Verb tag has too many forced present conjugation segments",
            content,
            '[',
        )?;
        forced_present = Some(parts);
    }

    if let Some(past_overrides_str) = past_str {
        reject_if(
            past_overrides_str.is_empty(),
            "Verb tag has an empty forced past conjugation segment",
            content,
            '[',
        )?;
        let parts: Vec<String> = past_overrides_str.split('|').map(str::to_string).collect();
        for part in &parts {
            reject_if(
                part.is_empty(),
                "Verb tag has an empty forced past conjugation segment",
                content,
                '[',
            )?;
        }
        reject_if(
            parts.len() > 3,
            "Verb tag has too many forced past conjugation segments",
            content,
            '[',
        )?;
        forced_past = Some(parts);
    }

    Ok((forced_present, forced_past))
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
    reject_if(parts.next().is_some(), malformed_msg, content, open_char)?;
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
fn track_recent_entity(ctx: &RenderContext<'_>, key: &str, entity: &dyn TemplateEntity) {
    let mut recents = ctx.recent_entities.borrow_mut();

    // Move to the back to represent the most recently used (LRU)
    if let Some(pos) = recents.iter().position(|r| r.key == key) {
        let item = recents.remove(pos);
        recents.push(item);
    } else {
        let mut flags = crate::models::RecentEntityFlags::empty();
        flags.set(
            crate::models::RecentEntityFlags::IS_PLURAL,
            entity.is_plural(),
        );
        flags.set(
            crate::models::RecentEntityFlags::IS_VIEWER_NORMAL,
            entity.contains_viewer(ctx.viewer_id),
        );
        flags.set(
            crate::models::RecentEntityFlags::IS_VIEWER_FORCED,
            entity.contains_viewer(crate::models::NULL_VIEWER),
        );

        recents.push(crate::models::RecentEntity {
            key: key.to_string(),
            gender: entity.gender(),
            flags,
        });
    }

    // Enforce the anaphora memory capacity limit
    let mut last_mentioned = ctx.last_mentioned.borrow_mut();
    let mut active_subject = ctx.active_subject.borrow_mut();
    crate::models::enforce_anaphora_limit(
        ctx.anaphora_limit,
        &mut recents,
        &mut last_mentioned,
        &mut active_subject,
    );
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
    if end > start
        && let Some(slice) = raw.get(start..end)
    {
        tokens.push(Token::Literal(slice.to_string()));
    }
}

#[inline]
fn is_article(s: &str) -> bool {
    s.eq_ignore_ascii_case("a")
        || s.eq_ignore_ascii_case("an")
        || s.eq_ignore_ascii_case("the")
        || s.eq_ignore_ascii_case("this")
        || s.eq_ignore_ascii_case("that")
        || s.eq_ignore_ascii_case("another")
        || s.eq_ignore_ascii_case("one")
        || s.eq_ignore_ascii_case("one of")
        || s.eq_ignore_ascii_case("one of the")
        || s.eq_ignore_ascii_case("some")
}

#[inline]
fn is_indefinite_article(s: &str) -> bool {
    s.eq_ignore_ascii_case("a") || s.eq_ignore_ascii_case("an")
}

#[inline]
fn should_capitalize_whole_tag(params: &EntityRefParams<'_>) -> bool {
    params.flags.is_capitalized() || params.article.is_some_and(is_capitalized)
}

#[inline]
fn create_entity_ref(
    key: &str,
    article: Option<&str>,
    flags: TagFlags,
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
        flags,
    })
}

#[inline]
const fn viewer_name(
    stance: crate::models::ActorStance,
    is_plural: bool,
    is_possessive: bool,
    is_capitalized: bool,
) -> &'static str {
    match stance {
        crate::models::ActorStance::FirstPerson => match (is_plural, is_possessive, is_capitalized)
        {
            (false, true, true) => "My",
            (false, true, false) => "my",
            (false, false, _) => "I",
            (true, true, true) => "Our",
            (true, true, false) => "our",
            (true, false, true) => "We",
            (true, false, false) => "we",
        },
        crate::models::ActorStance::SecondPerson => match (is_possessive, is_capitalized) {
            (true, true) => "Your",
            (true, false) => "your",
            (false, true) => "You",
            (false, false) => "you",
        },
        crate::models::ActorStance::ThirdPerson => "",
    }
}

#[inline]
fn is_capitalized(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_uppercase)
}

#[inline]
fn reject_if(
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

/// Performs a highly optimized SIMD pre-scan to detect the presence of protocol triggers.
///
/// **Optimization Rationale:** By running this once before iterating over a string's characters,
/// the engine can completely bypass the overhead of slicing and tag validation inside the hot loop
/// for the vast majority of strings that contain pure text.
#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
#[inline]
fn has_protocol_tags(input: &str) -> bool {
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

#[inline]
fn validate_property_segments(
    path: &str,
    error_msg: &str,
    content: &str,
    open_char: char,
) -> Result<(), String> {
    reject_if(
        path.split('.').any(str::is_empty),
        error_msg,
        content,
        open_char,
    )
}

#[inline]
fn effective_viewer_id<'a>(ctx: &RenderContext<'a>, force_3rd_person: bool) -> &'a str {
    if force_3rd_person || ctx.stance == crate::models::ActorStance::ThirdPerson {
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
