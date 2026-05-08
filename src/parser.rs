use std::collections::HashSet;

/// Parsing logic for boolean expressions and conditionals.
pub mod expressions;
pub use expressions::{Condition, ConditionValue};
/// Utility functions for text processing and tag validation.
pub mod utils;
pub(crate) use utils::*;

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

/// The default maximum nesting depth for conditionals and boolean expressions to prevent stack overflow.
pub const DEFAULT_MAX_DEPTH: usize = 64;

/// A segment of a tag that can either be a static literal or a dynamic variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagSegment {
    /// A static string literal.
    Literal(String),
    /// A dynamic variable lookup key.
    Variable {
        /// The string key of the variable in the context.
        key: String,
        /// The optional fallback string if the variable is missing.
        fallback: Option<String>,
    },
}

impl TagSegment {
    pub(crate) fn parse(s: &str) -> Self {
        let s = s.trim();
        if let Some(var) = s.strip_prefix('$') {
            let (key, fallback) = extract_variable_fallback(var);
            Self::Variable {
                key: key.to_lowercase(),
                fallback,
            }
        } else {
            Self::Literal(s.to_string())
        }
    }
}

/// A single evaluated branch inside a conditional logic block.
#[derive(Debug, Clone)]
pub struct ConditionalBranch {
    /// The condition required to enter this branch.
    pub condition: Condition,
    /// The body of the branch.
    pub body: Vec<Token>,
}

/// Represents a parsed unit of a template string.
#[derive(Debug, Clone)]
pub enum Token {
    /// Plain text that is inserted exactly as-is.
    Literal(String),
    /// e.g., `{source}`, `{a:source}`, `{the:target:obj}`, or `{source's glowing target}`
    EntityRef {
        /// The key of the entity in the `RenderContext`.
        key: TagSegment,
        /// An optional article to precede the entity name (e.g., "a", "an", "the").
        article: Option<TagSegment>,
        /// The optional type of pronoun requested (e.g., `"subj"`, `"obj"`, `"poss"`, `"abs_poss"`, `"reflex"`).
        p_type: Option<TagSegment>,
        /// The optional owner key if this is a narrative possessive (e.g., `source` in `{source's target}`)
        owner_key: Option<TagSegment>,
        /// Modifiers specifically attached to the owner
        owner_flags: TagFlags,
        /// Optional adjectives between the owner and the target
        adjectives: Option<TagSegment>,
        /// A packed bitflags struct containing all formatting modifiers.
        flags: TagFlags,
    },
    /// e.g., [source:pulse]
    VerbRef {
        /// The optional subject key to bind the verb to for correct conjugation.
        subject_key: Option<TagSegment>,
        /// The original, un-processed form of the verb from the template.
        original_verb: String,
        /// The lowercased form of the verb, for dictionary lookups.
        lower_verb: String,
        /// An optional variable key to resolve and conjugate dynamically at render time.
        dynamic_key: Option<String>,
        /// An optional fallback string to use if the dynamic key is missing.
        dynamic_fallback: Option<String>,
        /// A sequence of explicit present-tense overrides that bypasses the algorithm entirely (e.g., `["am", "are", "is"]`).
        /// Note: This vector does not include the base verb itself, which is stored in `original_verb`.
        forced_present: Option<Vec<String>>,
        /// A sequence of explicit past-tense overrides that bypasses the algorithm entirely (e.g., `["was", "were", "was"]`).
        forced_past: Option<Vec<String>>,
        /// A packed bitflags struct containing all formatting modifiers.
        flags: TagFlags,
    },
    /// e.g., `{$weather}`, `{$Colors}`
    VariableRef {
        /// The key of the string variable in the `RenderContext`.
        key: String,
        /// An optional fallback string to use if the dynamic key is missing.
        fallback: Option<String>,
        /// A packed bitflags struct containing all formatting modifiers.
        flags: TagFlags,
    },
    /// A dynamic control-flow block evaluated at render time.
    Conditional {
        /// The list of `if` and `elif` branches.
        branches: Vec<ConditionalBranch>,
        /// The `else` fallback, if provided.
        fallback: Option<Vec<Token>>,
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

/// Type alias for the destructured components of an entity tag.
pub(crate) type DestructuredEntityTag<'a> =
    (Option<&'a str>, Option<&'a str>, &'a str, Option<&'a str>);

/// A container for the destructured components of a verb tag.
pub(crate) struct ParsedVerb<'a> {
    /// An optional dynamic variable key.
    pub dynamic_key: Option<String>,
    /// An optional fallback string.
    pub dynamic_fallback: Option<String>,
    /// The base verb string.
    pub base_verb: &'a str,
    /// Forced present tense overrides.
    pub forced_present: Option<Vec<String>>,
    /// Forced past tense overrides.
    pub forced_past: Option<Vec<String>>,
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
        Self::compile_with_depth(raw, DEFAULT_MAX_DEPTH)
    }

