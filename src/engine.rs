use crate::grammar::{
    capitalize_cow, conjugate_verb, push_capitalized_if, resolve_article, resolve_pronoun,
};
use crate::models::{NULL_VIEWER, RenderContext, TemplateEntity};
use crate::parser::MOD_POSSESSIVE;
pub use crate::parser::{TagFlags, Template, Token};
use crate::typography::{
    NO_SENTENCE_BREAK_SENTINEL, SENTENCE_BREAK_SENTINEL, apply_all_caps, post_process_typography,
};
#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
use crate::typography::{has_protocol_tags, skip_protocol_tags};
use std::collections::HashMap;

/// Anaphora resolution and memory mutation logic.
pub mod anaphora;
/// Disambiguation and naming collision logic.
pub mod disambiguation;
/// Group formatting, Oxford comma distribution, and nested list flattening logic.
pub mod groups;

/// Parameters extracted from a token or fallback logic to render an entity.
pub(crate) struct EntityRefParams<'p> {
    key: &'p str,
    article: Option<&'p str>,
    p_type: Option<&'p str>,
    owner_key: Option<&'p str>,
    owner_flags: TagFlags,
    adjectives: Option<&'p str>,
    flags: TagFlags,
    ordinal: Option<usize>,
}

enum PronounResult<'a> {
    Rendered,
    Fallback {
        article_to_use: Option<&'a str>,
        active_flags: TagFlags,
        actual_p_type: Option<&'a str>,
    },
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct PronounContextFlags: u8 {
        const IS_PLURAL         = 1 << 0;
        const IS_GROUP          = 1 << 1;
        const IS_VIEWER         = 1 << 2;
        const IS_ACTIVE_SUBJECT = 1 << 3;
        const IS_REFLEXIVE      = 1 << 4;
        const ALREADY_SEEN      = 1 << 5;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct ViewerNameFlags: u8 {
        const IS_PLURAL      = 1 << 0;
        const IS_POSSESSIVE  = 1 << 1;
        const IS_CAPITALIZED = 1 << 2;
        const IS_OBJ         = 1 << 3;
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
            if let Ok(ent) = crate::evaluator::get_entity(ctx, k) {
                pre_resolved.insert(k.as_str(), ent);
            }
        }

        // 1. Pre-allocate buffer to prevent continuous heap allocations
        let mut raw_output = String::with_capacity(template.estimated_length);
        let mut caps_buffer = String::new();

        Self::render_tokens(
            ctx,
            &template.tokens,
            &mut raw_output,
            &mut caps_buffer,
            &pre_resolved,
            &future_keys,
        )?;

        if ctx.auto_clear {
            ctx.clear_anaphora();
        }

        // 2. Pass the fully assembled base-case string to the typography post-processor
        Ok(post_process_typography(&raw_output))
    }

    fn render_tokens<'a>(
        ctx: &'a RenderContext,
        tokens: &[Token],
        raw_output: &mut String,
        caps_buffer: &mut String,
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
        future_keys: &[&str],
    ) -> Result<(), String> {
        for token in tokens {
            let start_len = raw_output.len();
            let mut all_caps = false;

            match token {
                Token::Literal(text) => raw_output.push_str(text),
                Token::EntityRef { flags, .. } => {
                    all_caps = flags.contains(TagFlags::ALL_CAPS);
                    Self::render_entity_token(ctx, raw_output, token, pre_resolved, future_keys)?;
                }
                Token::VerbRef { flags, .. } => {
                    all_caps = flags.contains(TagFlags::ALL_CAPS);
                    Self::render_verb_ref(ctx, raw_output, token, pre_resolved)?;
                }
                Token::VariableRef {
                    key,
                    fallback,
                    flags,
                } => {
                    all_caps = flags.contains(TagFlags::ALL_CAPS);
                    Self::render_variable_ref(
                        ctx,
                        raw_output,
                        key,
                        fallback.as_deref(),
                        *flags,
                        pre_resolved,
                    )?;
                }
                Token::SentenceBreak => raw_output.push(SENTENCE_BREAK_SENTINEL),
                Token::NoSentenceBreak => raw_output.push(NO_SENTENCE_BREAK_SENTINEL),
                Token::Conditional { .. } => {
                    Self::render_conditional_token(
                        ctx,
                        token,
                        raw_output,
                        caps_buffer,
                        pre_resolved,
                        future_keys,
                    )?;
                }
            }

            if all_caps && raw_output.len() > start_len {
                caps_buffer.clear();
                apply_all_caps(&raw_output[start_len..], caps_buffer);
                raw_output.truncate(start_len);
                raw_output.push_str(caps_buffer);
            }
        }
        Ok(())
    }

    fn render_entity_token<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        token: &Token,
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
        future_keys: &[&str],
    ) -> Result<(), String> {
        let Token::EntityRef {
            key,
            article,
            p_type,
            owner_key,
            owner_flags,
            adjectives,
            flags,
        } = token
        else {
            return Ok(());
        };

        let resolved_key = crate::evaluator::resolve_tag_segment(ctx, key, pre_resolved)?;
        let resolved_article = article
            .as_ref()
            .map(|a| crate::evaluator::resolve_tag_segment(ctx, a, pre_resolved))
            .transpose()?;
        let resolved_p_type = p_type
            .as_ref()
            .map(|p| crate::evaluator::resolve_tag_segment(ctx, p, pre_resolved))
            .transpose()?;
        let resolved_owner_key = owner_key
            .as_ref()
            .map(|o| crate::evaluator::resolve_tag_segment(ctx, o, pre_resolved))
            .transpose()?;
        let resolved_adjectives = adjectives
            .as_ref()
            .map(|a| crate::evaluator::resolve_tag_segment(ctx, a, pre_resolved))
            .transpose()?;

        Self::render_entity_ref(
            ctx,
            raw_output,
            &EntityRefParams {
                key: resolved_key.as_ref(),
                article: resolved_article.as_deref(),
                p_type: resolved_p_type.as_deref(),
                owner_key: resolved_owner_key.as_deref(),
                owner_flags: *owner_flags,
                adjectives: resolved_adjectives.as_deref(),
                flags: *flags,
                ordinal: None,
            },
            future_keys,
            pre_resolved,
        )
    }

    fn render_conditional_token<'a>(
        ctx: &'a RenderContext,
        token: &Token,
        raw_output: &mut String,
        caps_buffer: &mut String,
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
        future_keys: &[&str],
    ) -> Result<(), String> {
        let Token::Conditional { branches, fallback } = token else {
            return Ok(());
        };

        let mut matched = false;
        for branch in branches {
            if crate::evaluator::evaluate_condition(ctx, &branch.condition, pre_resolved) {
                Self::render_tokens(
                    ctx,
                    &branch.body,
                    raw_output,
                    caps_buffer,
                    pre_resolved,
                    future_keys,
                )?;
                matched = true;
                break;
            }
        }
        if !matched && let Some(fb) = fallback {
            Self::render_tokens(ctx, fb, raw_output, caps_buffer, pre_resolved, future_keys)?;
        }
        Ok(())
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
            .map_or_else(|| crate::evaluator::get_entity(ctx, params.key), Ok)?;

        let effective_viewer = effective_viewer_id(ctx, params.flags.force_3rd_person());

        if let Some(owner_key) = params.owner_key {
            return Self::render_narrative_possessive(
                ctx,
                raw_output,
                params,
                entity,
                effective_viewer,
                owner_key,
                future_keys,
                pre_resolved,
            );
        }

        Self::render_target_entity(
            ctx,
            raw_output,
            params,
            entity,
            effective_viewer,
            false,
            future_keys,
            pre_resolved,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_narrative_possessive<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        params: &EntityRefParams<'_>,
        entity: &'a dyn TemplateEntity,
        effective_viewer: &str,
        owner_key: &str,
        future_keys: &[&str],
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
    ) -> Result<(), String> {
        let mut target_params = EntityRefParams {
            article: None,
            owner_flags: TagFlags::empty(),
            flags: params.flags,
            ..*params
        };
        target_params.flags.remove(TagFlags::ARTICLE_CAPITALIZED);
        target_params.flags.remove(TagFlags::ARTICLE_INDEFINITE);
        target_params.flags.remove(TagFlags::FORCE_ARTICLE);

        // Evaluate the target FIRST. If the target naturally evaluates to a pronoun (like "you"
        // or "it" from anaphora memory), we gracefully drop the owner and adjectives entirely.
        let (mut target_active_flags, actual_target_p_type, target_article) =
            match Self::try_render_pronoun(
                ctx,
                raw_output,
                &target_params,
                entity,
                effective_viewer,
                pre_resolved,
            )? {
                PronounResult::Rendered => return Ok(()),
                PronounResult::Fallback {
                    active_flags,
                    actual_p_type,
                    article_to_use,
                } => (active_flags, actual_p_type, article_to_use),
            };

        // If it's a proper noun and the @ override is present, drop the possessive owner!
        if entity.is_proper_noun_for(effective_viewer) && params.flags.drop_possessive() {
            let mut dropped_owner_flags = target_active_flags;
            if params.flags.article_capitalized() {
                dropped_owner_flags.insert(TagFlags::ARTICLE_CAPITALIZED);
            }
            if params.flags.article_indefinite() {
                dropped_owner_flags.insert(TagFlags::ARTICLE_INDEFINITE);
            }
            if params.flags.force_article() {
                dropped_owner_flags.insert(TagFlags::FORCE_ARTICLE);
            }

            Self::render_resolved_entity(
                ctx,
                raw_output,
                params,
                entity,
                effective_viewer,
                params.article,
                actual_target_p_type,
                dropped_owner_flags,
                false,
                future_keys,
                pre_resolved,
            );
            return Ok(());
        }

        Self::render_narrative_owner(
            ctx,
            raw_output,
            params,
            owner_key,
            future_keys,
            pre_resolved,
        )?;

        // The target article is unconditionally suppressed because it follows the possessive owner
        target_active_flags.remove(TagFlags::FORCE_ARTICLE);

        Self::render_resolved_entity(
            ctx,
            raw_output,
            &target_params,
            entity,
            effective_viewer,
            target_article, // Pass the fallback article so target ordinals can generate
            actual_target_p_type,
            target_active_flags,
            true, // after_possessive = true
            future_keys,
            pre_resolved,
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn render_narrative_owner<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        params: &EntityRefParams<'_>,
        owner_key: &str,
        future_keys: &[&str],
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
    ) -> Result<(), String> {
        let owner_entity = pre_resolved
            .get(owner_key)
            .copied()
            .map_or_else(|| crate::evaluator::get_entity(ctx, owner_key), Ok)?;

        let owner_viewer = effective_viewer_id(ctx, params.owner_flags.force_3rd_person());

        let mut o_flags = params.owner_flags;
        o_flags.set(
            TagFlags::ALL_CAPS,
            params.flags.contains(TagFlags::ALL_CAPS),
        );
        if params.flags.article_capitalized() {
            o_flags.set(TagFlags::PRONOUN_CAPITALIZED, true);
        }
        if params.flags.force_article() {
            o_flags.set(TagFlags::FORCE_ARTICLE, true);
        }
        if params.flags.article_indefinite() {
            o_flags.set(TagFlags::ARTICLE_INDEFINITE, true);
        }

        let owner_params = EntityRefParams {
            key: owner_key,
            article: params.article, // Pass the root article directly to the owner!
            p_type: Some("poss"),
            owner_key: None,
            owner_flags: TagFlags::empty(),
            adjectives: None,
            flags: o_flags,
            ordinal: None,
        };

        Self::render_target_entity(
            ctx,
            raw_output,
            &owner_params,
            owner_entity,
            owner_viewer,
            false,
            future_keys,
            pre_resolved,
        )?;

        raw_output.push(' ');

        if let Some(adj) = params.adjectives
            && !adj.is_empty()
        {
            if params.flags.contains(TagFlags::ALL_CAPS) {
                apply_all_caps(adj, raw_output);
            } else {
                raw_output.push_str(adj);
            }
            raw_output.push(' ');
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn render_target_entity<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        params: &EntityRefParams<'_>,
        entity: &'a dyn TemplateEntity,
        effective_viewer: &str,
        after_possessive: bool,
        future_keys: &[&str],
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
    ) -> Result<(), String> {
        let (article_to_use, active_flags, actual_p_type) = match Self::try_render_pronoun(
            ctx,
            raw_output,
            params,
            entity,
            effective_viewer,
            pre_resolved,
        )? {
            PronounResult::Rendered => return Ok(()),
            PronounResult::Fallback {
                article_to_use,
                active_flags,
                actual_p_type,
            } => (article_to_use, active_flags, actual_p_type),
        };

        Self::render_resolved_entity(
            ctx,
            raw_output,
            params,
            entity,
            effective_viewer,
            article_to_use,
            actual_p_type,
            active_flags,
            after_possessive,
            future_keys,
            pre_resolved,
        );

        Ok(())
    }

    #[inline]
    fn try_render_pronoun<'a>(
        ctx: &RenderContext,
        raw_output: &mut String,
        params: &EntityRefParams<'a>,
        entity: &dyn TemplateEntity,
        effective_viewer: &str,
        pre_resolved: &HashMap<&str, &dyn TemplateEntity>,
    ) -> Result<PronounResult<'a>, String> {
        let mut article_to_use = params.article;
        let mut active_flags = params.flags;
        let mut actual_p_type = params.p_type;

        let Some(p_type) = params.p_type else {
            return Ok(PronounResult::Fallback {
                article_to_use,
                active_flags,
                actual_p_type,
            });
        };

        let mut is_plural = entity.is_plural();
        let mut effective_gender = entity.gender();
        let is_group = entity.group_members().is_some();
        let extract_member = params.flags.extract_group_member() && is_group;

        if params.flags.force_singular() || extract_member {
            is_plural = false;
            if let Some(members) = entity.group_members() {
                effective_gender = groups::determine_group_singular_gender(members);
            } else if effective_gender == crate::models::Gender::Plural {
                effective_gender = crate::models::Gender::Neutral;
            }
        }

        let is_viewer = entity.contains_viewer(effective_viewer) && !extract_member;
        let is_active_subject =
            anaphora::check_is_active_subject(ctx, entity, params.key, pre_resolved);

        if is_active_subject && p_type == "obj" {
            actual_p_type = Some("reflex");
        }

        let already_seen = ctx
            .recent_entities
            .borrow()
            .iter()
            .any(|r| r.key == params.key);
        let is_reflexive = actual_p_type == Some("reflex");

        let mut pronoun_ctx = PronounContextFlags::empty();
        pronoun_ctx.set(PronounContextFlags::IS_PLURAL, is_plural);
        pronoun_ctx.set(PronounContextFlags::IS_GROUP, is_group);
        pronoun_ctx.set(PronounContextFlags::IS_VIEWER, is_viewer);
        pronoun_ctx.set(PronounContextFlags::IS_ACTIVE_SUBJECT, is_active_subject);
        pronoun_ctx.set(PronounContextFlags::IS_REFLEXIVE, is_reflexive);
        pronoun_ctx.set(PronounContextFlags::ALREADY_SEEN, already_seen);

        let can_use_pronoun = Self::evaluate_can_use_pronoun(
            ctx,
            params.key,
            effective_gender,
            pronoun_ctx,
            active_flags,
        );

        if can_use_pronoun {
            if !already_seen {
                anaphora::update_memory(&ctx.last_mentioned, params.key);
                anaphora::track_recent_entity(ctx, params.key, entity, params.adjectives, None);
            }
            let pronoun = resolve_pronoun(
                effective_gender,
                actual_p_type.unwrap_or(p_type),
                is_viewer,
                is_plural,
                ctx.stance,
            )?;
            let cap_pronoun = active_flags.contains(TagFlags::PRONOUN_CAPITALIZED)
                || active_flags.is_capitalized()
                || active_flags.article_capitalized();
            push_capitalized_if(raw_output, pronoun, cap_pronoun);
            return Ok(PronounResult::Rendered);
        }

        if actual_p_type == Some("poss") || actual_p_type == Some("abs_poss") {
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
            article_to_use = Some(fallback_article);
            active_flags.set(
                TagFlags::ARTICLE_INDEFINITE,
                !params.flags.force_singular() || !entity.is_plural(),
            );
            active_flags.set(TagFlags::ARTICLE_CAPITALIZED, is_cap); // the fallback inherits the pronoun's requested capitalization
            active_flags.set(TagFlags::IS_CAPITALIZED, false); // We don't want to force-capitalize common nouns ("A Goblin")
        }

        Ok(PronounResult::Fallback {
            article_to_use,
            active_flags,
            actual_p_type,
        })
    }

    #[inline]
    fn evaluate_can_use_pronoun(
        ctx: &RenderContext,
        key: &str,
        effective_gender: crate::models::Gender,
        pronoun_ctx: PronounContextFlags,
        active_flags: TagFlags,
    ) -> bool {
        let is_group = pronoun_ctx.contains(PronounContextFlags::IS_GROUP);
        let is_viewer = pronoun_ctx.contains(PronounContextFlags::IS_VIEWER);
        let is_active_subject = pronoun_ctx.contains(PronounContextFlags::IS_ACTIVE_SUBJECT);
        let is_reflexive = pronoun_ctx.contains(PronounContextFlags::IS_REFLEXIVE);
        let already_seen = pronoun_ctx.contains(PronounContextFlags::ALREADY_SEEN);

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
            && !anaphora::is_pronoun_ambiguous(
                ctx,
                key,
                effective_gender,
                pronoun_ctx,
                active_flags,
            )
        {
            can_use_pronoun = true;
        }

        can_use_pronoun
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
        after_possessive: bool,
        future_keys: &[&str],
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
    ) {
        let (name, ordinal) = disambiguation::resolve_display_name(
            ctx,
            entity,
            params,
            effective_viewer,
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

        anaphora::update_memory(&ctx.last_mentioned, params.key);
        anaphora::track_recent_entity(
            ctx,
            params.key,
            entity,
            params.adjectives,
            Some(name.as_ref()),
        );

        let active_params = EntityRefParams {
            key: params.key,
            article: article_to_use,
            p_type: actual_p_type,
            owner_key: None,
            owner_flags: TagFlags::empty(),
            adjectives: if after_possessive {
                None
            } else {
                params.adjectives
            },
            flags: active_flags,
            ordinal,
        };
        let cap_whole = should_capitalize_whole_tag(&active_params);

        // --- Handle Groups / Distributed Lists ---
        if let Some(members) = entity.group_members() {
            groups::render_group_entity(
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
            let mut v_flags = ViewerNameFlags::empty();
            v_flags.set(ViewerNameFlags::IS_PLURAL, is_plural); // This is already `false` if `force_singular` was requested
            v_flags.set(ViewerNameFlags::IS_POSSESSIVE, active_flags.is_possessive());
            v_flags.set(ViewerNameFlags::IS_CAPITALIZED, cap_whole);
            v_flags.set(ViewerNameFlags::IS_OBJ, actual_p_type == Some("obj"));

            raw_output.push_str(viewer_name(ctx.stance, v_flags));
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

        let mut adj_printed = false;
        if let Some(adj) = active_params.adjectives
            && !adj.is_empty()
        {
            let mut formatted_adj = String::new();
            if active_params.flags.contains(TagFlags::ALL_CAPS) {
                crate::typography::apply_all_caps(adj, &mut formatted_adj);
            } else {
                formatted_adj.push_str(adj);
            }

            if !article_printed && cap_whole && !after_possessive {
                raw_output.push_str(&crate::grammar::capitalize_first(&formatted_adj));
            } else {
                raw_output.push_str(&formatted_adj);
            }
            raw_output.push(' ');
            adj_printed = true;
        }

        let should_cap_noun = if article_printed
            || adj_printed
            || (after_possessive && !active_params.flags.force_article())
        {
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
    }

    #[inline]
    pub(crate) fn get_last_visible_char(input: &str) -> Option<char> {
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
    pub(crate) fn get_possessive_suffix(name: &str, is_plural: bool) -> &'static str {
        if matches!(Self::get_last_visible_char(name), Some('s' | 'S')) && is_plural {
            return "'";
        }
        MOD_POSSESSIVE
    }

    fn render_variable_ref<'a>(
        ctx: &'a RenderContext,
        raw_output: &mut String,
        key: &str,
        fallback: Option<&str>,
        flags: TagFlags,
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
    ) -> Result<(), String> {
        if let Some(val) =
            crate::evaluator::resolve_entity_property(ctx, key, fallback, pre_resolved)?
        {
            Self::apply_variable_formatting(val.as_ref(), raw_output, flags);
            return Ok(());
        }

        if let Some(values) = ctx.variables.get(key) {
            if values.is_empty() {
                return Ok(());
            }

            let mut formatted_vals: Vec<std::borrow::Cow<'_, str>> =
                Vec::with_capacity(values.len());
            for (i, v) in values.iter().enumerate() {
                let cap_this = if i == 0 {
                    flags.is_capitalized()
                } else {
                    false
                };
                if cap_this {
                    formatted_vals
                        .push(std::borrow::Cow::Owned(crate::grammar::capitalize_first(v)));
                } else {
                    formatted_vals.push(std::borrow::Cow::Borrowed(v.as_str()));
                }
            }

            let list = crate::grammar::format_oxford_list(formatted_vals, "and");
            raw_output.push_str(&list);
            Ok(())
        } else {
            if let Some(fb) = fallback {
                Self::apply_variable_formatting(fb, raw_output, flags);
                return Ok(());
            }
            tracing::error!("Missing dynamic variable for key '{key}'");
            Err(format!("Missing variable for key: {key}"))
        }
    }

    #[inline]
    fn apply_variable_formatting(val: &str, raw_output: &mut String, flags: TagFlags) {
        let mut formatted = String::new();
        if flags.contains(TagFlags::ALL_CAPS) {
            crate::typography::apply_all_caps(val, &mut formatted);
        } else if flags.is_capitalized() {
            formatted.push_str(&crate::grammar::capitalize_first(val));
        } else {
            formatted.push_str(val);
        }
        raw_output.push_str(&formatted);
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
            dynamic_key,
            dynamic_fallback,
            forced_present,
            forced_past,
            flags,
        } = token
        else {
            return Ok(());
        };

        let resolved_subject_key = subject_key
            .as_ref()
            .map(|s| crate::evaluator::resolve_tag_segment(ctx, s, pre_resolved))
            .transpose()?;

        // Explicitly bind the verb to its subject to solve passive voice / compound subjects
        let (mut is_viewer, mut is_plural, is_group) = if let Some(key) = resolved_subject_key {
            let key_str = key.as_ref();
            let entity = pre_resolved
                .get(key_str)
                .copied()
                .map_or_else(|| crate::evaluator::get_entity(ctx, key_str), Ok)?;
            let effective_viewer = effective_viewer_id(ctx, flags.force_3rd_person());
            anaphora::update_memory(&ctx.active_subject, key_str);
            anaphora::update_memory(&ctx.last_mentioned, key_str);
            anaphora::track_recent_entity(ctx, key_str, entity, None, None);
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

        if let Some(d_key) = dynamic_key {
            return Self::render_dynamic_verb_key(
                ctx,
                raw_output,
                d_key,
                dynamic_fallback.as_deref(),
                *flags,
                is_viewer,
                is_plural,
                pre_resolved,
            );
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

    #[allow(clippy::too_many_arguments)]
    fn render_dynamic_verb_key(
        ctx: &RenderContext,
        raw_output: &mut String,
        d_key: &str,
        dynamic_fallback: Option<&str>,
        flags: TagFlags,
        is_viewer: bool,
        is_plural: bool,
        pre_resolved: &HashMap<&str, &dyn TemplateEntity>,
    ) -> Result<(), String> {
        let mut conjugated_verbs = Vec::new();

        if let Some(val) =
            crate::evaluator::resolve_entity_property(ctx, d_key, dynamic_fallback, pre_resolved)?
        {
            let v_lower = val.to_lowercase();
            let conjugated = conjugate_verb(
                &val,
                &v_lower,
                flags.is_capitalized(),
                is_viewer,
                is_plural,
                ctx.stance,
                ctx.tense,
            );
            conjugated_verbs.push(conjugated.into_owned().into());
        }

        if conjugated_verbs.is_empty() {
            if let Some(verbs) = ctx.variables.get(d_key) {
                if verbs.is_empty() {
                    return Ok(());
                }
                for (i, v) in verbs.iter().enumerate() {
                    let v_lower = v.to_lowercase();
                    let cap_this = if i == 0 {
                        flags.is_capitalized()
                    } else {
                        false
                    };
                    let conjugated = conjugate_verb(
                        v, &v_lower, cap_this, is_viewer, is_plural, ctx.stance, ctx.tense,
                    );
                    conjugated_verbs.push(conjugated.into_owned().into());
                }
            } else if let Some(fb) = dynamic_fallback {
                let fb_lower = fb.to_lowercase();
                let conjugated = conjugate_verb(
                    fb,
                    &fb_lower,
                    flags.is_capitalized(),
                    is_viewer,
                    is_plural,
                    ctx.stance,
                    ctx.tense,
                );
                conjugated_verbs.push(conjugated.into_owned().into());
            } else {
                tracing::error!("Missing dynamic verb variable for key '{d_key}'");
                return Err(format!("Missing variable for key: {d_key}"));
            }
        }

        let list = crate::grammar::format_oxford_list(conjugated_verbs, "and");
        raw_output.push_str(&list);
        Ok(())
    }
}

#[inline]
fn should_capitalize_whole_tag(params: &EntityRefParams<'_>) -> bool {
    params.flags.is_capitalized() || params.flags.article_capitalized()
}

#[inline]
fn viewer_name(stance: crate::models::ActorStance, flags: ViewerNameFlags) -> &'static str {
    let is_plural = flags.contains(ViewerNameFlags::IS_PLURAL);
    let is_possessive = flags.contains(ViewerNameFlags::IS_POSSESSIVE);
    let is_capitalized = flags.contains(ViewerNameFlags::IS_CAPITALIZED);
    let is_obj = flags.contains(ViewerNameFlags::IS_OBJ);

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
fn effective_viewer_id<'a>(ctx: &RenderContext<'a>, force_3rd_person: bool) -> &'a str {
    if force_3rd_person || ctx.stance == crate::models::ActorStance::ThirdPerson {
        NULL_VIEWER
    } else {
        ctx.viewer_id
    }
}
