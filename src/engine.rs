use crate::grammar::{conjugate_verb, resolve_article, resolve_pronoun};
use crate::models::{NULL_VIEWER, RenderContext, TemplateEntity};
use std::collections::{HashMap, HashSet};
use unicode_segmentation::UnicodeSegmentation;

const SENTENCE_BREAK_SENTINEL: char = '\u{E000}';
const NO_SENTENCE_BREAK_SENTINEL: char = '\u{E001}';

// --- Template Syntax Constants ---
const TAG_ENTITY_OPEN: char = '{';
const TAG_ENTITY_CLOSE: char = '}';
const TAG_VERB_OPEN: char = '[';
const TAG_VERB_CLOSE: char = ']';
const TAG_SEPARATOR: char = ':';
const TAG_PROPERTY_SEP: char = '.';
const TAG_ESCAPE: char = '\\';

const VERB_TENSE_SEP: char = ';';
const VERB_FORM_SEP: char = '|';

const MOD_FORCE_3RD_PERSON: char = '+';
const MOD_NO_SMART: char = '!';
const MOD_FORCE_SINGULAR: char = '-';
const MOD_PREFER_NOUN: char = '*';
const MOD_ALLOW_AMBIGUOUS_YOU: char = '~';
const MOD_EXTRACT_GROUP_MEMBER: char = '^';
const MOD_POSSESSIVE: &str = "'s";