    /// Compiles a raw text string into a `Template` AST with a specific maximum nesting depth.
    ///
    /// # Arguments
    /// * `raw` - The raw template string containing markup tags.
    /// * `max_depth` - The maximum allowed nesting depth for conditionals and boolean expressions.
    ///
    /// # Errors
    /// Returns a `String` describing the syntax error if the template is malformed or exceeds the maximum depth.
    #[allow(clippy::too_many_lines)]
    pub fn compile_with_depth(raw: &str, max_depth: usize) -> Result<Self, String> {
        enum Frame {
            Root(Vec<Token>),
            If {
                condition: Condition,
                branches: Vec<ConditionalBranch>,
                current_body: Vec<Token>,
            },
            Else {
                branches: Vec<ConditionalBranch>,
                current_body: Vec<Token>,
            },
        }
        impl Frame {
            fn body_mut(&mut self) -> &mut Vec<Token> {
                match self {
                    Frame::Root(b)
                    | Frame::If {
                        current_body: b, ..
                    }
                    | Frame::Else {
                        current_body: b, ..
                    } => b,
                }
            }
        }

        let mut stack = vec![Frame::Root(Vec::new())];
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
                if let Some(&(next_i, next_c)) = chars.peek() {
                    if is_escapable_char(next_c) {
                        let frame = stack.last_mut().ok_or_else(|| {
                            "Internal parser error: Missing root frame".to_string()
                        })?;
                        push_literal(frame.body_mut(), raw, last_literal_start, i);
                        last_literal_start = next_i;
                        chars.next();
                    } else if next_c == 'u' {
                        let frame = stack.last_mut().ok_or_else(|| {
                            "Internal parser error: Missing root frame".to_string()
                        })?;
                        push_literal(frame.body_mut(), raw, last_literal_start, i);
                        chars.next(); // Consume 'u'

                        let mut decoded = String::new();
                        process_unicode_escape(&mut chars, &mut decoded);

                        if let Some(Token::Literal(last)) = frame.body_mut().last_mut() {
                            last.push_str(&decoded);
                        } else {
                            frame.body_mut().push(Token::Literal(decoded));
                        }

                        if let Some(&(curr_i, _)) = chars.peek() {
                            last_literal_start = curr_i;
                        } else {
                            last_literal_start = raw.len();
                        }
                    } else if next_c == '\n' || next_c == '\r' {
                        let frame = stack.last_mut().ok_or_else(|| {
                            "Internal parser error: Missing root frame".to_string()
                        })?;
                        push_literal(frame.body_mut(), raw, last_literal_start, i);

                        if next_c == '\r' {
                            chars.next();
                            if let Some(&(_, '\n')) = chars.peek() {
                                chars.next();
                            }
                        } else {
                            chars.next();
                        }

                        while let Some(&(_, w_c)) = chars.peek() {
                            if w_c == ' ' || w_c == '\t' {
                                chars.next();
                            } else {
                                break;
                            }
                        }

                        if let Some(&(curr_i, _)) = chars.peek() {
                            last_literal_start = curr_i;
                        } else {
                            last_literal_start = raw.len();
                        }
                    }
                }
                continue;
            }

