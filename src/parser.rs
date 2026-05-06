use std::collections::HashSet;

#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
use crate::typography::{has_protocol_tags, skip_protocol_tags};

// --- Template Syntax Constants ---
pub(crate) const TAG_ENTITY_OPEN: char = '{';
pub(crate) const TAG_ENTITY_CLOSE: char = '}';
pub(crate) const TAG_VERB_OPEN: char = '[';
pub(crate) const TAG_VERB_CLOSE: char = ']';
pub(crate) const TAG_SEPARATOR: char = ':';
pub(crate) const TAG_PROPERTY_SEP: char = '.';
pub(crate) const TAG_ESCAPE: char = '\\';

pub(crate) const VERB_TENSE_SEP: char = ';';
pub(crate) const VERB_FORM_SEP: char = '|';

pub(crate) const MOD_FORCE_3RD_PERSON: char = '+';
pub(crate) const MOD_NO_SMART: char = '!';
pub(crate) const MOD_FORCE_SINGULAR: char = '-';
pub(crate) const MOD_PREFER_NOUN: char = '*';
pub(crate) const MOD_ALLOW_AMBIGUOUS_YOU: char = '~';
pub(crate) const MOD_EXTRACT_GROUP_MEMBER: char = '^';
pub(crate) const MOD_POSSESSIVE: &str = "'s";
pub(crate) const MOD_DROP_POSSESSIVE: char = '@';

pub(crate) const CTRL_SENTENCE_BREAK: &str = "SB";
pub(crate) const CTRL_NO_SENTENCE_BREAK: &str = "NO_SB";