const CTRL_SENTENCE_BREAK: &str = "SB";
const CTRL_NO_SENTENCE_BREAK: &str = "NO_SB";

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
        /// The optional type of pronoun requested (e.g., `"subj"`, `"obj"`, `"poss"`, `"abs_poss"`, `"reflex"`).
        p_type: Option<String>,
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
    /// The unique entity keys referenced in the template, in order of first appearance.
    pub template_keys: Vec<String>,
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

            if c == TAG_ESCAPE {
                chars.next();
                if let Some(&(next_i, next_c)) = chars.peek()
                    && is_escapable_char(next_c)
                {
                    push_literal(&mut tokens, raw, last_literal_start, i);
                    last_literal_start = next_i;
                    chars.next();
                }
                continue;
            }

            if c == TAG_ENTITY_OPEN || c == TAG_VERB_OPEN {
                // Push any preceding literal text
                push_literal(&mut tokens, raw, last_literal_start, i);
                chars.next(); // Consume the opening brace or bracket

                let is_entity = c == TAG_ENTITY_OPEN;
                let close_char = if is_entity {
                    TAG_ENTITY_CLOSE
                } else {
                    TAG_VERB_CLOSE
                };
                let tag_name = if is_entity { "entity tag" } else { "verb tag" };

                let end_idx = consume_until_closed(&mut chars, i, close_char, tag_name)?;
                let content = raw.get(i + 1..end_idx).unwrap_or_default();

                let token = if is_entity {
                    Self::parse_entity(content)?
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

        let mut template_keys = Vec::new();
        let mut seen_keys = HashSet::new();
        for token in &tokens {
            let k = match token {
                Token::EntityRef { key, .. }
                | Token::VerbRef {
                    subject_key: Some(key),
                    ..
                } => Some(key.as_str()),
                _ => None,
            };
            if let Some(key) = k
                && seen_keys.insert(key)
            {
                template_keys.push(key.to_string());
            }
        }

        Ok(Template {
            tokens,
            template_keys,
            estimated_length: raw.len() + (raw.len() / 5),
        })
    }

    #[inline]
    fn destructure_entity_tag<'a>(
        parts: &'a [&'a str],
        content: &str,
    ) -> Result<(Option<&'a str>, &'a str, Option<&'a str>), String> {
        match parts {
            [p1, p2, p3] => {
                let (p1_clean, _) = parse_stance_prefixes(p1);
                if !p1_clean.is_empty() && !is_article(p1_clean) {
                    return Err(validation_error(
                        "Malformed entity tag",
                        content,
                        TAG_ENTITY_OPEN,
                    ));
                }
                let article = if p1.is_empty() { None } else { Some(*p1) };
                Ok((article, *p2, Some(*p3)))
            }
            [p1, p2] => {
                let (p1_clean, _) = parse_stance_prefixes(p1);
                if !p1_clean.is_empty() && is_article(p1_clean) {
                    Ok((Some(*p1), *p2, None))
                } else {
                    Ok((None, *p1, Some(*p2)))
                }
            }
            [p1] => Ok((None, *p1, None)),
            _ => Err(validation_error(
                "Malformed entity tag",
                content,
                TAG_ENTITY_OPEN,
            )),
        }
    }

    fn parse_entity(content: &str) -> Result<Token, String> {
        let has_letters = content.chars().any(char::is_alphabetic);
        let is_all_caps = has_letters
            && content
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(char::is_uppercase);

        let parts: Vec<&str> = content.split(TAG_SEPARATOR).map(str::trim).collect();
        reject_if(
            parts.is_empty() || parts.len() > 3,
            "Malformed entity tag",
            content,
            TAG_ENTITY_OPEN,
        )?;

        let (mut raw_article, raw_key, mut raw_p_type) =
            Self::destructure_entity_tag(&parts, content)?;

        let mut flags = TagFlags::empty();

        if let Some(art) = raw_article {
            let (clean_art, mut art_flags) = parse_stance_prefixes(art);
            if art_flags.contains(TagFlags::FORCE_3RD_PERSON) {
                art_flags.insert(TagFlags::FORCE_ARTICLE);
            }
            flags |= art_flags;
            flags.set(
                TagFlags::ARTICLE_INDEFINITE,
                is_indefinite_article(clean_art),
            );
            flags.set(TagFlags::ARTICLE_CAPITALIZED, is_capitalized(clean_art));
            raw_article = Some(clean_art);
        }

        let (clean_key, key_flags) = parse_stance_prefixes(raw_key);
        let (clean_key, is_possessive) = parse_possessive_suffix(clean_key);

        if raw_article.is_some() && clean_key.is_empty() {
            return Err(validation_error(
                "Entity tag has an article but an empty key",
                content,
                TAG_ENTITY_OPEN,
            ));
        }
        if raw_p_type.is_some() && clean_key.is_empty() {
            return Err(validation_error(
                "Pronoun tag has an empty key or type",
                content,
                TAG_ENTITY_OPEN,
            ));
        }
        reject_if(
            clean_key.is_empty(),
            "Entity tag has an empty key",
            content,
            TAG_ENTITY_OPEN,
        )?;
        validate_property_segments(
            clean_key,
            "Entity tag has an empty property segment",
            content,
            TAG_ENTITY_OPEN,
        )?;

        flags |= key_flags;
        flags.set(TagFlags::IS_POSSESSIVE, is_possessive);

        flags.set(TagFlags::IS_CAPITALIZED, is_capitalized(clean_key));

        if let Some(pt) = raw_p_type {
            let (clean_pt, pt_flags) = parse_stance_prefixes(pt);
            reject_if(
                clean_pt.is_empty(),
                "Pronoun tag has an empty key or type",
                content,
                TAG_ENTITY_OPEN,
            )?;
            flags |= pt_flags;
            flags.set(TagFlags::PRONOUN_CAPITALIZED, is_capitalized(clean_pt));
            raw_p_type = Some(clean_pt);
        }

        flags.set(TagFlags::ALL_CAPS, is_all_caps);

        Ok(Token::EntityRef {
            key: clean_key.to_lowercase(),
            article: raw_article.map(ToString::to_string),
            p_type: raw_p_type.map(str::to_lowercase),
            flags,
        })
    }

    fn parse_verb(content: &str) -> Result<Token, String> {
        let has_letters = content.chars().any(char::is_alphabetic);
        let is_all_caps = has_letters
            && content
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(char::is_uppercase);

        let (p1, p2_opt) = split_tag(content, TAG_VERB_OPEN, "Malformed verb tag")?;
        let (p1_str, p1_flags) = parse_stance_prefixes(p1);

        let (subject_key, verb_part) = if let Some(p2) = p2_opt {
            reject_if(
                p1_str.is_empty(),
                "Verb tag has an empty subject key",
                content,
                TAG_VERB_OPEN,
            )?;
            validate_property_segments(
                p1_str,
                "Verb tag has an empty property segment",
                content,
                TAG_VERB_OPEN,
            )?;
            (Some(p1_str.to_lowercase()), p2)
        } else {
            if p1_str == CTRL_SENTENCE_BREAK {
                return Ok(Token::SentenceBreak);
            }
            if p1_str == CTRL_NO_SENTENCE_BREAK {
                return Ok(Token::NoSentenceBreak);
            }
            (None, p1_str)
        };

        let (actual_verb, forced_present, forced_past) =
            if let Some((base_verb, forced)) = verb_part.split_once(VERB_FORM_SEP) {
                reject_if(
                    base_verb.trim().is_empty() || forced.trim().is_empty(),
                    "Verb tag has an empty verb or forced conjugation segment",
                    content,
                    TAG_VERB_OPEN,
                )?;

                let (forced_present, forced_past) = parse_forced_conjugations(forced, content)?;
                (base_verb.trim(), forced_present, forced_past)
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

        let mut flags = p1_flags;
        flags.set(TagFlags::IS_CAPITALIZED, is_capitalized);
        flags.set(TagFlags::ALL_CAPS, is_all_caps);

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
    pub struct TagFlags: u16 {
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
        /// A flag indicating if the article provided is an indefinite article (e.g. "a", "an").
        const ARTICLE_INDEFINITE = 1 << 6;
        /// A flag indicating if the article provided was capitalized (e.g. {A:source}).
        const ARTICLE_CAPITALIZED = 1 << 7;
        /// A flag indicating the builder used the Safe Override (*) to prefer nouns over pronouns.
        const PREFER_NOUN = 1 << 8;
        /// A flag indicating the builder used the Safe Override (~) to allow the ambiguous plural "you" fallback.
        const ALLOW_AMBIGUOUS_YOU = 1 << 9;
        /// A flag indicating the builder used the Safe Override (^) to extract an unspecified member from a group.
        const EXTRACT_GROUP_MEMBER = 1 << 10;
        /// A flag indicating the pronoun requested was capitalized.
        const PRONOUN_CAPITALIZED = 1 << 11;
        /// A flag indicating the entire tag was written in uppercase, activating ALL CAPS mode.
        const ALL_CAPS = 1 << 12;
    }
}

impl TagFlags {
    /// Creates a new `TagFlags` instance from individual boolean toggles.
    #[inline]
    #[must_use]
    #[allow(clippy::fn_params_excessive_bools)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        is_capitalized: bool,
        force_article: bool,
        force_3rd_person: bool,
        is_possessive: bool,
        no_smart: bool,
        force_singular: bool,
        article_indefinite: bool,
        article_capitalized: bool,
        prefer_noun: bool,
        allow_ambiguous_you: bool,
        extract_group_member: bool,
        pronoun_capitalized: bool,
        all_caps: bool,
    ) -> Self {
        let mut flags = Self::empty();
        flags.set(Self::IS_CAPITALIZED, is_capitalized);
        flags.set(Self::FORCE_ARTICLE, force_article);
        flags.set(Self::FORCE_3RD_PERSON, force_3rd_person);
        flags.set(Self::IS_POSSESSIVE, is_possessive);
        flags.set(Self::NO_SMART, no_smart);
        flags.set(Self::FORCE_SINGULAR, force_singular);
        flags.set(Self::ARTICLE_INDEFINITE, article_indefinite);
        flags.set(Self::ARTICLE_CAPITALIZED, article_capitalized);
        flags.set(Self::PREFER_NOUN, prefer_noun);
        flags.set(Self::ALLOW_AMBIGUOUS_YOU, allow_ambiguous_you);
        flags.set(Self::EXTRACT_GROUP_MEMBER, extract_group_member);
        flags.set(Self::PRONOUN_CAPITALIZED, pronoun_capitalized);
        flags.set(Self::ALL_CAPS, all_caps);
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

    /// Returns `true` if the article provided is indefinite.
    #[inline]
    #[must_use]
    pub const fn article_indefinite(self) -> bool {
        self.contains(Self::ARTICLE_INDEFINITE)
    }

    /// Returns `true` if the article provided was capitalized.
    #[inline]
    #[must_use]
    pub const fn article_capitalized(self) -> bool {
        self.contains(Self::ARTICLE_CAPITALIZED)
    }

    /// Returns `true` if the safe noun override flag is set.
    #[inline]
    #[must_use]
    pub const fn prefer_noun(self) -> bool {
        self.contains(Self::PREFER_NOUN)
    }

    /// Returns `true` if the ambiguous plural you override flag is set.
    #[inline]
    #[must_use]
    pub const fn allow_ambiguous_you(self) -> bool {
        self.contains(Self::ALLOW_AMBIGUOUS_YOU)
    }

    /// Returns `true` if the extract group member override flag is set.
    #[inline]
    #[must_use]
    pub const fn extract_group_member(self) -> bool {
        self.contains(Self::EXTRACT_GROUP_MEMBER)
    }
}

/// Parameters extracted from a token or fallback logic to render an entity.
struct EntityRefParams<'a> {
    key: &'a str,
    article: Option<&'a str>,
    p_type: Option<&'a str>,
    flags: TagFlags,
    ordinal: Option<usize>,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct GroupMemberFlags: u8 {
        const AFTER_POSSESSIVE       = 1 << 0;
        const FIRST_VISIBLE_ITEM     = 1 << 1;
        const DISTRIBUTE_POSSESSIVES = 1 << 2;
        const IS_REFLEXIVE           = 1 << 3;
    }
}