            if c == TAG_ENTITY_OPEN || c == TAG_VERB_OPEN {
                let mut lookahead = chars.clone();
                lookahead.next(); // Skip the '{' or '['

                if c == TAG_ENTITY_OPEN
                    && let Some(&(next_i, next_c)) = lookahead.peek()
                {
                    if next_c == '#' {
                        if let Some(end_rel) = raw[next_i..].find("#}") {
                            let frame = stack.last_mut().ok_or_else(|| {
                                "Internal parser error: Missing root frame".to_string()
                            })?;
                            push_literal(frame.body_mut(), raw, last_literal_start, i);
                            let end_idx = next_i + end_rel;
                            while let Some(&(curr_i, _)) = chars.peek() {
                                if curr_i <= end_idx + 1 {
                                    chars.next();
                                } else {
                                    break;
                                }
                            }
                            last_literal_start = end_idx + 2;
                            continue;
                        }
                        return Err(format!("Unclosed comment starting at index {i}"));
                    } else if next_c == '%' {
                        chars.next(); // Consume '{'
                        chars.next(); // Consume '%'

                        let end_idx = consume_control_tag(&mut chars, i)?;

                        let frame = stack.last_mut().ok_or_else(|| {
                            "Internal parser error: Missing root frame".to_string()
                        })?;
                        push_literal(frame.body_mut(), raw, last_literal_start, i);
                        let content = raw[next_i + 1..end_idx].trim();

                        if let Some(cond_str) = content.strip_prefix("if ") {
                            if stack.len() > max_depth {
                                return Err(format!(
                                    "Maximum template nesting depth of {max_depth} exceeded"
                                ));
                            }
                            let condition = Condition::parse(cond_str, max_depth)?;
                            stack.push(Frame::If {
                                condition,
                                branches: Vec::new(),
                                current_body: Vec::new(),
                            });
                        } else if let Some(cond_str) = content.strip_prefix("elif ") {
                            let condition = Condition::parse(cond_str, max_depth)?;
                            let last = stack.last_mut().ok_or_else(|| {
                                "Unexpected {% elif %} without an open {% if %}".to_string()
                            })?;
                            match last {
                                Frame::If {
                                    condition: old_cond,
                                    branches,
                                    current_body,
                                } => {
                                    branches.push(ConditionalBranch {
                                        condition: old_cond.clone(),
                                        body: std::mem::take(current_body),
                                    });
                                    *old_cond = condition;
                                }
                                _ => {
                                    return Err("Unexpected {% elif %} in this context".to_string());
                                }
                            }
                        } else if content == "else" {
                            let last = stack.pop().ok_or_else(|| {
                                "Unexpected {% else %} without an open {% if %}".to_string()
                            })?;
                            match last {
                                Frame::If {
                                    condition,
                                    mut branches,
                                    current_body,
                                } => {
                                    branches.push(ConditionalBranch {
                                        condition,
                                        body: current_body,
                                    });
                                    stack.push(Frame::Else {
                                        branches,
                                        current_body: Vec::new(),
                                    });
                                }
                                _ => {
                                    return Err("Unexpected {% else %} in this context".to_string());
                                }
                            }
                        } else if content == "endif" {
                            let last = stack.pop().ok_or_else(|| {
                                "Unexpected {% endif %} without an open {% if %}".to_string()
                            })?;
                            let (branches, fallback) = match last {
                                Frame::If {
                                    condition,
                                    mut branches,
                                    current_body,
                                } => {
                                    branches.push(ConditionalBranch {
                                        condition,
                                        body: current_body,
                                    });
                                    (branches, None)
                                }
                                Frame::Else {
                                    branches,
                                    current_body,
                                } => (branches, Some(current_body)),
                                Frame::Root(_) => {
                                    return Err("Unexpected {% endif %}".to_string());
                                }
                            };
                            let frame = stack.last_mut().ok_or_else(|| {
                                "Internal parser error: Missing root frame".to_string()
                            })?;
                            frame
                                .body_mut()
                                .push(Token::Conditional { branches, fallback });
                        } else {
                            return Err(format!("Unknown control tag: {{% {content} %}}"));
                        }

                        while let Some(&(curr_i, _)) = chars.peek() {
                            if curr_i <= end_idx + 1 {
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        last_literal_start = end_idx + 2;
                        continue;
                    }
                }

                // Push any preceding literal text
                let frame = stack
                    .last_mut()
                    .ok_or_else(|| "Internal parser error: Missing root frame".to_string())?;
                push_literal(frame.body_mut(), raw, last_literal_start, i);
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
                    if content.trim_start().starts_with('$') {
                        Self::parse_variable(content)?
                    } else {
                        Self::parse_entity(content)?
                    }
                } else {
                    Self::parse_verb(content)?
                };

                let frame = stack
                    .last_mut()
                    .ok_or_else(|| "Internal parser error: Missing root frame".to_string())?;
                frame.body_mut().push(token);
                last_literal_start = end_idx + 1;
            } else {
                // Move to the next character if it's not a special tag
                chars.next();
            }
        }