/// Represents a parsed unit of a template string.
#[derive(Debug)]
pub enum Token {
    /// Plain text that is inserted exactly as-is.
    Literal(String),
    /// e.g., `{source}`, `{a:source}`, `{the:target:obj}`, or `{source's glowing target}`
    EntityRef {
        /// The key of the entity in the `RenderContext`.
        key: String,
        /// An optional article to precede the entity name (e.g., "a", "an", "the").
        article: Option<String>,
        /// The optional type of pronoun requested (e.g., `"subj"`, `"obj"`, `"poss"`, `"abs_poss"`, `"reflex"`).
        p_type: Option<String>,
        /// The optional owner key if this is a narrative possessive (e.g., `source` in `{source's target}`)
        owner_key: Option<String>,
        /// Modifiers specifically attached to the owner
        owner_flags: TagFlags,
        /// Optional adjectives between the owner and the target
        adjectives: Option<String>,
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

/// Type alias for the parsed components of a verb's conjugations.
pub(crate) type VerbConjugations<'a> = (&'a str, Option<Vec<String>>, Option<Vec<String>>);

/// Type alias for the destructured components of an entity tag.
pub(crate) type DestructuredEntityTag<'a> =
    (Option<&'a str>, Option<&'a str>, &'a str, Option<&'a str>);

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
            match token {
                Token::EntityRef { key, owner_key, .. } => {
                    if seen_keys.insert(key.as_str()) {
                        template_keys.push(key.clone());
                    }
                    if let Some(ok) = owner_key
                        && seen_keys.insert(ok.as_str())
                    {
                        template_keys.push(ok.clone());
                    }
                }
                Token::VerbRef {
                    subject_key: Some(key),
                    ..
                } if seen_keys.insert(key.as_str()) => {
                    template_keys.push(key.clone());
                }
                _ => {}
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
    ) -> Result<DestructuredEntityTag<'a>, String> {
        match parts {
            [p1, p2, p3, p4] => {
                let (p1_clean, _) = parse_stance_prefixes(p1);
                if p1_clean.is_empty() || !is_article(p1_clean) {
                    return Err(validation_error(
                        "Malformed entity tag",
                        content,
                        TAG_ENTITY_OPEN,
                    ));
                }
                Ok((Some(*p1), Some(*p2), *p3, Some(*p4)))
            }
            [p1, p2, p3] => {
                let (p1_clean, _) = parse_stance_prefixes(p1);
                if p1_clean.is_empty() || is_article(p1_clean) {
                    let (p2_clean, _) = parse_stance_prefixes(p2);
                    let article = if p1.is_empty() { None } else { Some(*p1) };

                    if is_owner_part(p2_clean) && !is_p_type(p3) {
                        Ok((article, Some(*p2), *p3, None))
                    } else if is_p_type(p3) || p3.is_empty() {
                        Ok((article, None, *p2, Some(*p3)))
                    } else {
                        Err(validation_error(
                            "Malformed entity tag",
                            content,
                            TAG_ENTITY_OPEN,
                        ))
                    }
                } else if is_owner_part(p1_clean) && (is_p_type(p3) || p3.is_empty()) {
                    Ok((None, Some(*p1), *p2, Some(*p3)))
                } else {
                    Err(validation_error(
                        "Malformed entity tag",
                        content,
                        TAG_ENTITY_OPEN,
                    ))
                }
            }
            [p1, p2] => {
                let (p1_clean, _) = parse_stance_prefixes(p1);

                if !p1_clean.is_empty() && is_article(p1_clean) {
                    let article = if p1.is_empty() { None } else { Some(*p1) };
                    Ok((article, None, *p2, None))
                } else if is_p_type(p2) || p2.is_empty() {
                    Ok((None, None, *p1, Some(*p2)))
                } else if is_owner_part(p1_clean) {
                    Ok((None, Some(*p1), *p2, None))
                } else if p1_clean.is_empty() {
                    let article = if p1.is_empty() { None } else { Some(*p1) };
                    Ok((article, None, *p2, None))
                } else {
                    Err(validation_error(
                        "Malformed entity tag",
                        content,
                        TAG_ENTITY_OPEN,
                    ))
                }
            }
            [p1] => Ok((None, None, *p1, None)),
            _ => Err(validation_error(
                "Malformed entity tag",
                content,
                TAG_ENTITY_OPEN,
            )),
        }
    }

    fn parse_entity(content: &str) -> Result<Token, String> {
        let is_all_caps = is_all_caps(content);

        let parts: Vec<&str> = content.split(TAG_SEPARATOR).map(str::trim).collect();
        reject_if(
            parts.is_empty() || parts.len() > 4,
            "Malformed entity tag",
            content,
            TAG_ENTITY_OPEN,
        )?;

        let (raw_article, raw_owner, raw_key, raw_p_type) =
            Self::destructure_entity_tag(&parts, content)?;

        let mut flags = TagFlags::empty();

        let clean_article = Self::process_article_part(raw_article, &mut flags);

        let (owner_key, owner_flags, adjectives, working_key) =
            Self::process_owner_part(raw_owner, raw_key);

        let clean_key = Self::process_key_part(
            working_key,
            raw_article.is_some(),
            raw_p_type.is_some(),
            content,
            &mut flags,
        )?;
        let clean_p_type = Self::process_p_type_part(raw_p_type, content, &mut flags)?;

        flags.set(TagFlags::ALL_CAPS, is_all_caps);

        Ok(Token::EntityRef {
            key: clean_key.to_lowercase(),
            article: clean_article.map(ToString::to_string),
            p_type: clean_p_type.map(str::to_lowercase),
            owner_key,
            owner_flags,
            adjectives,
            flags,
        })
    }

    #[inline]
    fn process_owner_part<'a>(
        raw_owner: Option<&'a str>,
        raw_key: &'a str,
    ) -> (Option<String>, TagFlags, Option<String>, &'a str) {
        let mut owner_key = None;
        let mut owner_flags = TagFlags::empty();
        let mut adjectives = None;
        let mut working_key = raw_key;

        if let Some(owner_part) = raw_owner {
            let owner_idx_3 = owner_part
                .find("'s ")
                .or_else(|| owner_part.find("'S "))
                .or_else(|| owner_part.find("’s "))
                .or_else(|| owner_part.find("’S "));
            let owner_idx_2 = owner_part.find("' ").or_else(|| owner_part.find("’ "));

            let (owner_str, adj_str) = if let Some(idx) = owner_idx_3 {
                (&owner_part[..idx], &owner_part[idx + 3..])
            } else if let Some(idx) = owner_idx_2 {
                (&owner_part[..idx], &owner_part[idx + 2..])
            } else if let Some(stripped) = owner_part
                .strip_suffix(MOD_POSSESSIVE)
                .or_else(|| owner_part.strip_suffix("'S"))
                .or_else(|| owner_part.strip_suffix("’s"))
                .or_else(|| owner_part.strip_suffix("’S"))
            {
                (stripped, "")
            } else if let Some(stripped) = owner_part
                .strip_suffix('\'')
                .or_else(|| owner_part.strip_suffix('’'))
            {
                (stripped, "")
            } else {
                (owner_part, "")
            };

            let (clean_owner, mut o_flags) = parse_stance_prefixes(owner_str);
            o_flags.set(TagFlags::IS_CAPITALIZED, is_capitalized(clean_owner));
            owner_key = Some(clean_owner.to_lowercase());
            owner_flags = o_flags;

            let adj = adj_str.trim();
            if !adj.is_empty() {
                adjectives = Some(format!("{adj} "));
            }
        } else {
            let owner_idx_3 = working_key
                .find("'s ")
                .or_else(|| working_key.find("'S "))
                .or_else(|| working_key.find("’s "))
                .or_else(|| working_key.find("’S "));
            let owner_idx_2 = working_key.find("' ").or_else(|| working_key.find("’ "));

            if let Some(idx) = owner_idx_3 {
                let owner_part = &working_key[..idx];
                let (clean_owner, mut o_flags) = parse_stance_prefixes(owner_part);
                o_flags.set(TagFlags::IS_CAPITALIZED, is_capitalized(clean_owner));
                owner_key = Some(clean_owner.to_lowercase());
                owner_flags = o_flags;

                let remainder = &working_key[idx + 3..];
                if let Some(space_idx) = remainder.rfind(' ') {
                    adjectives = Some(remainder[..=space_idx].to_string());
                    working_key = &remainder[space_idx + 1..];
                } else {
                    working_key = remainder;
                }
            } else if let Some(idx) = owner_idx_2 {
                let owner_part = &working_key[..idx];
                let (clean_owner, mut o_flags) = parse_stance_prefixes(owner_part);
                o_flags.set(TagFlags::IS_CAPITALIZED, is_capitalized(clean_owner));
                owner_key = Some(clean_owner.to_lowercase());
                owner_flags = o_flags;

                let remainder = &working_key[idx + 2..];
                if let Some(space_idx) = remainder.rfind(' ') {
                    adjectives = Some(remainder[..=space_idx].to_string());
                    working_key = &remainder[space_idx + 1..];
                } else {
                    working_key = remainder;
                }
            }
        }

        (owner_key, owner_flags, adjectives, working_key)
    }

    #[inline]
    fn process_article_part<'a>(
        raw_article: Option<&'a str>,
        flags: &mut TagFlags,
    ) -> Option<&'a str> {
        raw_article.map(|art| {
            let (clean_art, mut art_flags) = parse_stance_prefixes(art);
            if art_flags.contains(TagFlags::FORCE_3RD_PERSON) {
                art_flags.insert(TagFlags::FORCE_ARTICLE);
            }
            *flags |= art_flags;
            flags.set(
                TagFlags::ARTICLE_INDEFINITE,
                is_indefinite_article(clean_art),
            );
            flags.set(TagFlags::ARTICLE_CAPITALIZED, is_capitalized(clean_art));
            clean_art
        })
    }

    #[inline]
    fn process_key_part<'a>(
        raw_key: &'a str,
        has_article: bool,
        has_p_type: bool,
        content: &str,
        flags: &mut TagFlags,
    ) -> Result<&'a str, String> {
        let (clean_key, key_flags) = parse_stance_prefixes(raw_key);
        let (clean_key, is_possessive) = parse_possessive_suffix(clean_key);

        if has_article && clean_key.is_empty() {
            return Err(validation_error(
                "Entity tag has an article but an empty key",
                content,
                TAG_ENTITY_OPEN,
            ));
        }
        if has_p_type && clean_key.is_empty() {
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

        *flags |= key_flags;
        flags.set(TagFlags::IS_POSSESSIVE, is_possessive);
        flags.set(TagFlags::IS_CAPITALIZED, is_capitalized(clean_key));

        Ok(clean_key)
    }

    #[inline]
    fn process_p_type_part<'a>(
        raw_p_type: Option<&'a str>,
        content: &str,
        flags: &mut TagFlags,
    ) -> Result<Option<&'a str>, String> {
        raw_p_type
            .map(|pt| {
                let (clean_pt, pt_flags) = parse_stance_prefixes(pt);
                reject_if(
                    clean_pt.is_empty(),
                    "Pronoun tag has an empty key or type",
                    content,
                    TAG_ENTITY_OPEN,
                )?;
                *flags |= pt_flags;
                flags.set(TagFlags::PRONOUN_CAPITALIZED, is_capitalized(clean_pt));
                Ok(clean_pt)
            })
            .transpose()
    }

    fn parse_verb(content: &str) -> Result<Token, String> {
        let is_all_caps = is_all_caps(content);

        let (p1, p2_opt) = split_tag(content, TAG_VERB_OPEN, "Malformed verb tag")?;
        let (p1_str, p1_flags) = parse_stance_prefixes(p1);

        let (subject_key, verb_part) = match Self::process_verb_subject(p1_str, p2_opt, content)? {
            Ok(parts) => parts,
            Err(token) => return Ok(token),
        };

        let (actual_verb, forced_present, forced_past) =
            Self::process_verb_conjugations(verb_part, content)?;

        let original_verb = actual_verb.to_string();
        let is_capitalized = is_capitalized(&original_verb);
        let lower_verb = original_verb.to_lowercase();

        Self::emit_verb_warnings(&original_verb, &lower_verb);

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

    #[inline]
    fn process_verb_subject<'a>(
        p1_str: &'a str,
        p2_opt: Option<&'a str>,
        content: &str,
    ) -> Result<Result<(Option<String>, &'a str), Token>, String> {
        if let Some(p2) = p2_opt {
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
            Ok(Ok((Some(p1_str.to_lowercase()), p2)))
        } else {
            if p1_str == CTRL_SENTENCE_BREAK {
                return Ok(Err(Token::SentenceBreak));
            }
            if p1_str == CTRL_NO_SENTENCE_BREAK {
                return Ok(Err(Token::NoSentenceBreak));
            }
            Ok(Ok((None, p1_str)))
        }
    }

    #[inline]
    fn process_verb_conjugations<'a>(
        verb_part: &'a str,
        content: &str,
    ) -> Result<VerbConjugations<'a>, String> {
        if let Some((base_verb, forced)) = verb_part.split_once(VERB_FORM_SEP) {
            reject_if(
                base_verb.trim().is_empty() || forced.trim().is_empty(),
                "Verb tag has an empty verb or forced conjugation segment",
                content,
                TAG_VERB_OPEN,
            )?;

            let (forced_present, forced_past) = parse_forced_conjugations(forced, content)?;
            Ok((base_verb.trim(), forced_present, forced_past))
        } else {
            Ok((verb_part, None, None))
        }
    }

    #[inline]
    fn emit_verb_warnings(original_verb: &str, lower_verb: &str) {
        if let Some(options) = crate::grammar::get_collision_options(lower_verb) {
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
        /// A flag indicating if the builder explicitly forced the possessive form (e.g., `{source's}`, `{source's target}`).
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
        /// A flag indicating the builder used the Safe Override (@) to drop possessives for proper nouns.
        const DROP_POSSESSIVE = 1 << 13;
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
        drop_possessive: bool,
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
        flags.set(Self::DROP_POSSESSIVE, drop_possessive);
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

    /// Returns `true` if the drop possessive override flag is set.
    #[inline]
    #[must_use]
    pub const fn drop_possessive(self) -> bool {
        self.contains(Self::DROP_POSSESSIVE)
    }
}

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
pub(crate) fn is_owner_part(s: &str) -> bool {
    let (clean, _) = parse_stance_prefixes(s);
    let clean = clean.trim();
    clean.contains("'s ")
        || clean.contains("'S ")
        || clean.contains("’s ")
        || clean.contains("’S ")
        || clean.contains("' ")
        || clean.contains("’ ")
        || clean.ends_with("'s")
        || clean.ends_with("'S")
        || clean.ends_with("’s")
        || clean.ends_with("’S")
        || clean.ends_with('\'')
        || clean.ends_with('’')
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
        tokens.push(Token::Literal(slice.to_string()));
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