/// Configuration for formatting a group member.
struct GroupMemberFormatConfig<'a> {
    flags: GroupMemberFlags,
    article_to_use: Option<&'a str>,
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
        let future_keys: Vec<&str> = if ctx.lookahead {
            template.template_keys.iter().map(String::as_str).collect()
        } else {
            Vec::new()
        };

        // Pre-resolve all entities that could possibly be checked during this render call
        // to avoid redundant string splitting and hash map lookups in the subsequent hot loops.
        //
        // WARNING: Do NOT attempt to pre-calculate an exact `collision_candidates` array here.
        // The anaphora memory (`recent_entities`) is dynamic and accumulates state left-to-right
        // as the template is rendered. Freezing the collision candidates at the start of the
        // render call causes the engine to fail to detect collisions with entities introduced
        // sequentially within the same template, breaking indefinite article upgrades ("Another",
        // "Other") and long description disambiguation.
        let mut pre_resolved: HashMap<&str, &dyn TemplateEntity> = HashMap::new();
        for k in &template.template_keys {
            if let Ok(ent) = Self::get_entity(ctx, k) {
                pre_resolved.insert(k.as_str(), ent);
            }
        }

        // 1. Pre-allocate buffer to prevent continuous heap allocations
        let mut raw_output = String::with_capacity(template.estimated_length);

        for token in &template.tokens {
            let start_len = raw_output.len();
            let mut all_caps = false;

            match token {
                Token::Literal(text) => raw_output.push_str(text),
                Token::EntityRef {
                    key,
                    article,
                    p_type,
                    flags,
                } => {
                    all_caps = flags.contains(TagFlags::ALL_CAPS);
                    Self::render_entity_ref(
                        ctx,
                        &mut raw_output,
                        &EntityRefParams {
                            key,
                            article: article.as_deref(),
                            p_type: p_type.as_deref(),
                            flags: *flags,
                            ordinal: None,
                        },
                        &future_keys,
                        &pre_resolved,
                    )?;
                }
                Token::VerbRef { .. } => {
                    if let Token::VerbRef { flags, .. } = token {
                        all_caps = flags.contains(TagFlags::ALL_CAPS);
                    }
                    Self::render_verb_ref(ctx, &mut raw_output, token, &pre_resolved)?;
                }
                Token::SentenceBreak => {
                    raw_output.push(SENTENCE_BREAK_SENTINEL);
                }
                Token::NoSentenceBreak => raw_output.push(NO_SENTENCE_BREAK_SENTINEL),
            }

            if all_caps && raw_output.len() > start_len {
                let upper = raw_output[start_len..].to_uppercase();
                raw_output.truncate(start_len);
                raw_output.push_str(&upper);
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
        if let Some((root_key, remainder)) = key.split_once(TAG_PROPERTY_SEP)
            && let Some(mut current) = ctx.entities.get(root_key).copied()
        {
            let mut current_path = root_key;
            for prop in remainder.split(TAG_PROPERTY_SEP) {
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
    #[allow(clippy::too_many_arguments)]
    fn check_will_vacate(
        ctx: &RenderContext,
        effective_viewer: &str,
        recent_borrow: &[crate::models::RecentEntity],
        future_keys: &[&str],
        pre_resolved: &HashMap<&str, &dyn TemplateEntity>,
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

            let mut check_entity = |eval_key: &str, eval_entity: &dyn TemplateEntity| {
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
            };

            for r in recent_borrow {
                if let Ok(eval_entity) = Self::get_entity(ctx, &r.key) {
                    check_entity(&r.key, eval_entity);
                }
            }
            for &fk in future_keys {
                if !recent_borrow.iter().any(|r| r.key == fk)
                    && let Some(&eval_entity) = pre_resolved.get(fk)
                {
                    check_entity(fk, eval_entity);
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
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
    ) -> (std::borrow::Cow<'a, str>, Option<usize>) {
        let mut name = entity.display_name_for(effective_viewer);
        let mut name_collision = false;

        if !no_smart {
            let mut short_collisions = 0;
            let mut unresolved_short_collisions = 0;
            let recent_borrow = ctx.recent_entities.borrow();

            // We use a closure here to iterate over the live `recent_borrow` and `future_keys`.
            // Evaluating collisions dynamically avoids allocations while ensuring we accurately
            // catch entities that were just introduced in this template, maintaining strict
            // left-to-right chronological accuracy.
            let mut check_collision = |other_key: &str, other_entity: &'a dyn TemplateEntity| {
                if other_key != key && other_entity.display_name_for(effective_viewer) == name {
                    short_collisions += 1;

                    // Determine if this other entity will vacate the short name by using its own long name
                    if !Self::check_will_vacate(
                        ctx,
                        effective_viewer,
                        &recent_borrow,
                        future_keys,
                        pre_resolved,
                        other_key,
                        other_entity,
                        name.as_ref(),
                        // We pass `name` again for `other_name` because the `if` condition above
                        // proved they are identical, saving us from binding a temporary variable.
                        name.as_ref(),
                    ) {
                        unresolved_short_collisions += 1;
                    }
                }
            };

            for r in recent_borrow.iter() {
                if let Ok(other_entity) = Self::get_entity(ctx, &r.key) {
                    check_collision(&r.key, other_entity);
                }
            }
            for &fk in future_keys {
                if !recent_borrow.iter().any(|r| r.key == fk)
                    && let Some(&other_entity) = pre_resolved.get(fk)
                {
                    check_collision(fk, other_entity);
                }
            }

            name_collision = unresolved_short_collisions > 0;

            if short_collisions > 0
                && let Some(long_name) = entity.long_display_name_for(effective_viewer)
                && long_name != name
            {
                let mut long_collisions = 0;

                // Verify how many times the long name collides
                let mut check_long = |other_key: &str, other_entity: &'a dyn TemplateEntity| {
                    if other_key != key {
                        let other_short = other_entity.display_name_for(effective_viewer);
                        // Only consider the other entity's long name if its short name is in the
                        // exact same collision group as our entity's short name (preventing phantoms).
                        if other_short == long_name
                            || (other_short == name
                                && other_entity
                                    .long_display_name_for(effective_viewer)
                                    .as_deref()
                                    == Some(long_name.as_ref()))
                        {
                            long_collisions += 1;
                        }
                    }
                };

                for r in recent_borrow.iter() {
                    if let Ok(other_entity) = Self::get_entity(ctx, &r.key) {
                        check_long(&r.key, other_entity);
                    }
                }
                for &fk in future_keys {
                    if !recent_borrow.iter().any(|r| r.key == fk)
                        && let Some(&other_entity) = pre_resolved.get(fk)
                    {
                        check_long(fk, other_entity);
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
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
    ) -> Result<(), String> {
        let entity = pre_resolved
            .get(params.key)
            .copied()
            .map_or_else(|| Self::get_entity(ctx, params.key), Ok)?;

        let effective_viewer = effective_viewer_id(ctx, params.flags.force_3rd_person());

        let mut article_to_use = params.article;
        let mut active_flags = params.flags;
        let mut actual_p_type = params.p_type;

        if Self::try_render_pronoun(
            ctx,
            raw_output,
            params,
            entity,
            effective_viewer,
            pre_resolved,
            &mut article_to_use,
            &mut active_flags,
            &mut actual_p_type,
        )? {
            return Ok(());
        }

        Self::render_resolved_entity(
            ctx,
            raw_output,
            params,
            entity,
            effective_viewer,
            article_to_use,
            actual_p_type,
            active_flags,
            future_keys,
            pre_resolved,
        );

        Ok(())
    }

    #[inline]
    fn determine_group_singular_gender(members: &[&dyn TemplateEntity]) -> crate::models::Gender {
        let mut flat = Vec::new();
        crate::models::flatten_group(members, &mut flat, 0);
        let mut shared = None;
        for m in flat {
            let g = m.gender();
            let singular_g = if g == crate::models::Gender::Plural {
                crate::models::Gender::Neutral
            } else {
                g
            };
            if let Some(s) = shared {
                if s != singular_g {
                    return crate::models::Gender::Neutral;
                }
            } else {
                shared = Some(singular_g);
            }
        }
        shared.unwrap_or(crate::models::Gender::Neutral)
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn try_render_pronoun<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        params: &EntityRefParams<'_>,
        entity: &'a dyn TemplateEntity,
        effective_viewer: &str,
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
        article_to_use: &mut Option<&'a str>,
        active_flags: &mut TagFlags,
        actual_p_type: &mut Option<&'a str>,
    ) -> Result<bool, String> {
        let Some(p_type) = params.p_type else {
            return Ok(false);
        };

        let mut is_plural = entity.is_plural();
        let mut effective_gender = entity.gender();
        let is_group = entity.group_members().is_some();
        let extract_member = params.flags.extract_group_member() && is_group;

        if params.flags.force_singular() || extract_member {
            is_plural = false;
            if let Some(members) = entity.group_members() {
                effective_gender = Self::determine_group_singular_gender(members);
            } else if effective_gender == crate::models::Gender::Plural {
                effective_gender = crate::models::Gender::Neutral;
            }
        }

        let is_viewer = entity.contains_viewer(effective_viewer) && !extract_member;
        let is_active_subject =
            Self::check_is_active_subject(ctx, entity, params.key, pre_resolved);

        if is_active_subject && p_type == "obj" {
            *actual_p_type = Some("reflex");
        }

        let already_seen = ctx
            .recent_entities
            .borrow()
            .iter()
            .any(|r| r.key == params.key);
        let is_reflexive = *actual_p_type == Some("reflex");

        // "You" is ambiguous for a group in the 2nd person.
        // 1st person plural is "We" (unambiguous). 3rd person is "They" (unambiguous).
        let ambiguous_plural_you = is_viewer
            && is_group
            && ctx.stance == crate::models::ActorStance::SecondPerson
            && !is_reflexive
            && !active_flags.allow_ambiguous_you();

        let mut can_use_pronoun = (!ambiguous_plural_you && (is_active_subject || is_viewer))
            || is_reflexive
            || active_flags.no_smart();

        if active_flags.prefer_noun() && (!is_viewer || is_group) && !is_reflexive {
            can_use_pronoun = false;
        }

        if !can_use_pronoun
            && already_seen
            && !active_flags.prefer_noun()
            && !Self::is_pronoun_ambiguous(
                ctx,
                params.key,
                effective_gender,
                is_plural,
                params.flags,
            )
        {
            can_use_pronoun = true;
        }

        if can_use_pronoun {
            if !already_seen {
                update_memory(&ctx.last_mentioned, params.key);
                track_recent_entity(ctx, params.key, entity);
            }
            let pronoun = resolve_pronoun(
                effective_gender,
                actual_p_type.as_deref().unwrap_or(p_type),
                is_viewer,
                is_plural,
                ctx.stance,
            )?;
            let cap_pronoun = active_flags.contains(TagFlags::PRONOUN_CAPITALIZED)
                || active_flags.is_capitalized()
                || active_flags.article_capitalized();
            push_capitalized_if(raw_output, pronoun, cap_pronoun);
            return Ok(true);
        }

        if *actual_p_type == Some("poss") || *actual_p_type == Some("abs_poss") {
            active_flags.set(TagFlags::IS_POSSESSIVE, true);
        }

        if active_flags.contains(TagFlags::PRONOUN_CAPITALIZED) {
            active_flags.set(TagFlags::ARTICLE_CAPITALIZED, true);
        }

        if article_to_use.is_none() {
            let is_cap = active_flags.article_capitalized();
            let fallback_article = if params.flags.force_singular() && entity.is_plural() {
                if is_cap { "One of the" } else { "one of the" }
            } else {
                if is_cap { "A" } else { "a" }
            };
            *article_to_use = Some(fallback_article);
            active_flags.set(
                TagFlags::ARTICLE_INDEFINITE,
                !params.flags.force_singular() || !entity.is_plural(),
            );
            active_flags.set(TagFlags::ARTICLE_CAPITALIZED, is_cap); // the fallback inherits the pronoun's requested capitalization
            active_flags.set(TagFlags::IS_CAPITALIZED, false); // We don't want to force-capitalize common nouns ("A Goblin")
        }

        Ok(false)
    }

    #[inline]
    fn is_after_possessive(output: &str) -> bool {
        let s = output.trim_end();
        if s.ends_with("'s") || s.ends_with('\'') || s.ends_with('’') {
            return true;
        }
        let last_word = s.rsplit(|c: char| !c.is_alphabetic()).next().unwrap_or("");
        matches!(
            last_word.to_ascii_lowercase().as_str(),
            "my" | "your" | "his" | "her" | "its" | "our" | "their" | "whose"
        )
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn render_resolved_entity<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        params: &EntityRefParams<'_>,
        entity: &'a dyn TemplateEntity,
        effective_viewer: &str,
        mut article_to_use: Option<&'a str>,
        actual_p_type: Option<&'a str>,
        active_flags: TagFlags,
        future_keys: &[&str],
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
    ) {
        let (name, ordinal) = Self::resolve_display_name(
            ctx,
            entity,
            params.key,
            effective_viewer,
            params.flags.no_smart(),
            future_keys,
            pre_resolved,
        );

        let already_seen = ctx
            .recent_entities
            .borrow()
            .iter()
            .any(|r| r.key == params.key);

        if !active_flags.no_smart() && article_to_use.is_some() && active_flags.article_indefinite()
        {
            if already_seen {
                article_to_use = Some(if active_flags.article_capitalized() {
                    "The"
                } else {
                    "the"
                });
            } else if let Some(ord) = ordinal
                && ord == 2
            {
                article_to_use = Some(if active_flags.article_capitalized() {
                    "Another"
                } else {
                    "another"
                });
            }
        }

        update_memory(&ctx.last_mentioned, params.key);
        track_recent_entity(ctx, params.key, entity);

        let after_possessive = Self::is_after_possessive(raw_output);

        let active_params = EntityRefParams {
            key: params.key,
            article: article_to_use,
            p_type: actual_p_type,
            flags: active_flags,
            ordinal,
        };
        let cap_whole = should_capitalize_whole_tag(&active_params);

        // --- Handle Groups / Distributed Lists ---
        if let Some(members) = entity.group_members() {
            Self::render_group_entity(
                ctx,
                raw_output,
                entity,
                members,
                effective_viewer,
                &active_params,
                cap_whole,
                after_possessive,
                pre_resolved,
            );
            return;
        }

        let is_plural = entity.is_plural()
            && !active_params.flags.force_singular()
            && !active_flags.extract_group_member();

        // --- Handle Single Entity Viewers ---
        if entity.contains_viewer(effective_viewer) && !active_flags.extract_group_member() {
            raw_output.push_str(viewer_name(
                ctx.stance,
                is_plural, // This is already `false` if `force_singular` was requested
                active_flags.is_possessive(),
                cap_whole,
                actual_p_type == Some("obj"),
            ));
            return;
        }

        // --- Single Entity Fallback ---
        Self::render_single_entity(
            ctx,
            raw_output,
            entity,
            effective_viewer,
            name,
            &active_params,
            cap_whole,
            after_possessive,
            is_plural,
        );
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn render_single_entity<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        entity: &'a dyn TemplateEntity,
        effective_viewer: &str,
        name: std::borrow::Cow<'a, str>,
        active_params: &EntityRefParams<'_>,
        cap_whole: bool,
        after_possessive: bool,
        is_plural: bool,
    ) {
        let mut article_printed = false;

        let mut article_flags = crate::grammar::ArticleFlags::empty();
        article_flags.set(
            crate::grammar::ArticleFlags::IS_PROPER_NOUN,
            entity.is_proper_noun_for(effective_viewer),
        );
        article_flags.set(crate::grammar::ArticleFlags::IS_PLURAL, is_plural);
        article_flags.set(
            crate::grammar::ArticleFlags::FORCE_ARTICLE,
            active_params.flags.force_article(),
        );
        article_flags.set(
            crate::grammar::ArticleFlags::AFTER_POSSESSIVE,
            after_possessive,
        );

        if let Some(resolved_art) = active_params.article.as_ref().and_then(|art| {
            resolve_article(
                art,
                &name,
                active_params.ordinal,
                entity.collective_noun(),
                ctx.ordinal_word_threshold,
                article_flags,
            )
        }) {
            raw_output.push_str(resolved_art.as_ref());
            article_printed = true;
        }

        let should_cap_noun = if article_printed {
            active_params.flags.is_capitalized()
        } else if after_possessive && !active_params.flags.force_article() {
            false
        } else {
            cap_whole
        };

        let name_cow = capitalize_cow(name, should_cap_noun);
        let name_str = name_cow.as_ref();

        raw_output.push_str(name_str);

        if active_params.flags.is_possessive() {
            raw_output.push_str(Self::get_possessive_suffix(name_str, entity.is_plural()));
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn render_group_entity<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        entity: &'a dyn TemplateEntity,
        members: &[&'a dyn TemplateEntity],
        effective_viewer: &str,
        params: &EntityRefParams<'_>,
        cap_whole: bool,
        after_possessive: bool,
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
    ) {
        let (viewer_entity, visible) = crate::models::partition_group(members, effective_viewer);

        let total_visible = visible.len() + usize::from(viewer_entity.is_some());
        if total_visible == 0 {
            return;
        }

        let active_subject_entity = ctx
            .active_subject
            .borrow()
            .as_deref()
            .and_then(|active_key| {
                pre_resolved
                    .get(active_key)
                    .copied()
                    .map_or_else(|| Self::get_entity(ctx, active_key).ok(), Some)
            });

        let viewer_is_active_subject =
            active_subject_entity.is_some_and(|active| active.contains_viewer(effective_viewer));
        let is_objective = params.p_type == Some("obj");

        let mut ends_with_possessive_pronoun = false;
        let mut decomposed_we = false;
        let mut formatted_names = Vec::with_capacity(total_visible + 1);

        if let Some(viewer) = viewer_entity
            && let Some(prefix) = Self::format_group_viewer_prefix(
                ctx,
                viewer,
                params,
                visible.is_empty(),
                viewer_is_active_subject,
                is_objective,
                &mut ends_with_possessive_pronoun,
                &mut decomposed_we,
            )
        {
            formatted_names.push(prefix);
        }

        let will_append_my = viewer_entity.is_some_and(|viewer| {
            ctx.stance == crate::models::ActorStance::FirstPerson
                && (!viewer.is_plural() || decomposed_we)
        });

        let distribute_possessives = viewer_entity.is_some() && params.flags.is_possessive();

        let lower_article_storage = params.article.map(str::to_lowercase);
        let mut first_visible_item = viewer_entity.is_none();
        for (member, name) in visible {
            let article_to_use = if first_visible_item {
                params.article
            } else {
                lower_article_storage.as_deref()
            };

            let member_is_active_subj =
                active_subject_entity.is_some_and(|active| std::ptr::eq(active, member));

            let mut flags = GroupMemberFlags::empty();
            flags.set(GroupMemberFlags::AFTER_POSSESSIVE, after_possessive);
            flags.set(GroupMemberFlags::FIRST_VISIBLE_ITEM, first_visible_item);
            flags.set(
                GroupMemberFlags::DISTRIBUTE_POSSESSIVES,
                distribute_possessives,
            );
            flags.set(
                GroupMemberFlags::IS_REFLEXIVE,
                is_objective && member_is_active_subj,
            );

            let config = GroupMemberFormatConfig {
                flags,
                article_to_use,
            };

            formatted_names.push(Self::format_group_member(
                ctx,
                entity,
                member,
                name,
                effective_viewer,
                params,
                &config,
            ));
            first_visible_item = false;
        }

        if will_append_my {
            let suffix = Self::format_group_viewer_suffix(
                params.flags.is_possessive(),
                viewer_is_active_subject,
                is_objective,
                &mut ends_with_possessive_pronoun,
            );
            formatted_names.push(suffix);
        }

        let conjunction = if params.flags.extract_group_member() {
            "or"
        } else {
            "and"
        };
        let list_str = crate::grammar::format_oxford_list(formatted_names, conjunction);

        let mut final_str = list_str.into_owned();
        if params.flags.is_possessive() && !ends_with_possessive_pronoun && !distribute_possessives
        {
            final_str.push_str(Self::get_possessive_suffix(&final_str, entity.is_plural()));
        }

        push_capitalized_if(raw_output, &final_str, cap_whole);
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn format_group_viewer_prefix(
        ctx: &RenderContext,
        viewer: &dyn TemplateEntity,
        params: &EntityRefParams<'_>,
        visible_is_empty: bool,
        viewer_is_active_subject: bool,
        is_objective: bool,
        ends_with_possessive_pronoun: &mut bool,
        decomposed_we: &mut bool,
    ) -> Option<std::borrow::Cow<'static, str>> {
        if ctx.stance == crate::models::ActorStance::SecondPerson {
            if params.flags.is_possessive() {
                if visible_is_empty {
                    *ends_with_possessive_pronoun = true;
                }
                return Some(std::borrow::Cow::Borrowed("your"));
            } else if is_objective && viewer_is_active_subject {
                let reflex = resolve_pronoun(
                    crate::models::Gender::Neutral,
                    "reflex",
                    true,
                    viewer.is_plural(),
                    ctx.stance,
                )
                .unwrap_or("yourself");
                return Some(std::borrow::Cow::Borrowed(reflex));
            }
            return Some(std::borrow::Cow::Borrowed("you"));
        } else if ctx.stance == crate::models::ActorStance::FirstPerson && viewer.is_plural() {
            if visible_is_empty {
                if params.flags.is_possessive() {
                    *ends_with_possessive_pronoun = true;
                    return Some(std::borrow::Cow::Borrowed("our"));
                } else if is_objective {
                    if viewer_is_active_subject {
                        let reflex = resolve_pronoun(
                            crate::models::Gender::Neutral,
                            "reflex",
                            true,
                            true,
                            ctx.stance,
                        )
                        .unwrap_or("ourselves");
                        return Some(std::borrow::Cow::Borrowed(reflex));
                    }
                    return Some(std::borrow::Cow::Borrowed("us"));
                }
                return Some(std::borrow::Cow::Borrowed("we"));
            }

            *decomposed_we = true;
            if params.flags.is_possessive() {
                return Some(std::borrow::Cow::Borrowed("your"));
            } else if is_objective && viewer_is_active_subject {
                let reflex = resolve_pronoun(
                    crate::models::Gender::Neutral,
                    "reflex",
                    true,
                    false,
                    crate::models::ActorStance::SecondPerson,
                )
                .unwrap_or("yourself");
                return Some(std::borrow::Cow::Borrowed(reflex));
            }
            return Some(std::borrow::Cow::Borrowed("you"));
        }
        None
    }

    #[inline]
    fn format_group_viewer_suffix(
        is_possessive: bool,
        viewer_is_active_subject: bool,
        is_objective: bool,
        ends_with_possessive_pronoun: &mut bool,
    ) -> std::borrow::Cow<'static, str> {
        if is_possessive {
            *ends_with_possessive_pronoun = true;
            std::borrow::Cow::Borrowed("my")
        } else if is_objective {
            if viewer_is_active_subject {
                let reflex = resolve_pronoun(
                    crate::models::Gender::Neutral,
                    "reflex",
                    true,
                    false,
                    crate::models::ActorStance::FirstPerson,
                )
                .unwrap_or("myself");
                std::borrow::Cow::Borrowed(reflex)
            } else {
                std::borrow::Cow::Borrowed("me")
            }
        } else {
            std::borrow::Cow::Borrowed("I")
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn format_group_member<'a>(
        ctx: &RenderContext,
        entity: &dyn TemplateEntity,
        member: &'a dyn TemplateEntity,
        name: std::borrow::Cow<'a, str>,
        effective_viewer: &str,
        params: &EntityRefParams<'_>,
        config: &GroupMemberFormatConfig<'_>,
    ) -> std::borrow::Cow<'a, str> {
        let first_visible_item = config.flags.contains(GroupMemberFlags::FIRST_VISIBLE_ITEM);

        if config.flags.contains(GroupMemberFlags::IS_REFLEXIVE) {
            let reflex = resolve_pronoun(
                member.gender(),
                "reflex",
                false,
                member.is_plural(),
                ctx.stance,
            )
            .unwrap_or("itself");
            let mut final_name = std::borrow::Cow::Owned(
                if params.flags.is_capitalized()
                    || (params.flags.article_capitalized() && first_visible_item)
                {
                    crate::grammar::capitalize_first(reflex)
                } else {
                    reflex.to_string()
                },
            );

            if config
                .flags
                .contains(GroupMemberFlags::DISTRIBUTE_POSSESSIVES)
            {
                let suffix = Self::get_possessive_suffix(&final_name, member.is_plural());
                let mut owned = final_name.into_owned();
                owned.push_str(suffix);
                final_name = std::borrow::Cow::Owned(owned);
            }
            return final_name;
        }

        let mut article_flags = crate::grammar::ArticleFlags::empty();
        article_flags.set(
            crate::grammar::ArticleFlags::IS_PROPER_NOUN,
            member.is_proper_noun_for(effective_viewer),
        );
        article_flags.set(crate::grammar::ArticleFlags::IS_PLURAL, member.is_plural());
        article_flags.set(
            crate::grammar::ArticleFlags::FORCE_ARTICLE,
            params.flags.force_article(),
        );
        article_flags.set(
            crate::grammar::ArticleFlags::AFTER_POSSESSIVE,
            config.flags.contains(GroupMemberFlags::AFTER_POSSESSIVE) && first_visible_item,
        );
        article_flags.set(
            crate::grammar::ArticleFlags::IS_CAPITALIZED,
            params.flags.article_capitalized() && first_visible_item,
        );

        let mut final_name = if let Some(resolved_art) = config.article_to_use.and_then(|art| {
            resolve_article(
                art,
                &name,
                params.ordinal,
                entity.collective_noun(),
                ctx.ordinal_word_threshold,
                article_flags,
            )
        }) {
            std::borrow::Cow::Owned(format!("{}{name}", resolved_art.as_ref()))
        } else {
            name
        };

        if config
            .flags
            .contains(GroupMemberFlags::DISTRIBUTE_POSSESSIVES)
        {
            let suffix = Self::get_possessive_suffix(&final_name, member.is_plural());
            let mut owned = final_name.into_owned();
            owned.push_str(suffix);
            final_name = std::borrow::Cow::Owned(owned);
        }

        final_name
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
            // (SIMD pre-scan is optimized, and next_back() evaluates from the end).
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
        MOD_POSSESSIVE
    }

    #[inline]
    fn check_is_active_subject(
        ctx: &RenderContext,
        entity: &dyn TemplateEntity,
        key: &str,
        pre_resolved: &HashMap<&str, &dyn TemplateEntity>,
    ) -> bool {
        if ctx.active_subject.borrow().as_deref() == Some(key) {
            return true;
        }
        if let Some(active_key) = ctx.active_subject.borrow().as_deref()
            && let Ok(active_entity) = pre_resolved
                .get(active_key)
                .copied()
                .map_or_else(|| Self::get_entity(ctx, active_key), Ok)
        {
            return std::ptr::eq(entity, active_entity);
        }
        false
    }

    #[inline]
    fn is_pronoun_ambiguous(
        ctx: &RenderContext,
        key: &str,
        effective_gender: crate::models::Gender,
        is_plural: bool,
        flags: TagFlags,
    ) -> bool {
        for other in ctx.recent_entities.borrow().iter() {
            if other.key != key {
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

                if !other_is_viewer
                    && effective_gender == other.gender
                    && is_plural
                        == other
                            .flags
                            .contains(crate::models::RecentEntityFlags::IS_PLURAL)
                {
                    return true;
                }
            }
        }
        false
    }

    fn render_verb_ref<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        token: &Token,
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
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
        let (mut is_viewer, mut is_plural, is_group) = if let Some(key) = subject_key {
            let entity = pre_resolved
                .get(key.as_str())
                .copied()
                .map_or_else(|| Self::get_entity(ctx, key), Ok)?;
            let effective_viewer = effective_viewer_id(ctx, flags.force_3rd_person());
            update_memory(&ctx.active_subject, key);
            update_memory(&ctx.last_mentioned, key);
            track_recent_entity(ctx, key, entity);
            (
                entity.contains_viewer(effective_viewer),
                entity.is_plural(),
                entity.group_members().is_some(),
            )
        } else {
            (false, false, false)
        };

        let extract_member = flags.extract_group_member() && is_group;
        if flags.force_singular() || extract_member {
            is_plural = false;
        }
        if extract_member {
            is_viewer = false;
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
    let close_char = if open_char == TAG_ENTITY_OPEN {
        TAG_ENTITY_CLOSE
    } else {
        TAG_VERB_CLOSE
    };
    let formatted_message = format!("{message}: {open_char}{content}{close_char}");
    tracing::error!("{}", formatted_message);
    formatted_message
}

/// Parses prefix modifiers (like `+`) used to force perspectives, returning the
/// stripped string alongside the boolean flags for `force_3rd_person`/`force_article`, `no_smart`, and `force_singular`.
#[inline]
fn parse_stance_prefixes(mut s: &str) -> (&str, TagFlags) {
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
        } else {
            break;
        }
    }
    (s, flags)
}

type ParsedForcedConjugations = (Option<Vec<String>>, Option<Vec<String>>);

#[inline]
fn parse_forced_conjugations(
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
fn split_tag<'a>(
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
fn is_indefinite_article(s: &str) -> bool {
    s.eq_ignore_ascii_case("a") || s.eq_ignore_ascii_case("an")
}

#[inline]
const fn is_escapable_char(c: char) -> bool {
    matches!(
        c,
        TAG_ENTITY_OPEN | TAG_VERB_OPEN | TAG_ENTITY_CLOSE | TAG_VERB_CLOSE | TAG_ESCAPE
    )
}

#[inline]
fn should_capitalize_whole_tag(params: &EntityRefParams<'_>) -> bool {
    params.flags.is_capitalized() || params.flags.article_capitalized()
}

#[inline]
#[allow(clippy::fn_params_excessive_bools)]
const fn viewer_name(
    stance: crate::models::ActorStance,
    is_plural: bool,
    is_possessive: bool,
    is_capitalized: bool,
    is_obj: bool,
) -> &'static str {
    match stance {
        crate::models::ActorStance::FirstPerson => {
            match (is_plural, is_possessive, is_capitalized, is_obj) {
                (false, true, true, _) => "My",
                (false, true, false, _) => "my",
                (false, false, true, true) => "Me",
                (false, false, false, true) => "me",
                (false, false, _, false) => "I",
                (true, true, true, _) => "Our",
                (true, true, false, _) => "our",
                (true, false, true, true) => "Us",
                (true, false, false, true) => "us",
                (true, false, true, false) => "We",
                (true, false, false, false) => "we",
            }
        }
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

/// Performs a SIMD pre-scan to detect the presence of protocol triggers.
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
        path.split(TAG_PROPERTY_SEP).any(str::is_empty),
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