        // Push any remaining literal text at the end of the string
        let frame = stack
            .last_mut()
            .ok_or_else(|| "Internal parser error: Missing root frame".to_string())?;
        push_literal(frame.body_mut(), raw, last_literal_start, raw.len());

        if stack.len() != 1 {
            return Err("Unclosed {% if %} tag at the end of the template".to_string());
        }

        let Frame::Root(tokens) = stack
            .pop()
            .ok_or_else(|| "Internal parser error: Missing root frame".to_string())?
        else {
            return Err("Internal parser error: Invalid root frame structure".to_string());
        };

        let mut template_keys = Vec::new();
        let mut seen_keys = HashSet::new();
        Self::extract_keys(&tokens, &mut seen_keys, &mut template_keys);

        Ok(Template {
            tokens,
            template_keys,
            estimated_length: raw.len() + (raw.len() / 5),
        })
    }

    #[allow(clippy::too_many_lines)]
    fn extract_keys<'a>(tokens: &'a [Token], seen: &mut HashSet<&'a str>, keys: &mut Vec<String>) {
        fn check_entity_segment<'a>(
            seg: &'a TagSegment,
            seen: &mut HashSet<&'a str>,
            keys: &mut Vec<String>,
        ) {
            match seg {
                TagSegment::Literal(k) => {
                    if seen.insert(k.as_str()) {
                        keys.push(k.clone());
                    }
                }
                TagSegment::Variable { key: k, .. } => {
                    if let Some((ent, _)) = k.rsplit_once('.')
                        && seen.insert(ent)
                    {
                        keys.push(ent.to_string());
                    }
                }
            }
        }

        fn check_var_segment<'a>(
            seg: &'a TagSegment,
            seen: &mut HashSet<&'a str>,
            keys: &mut Vec<String>,
        ) {
            if let TagSegment::Variable { key: k, .. } = seg
                && let Some((ent, _)) = k.rsplit_once('.')
                && seen.insert(ent)
            {
                keys.push(ent.to_string());
            }
        }

        fn check_val<'a>(
            val: &'a ConditionValue,
            seen: &mut HashSet<&'a str>,
            keys: &mut Vec<String>,
        ) {
            if let ConditionValue::EntityProperty(e, _) = val
                && seen.insert(e.as_str())
            {
                keys.push(e.clone());
            }
        }

        for token in tokens {
            match token {
                Token::EntityRef {
                    key,
                    owner_key,
                    article,
                    p_type,
                    adjectives,
                    ..
                } => {
                    check_entity_segment(key, seen, keys);
                    if let Some(ok) = owner_key {
                        check_entity_segment(ok, seen, keys);
                    }
                    if let Some(art) = article {
                        check_var_segment(art, seen, keys);
                    }
                    if let Some(pt) = p_type {
                        check_var_segment(pt, seen, keys);
                    }
                    if let Some(adj) = adjectives {
                        check_var_segment(adj, seen, keys);
                    }
                }
                Token::VerbRef {
                    subject_key,
                    dynamic_key,
                    ..
                } => {
                    if let Some(sk) = subject_key {
                        check_entity_segment(sk, seen, keys);
                    }
                    if let Some(dk) = dynamic_key
                        && let Some((ent, _)) = dk.rsplit_once('.')
                        && seen.insert(ent)
                    {
                        keys.push(ent.to_string());
                    }
                }
                Token::VariableRef { key, .. } => {
                    if let Some((ent, _)) = key.rsplit_once('.')
                        && seen.insert(ent)
                    {
                        keys.push(ent.to_string());
                    }
                }
                Token::Conditional { branches, fallback } => {
                    for branch in branches {
                        fn check_cond<'a>(
                            c: &'a Condition,
                            seen: &mut HashSet<&'a str>,
                            keys: &mut Vec<String>,
                        ) {
                            match c {
                                Condition::Value(val) => check_val(val, seen, keys),
                                Condition::Not(inner) => check_cond(inner, seen, keys),
                                Condition::And(l, r) | Condition::Or(l, r) => {
                                    check_cond(l, seen, keys);
                                    check_cond(r, seen, keys);
                                }
                                Condition::Eq(v1, v2)
                                | Condition::NotEq(v1, v2)
                                | Condition::Gt(v1, v2)
                                | Condition::Lt(v1, v2)
                                | Condition::GtEq(v1, v2)
                                | Condition::LtEq(v1, v2) => {
                                    check_val(v1, seen, keys);
                                    check_val(v2, seen, keys);
                                }
                            }
                        }
                        check_cond(&branch.condition, seen, keys);
                        Self::extract_keys(&branch.body, seen, keys);
                    }
                    if let Some(fb) = fallback {
                        Self::extract_keys(fb, seen, keys);
                    }
                }
                _ => {}
            }
        }
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
                    let article = if p1.is_empty() { None } else { Some(*p1) };

                    if is_p_type(p3) || p3.is_empty() {
                        Ok((article, None, *p2, Some(*p3)))
                    } else {
                        Ok((article, Some(*p2), *p3, None))
                    }
                } else if is_p_type(p3) || p3.is_empty() {
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
                } else if p1_clean.is_empty() {
                    let article = if p1.is_empty() { None } else { Some(*p1) };
                    Ok((article, None, *p2, None))
                } else {
                    Ok((None, Some(*p1), *p2, None))
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
            key: TagSegment::parse(&clean_key.to_lowercase()),
            article: clean_article.map(TagSegment::parse),
            p_type: clean_p_type.map(|s| TagSegment::parse(&s.to_lowercase())),
            owner_key: owner_key.map(|s| TagSegment::parse(&s)),
            owner_flags,
            adjectives: adjectives.map(|s| TagSegment::parse(&s)),
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
            let (owner_str, adj_str) = if let Some((idx, len)) = find_spaced_possessive(owner_part)
            {
                (&owner_part[..idx], &owner_part[idx + len..])
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
                ("", owner_part)
            };

            if !owner_str.is_empty() {
                let (clean_owner, mut o_flags) = parse_stance_prefixes(owner_str);
                let clean_owner = clean_owner.trim();
                o_flags.set(TagFlags::IS_CAPITALIZED, is_capitalized(clean_owner));
                owner_key = Some(clean_owner.to_lowercase());
                owner_flags = o_flags;
            }

            let adj = adj_str.trim();
            if !adj.is_empty() {
                adjectives = Some(adj.to_string());
            }
        } else if let Some((idx, len)) = find_spaced_possessive(working_key) {
            let owner_part = &working_key[..idx];
            let (clean_owner, mut o_flags) = parse_stance_prefixes(owner_part);
            let clean_owner = clean_owner.trim();
            o_flags.set(TagFlags::IS_CAPITALIZED, is_capitalized(clean_owner));
            owner_key = Some(clean_owner.to_lowercase());
            owner_flags = o_flags;

            let remainder = &working_key[idx + len..];
            if let Some(space_idx) = remainder.rfind(' ') {
                adjectives = Some(remainder[..space_idx].trim().to_string());
                working_key = &remainder[space_idx + 1..];
            } else {
                working_key = remainder;
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

        let parsed_verb = Self::process_verb_conjugations(verb_part, content)?;

        let original_verb = parsed_verb.base_verb.to_string();
        // Capitalize the dynamic variable name (e.g. {Action}) to flag the engine to capitalize the resulting verb
        let is_capitalized = parsed_verb
            .dynamic_key
            .as_deref()
            .is_some_and(is_capitalized)
            || is_capitalized(&original_verb);
        let lower_verb = original_verb.to_lowercase();

        Self::emit_verb_warnings(
            &original_verb,
            &lower_verb,
            parsed_verb.dynamic_key.is_some(),
        );

        let mut flags = p1_flags;
        flags.set(TagFlags::IS_CAPITALIZED, is_capitalized);
        flags.set(TagFlags::ALL_CAPS, is_all_caps);

        Ok(Token::VerbRef {
            subject_key: subject_key.map(|s| TagSegment::parse(&s.to_lowercase())),
            original_verb,
            lower_verb,
            dynamic_key: parsed_verb.dynamic_key.map(|k| k.to_lowercase()),
            dynamic_fallback: parsed_verb.dynamic_fallback,
            forced_present: parsed_verb.forced_present,
            forced_past: parsed_verb.forced_past,
            flags,
        })
    }

    fn parse_variable(content: &str) -> Result<Token, String> {
        let is_all_caps = is_all_caps(content);
        let clean = content.trim_start().strip_prefix('$').unwrap_or(content);

        let (key, fallback) = extract_variable_fallback(clean);

        reject_if(
            key.is_empty(),
            "Variable tag has an empty key",
            content,
            TAG_ENTITY_OPEN,
        )?;

        let mut flags = TagFlags::empty();
        flags.set(TagFlags::IS_CAPITALIZED, is_capitalized(key));
        flags.set(TagFlags::ALL_CAPS, is_all_caps);

        Ok(Token::VariableRef {
            key: key.to_lowercase(),
            fallback,
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
    ) -> Result<ParsedVerb<'a>, String> {
        let (base_verb, forced) = if let Some((bv, f)) = verb_part.split_once(VERB_FORM_SEP) {
            (bv.trim(), Some(f.trim()))
        } else {
            (verb_part.trim(), None)
        };

        let (dynamic_key, dynamic_fallback) = if let Some(var_name) = base_verb.strip_prefix('$') {
            let (k, f) = extract_variable_fallback(var_name);
            (Some(k.to_string()), f)
        } else {
            (None, None)
        };

        if let Some(forced_str) = forced {
            reject_if(
                base_verb.is_empty() || forced_str.is_empty(),
                "Verb tag has an empty verb or forced conjugation segment",
                content,
                TAG_VERB_OPEN,
            )?;

            let (forced_present, forced_past) = parse_forced_conjugations(forced_str, content)?;
            Ok(ParsedVerb {
                dynamic_key,
                dynamic_fallback,
                base_verb,
                forced_present,
                forced_past,
            })
        } else {
            Ok(ParsedVerb {
                dynamic_key,
                dynamic_fallback,
                base_verb,
                forced_present: None,
                forced_past: None,
            })
        }
    }

    #[inline]
    fn emit_verb_warnings(original_verb: &str, lower_verb: &str, is_dynamic: bool) {
        if is_dynamic {
            return;
        }

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
