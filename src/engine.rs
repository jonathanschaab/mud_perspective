use crate::grammar::{
    capitalize_cow, conjugate_verb, push_capitalized_if, resolve_article, resolve_pronoun,
};
use crate::models::{NULL_VIEWER, RenderContext, TemplateEntity, is_same_entity};
use crate::parser::{MOD_POSSESSIVE, TAG_PROPERTY_SEP};
pub use crate::parser::{TagFlags, Template, Token};
use crate::typography::{
    NO_SENTENCE_BREAK_SENTINEL, SENTENCE_BREAK_SENTINEL, apply_all_caps, post_process_typography,
};
#[cfg(any(feature = "mxp", feature = "msp", feature = "ansi"))]
use crate::typography::{has_protocol_tags, skip_protocol_tags};
use std::collections::HashMap;

/// Parameters extracted from a token or fallback logic to render an entity.
struct EntityRefParams<'a> {
    key: &'a str,
    article: Option<&'a str>,
    p_type: Option<&'a str>,
    owner_key: Option<&'a str>,
    owner_flags: TagFlags,
    adjectives: Option<&'a str>,
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
    struct GroupMemberFlags: u8 {
        const AFTER_POSSESSIVE       = 1 << 0;
        const FIRST_VISIBLE_ITEM     = 1 << 1;
        const DISTRIBUTE_POSSESSIVES = 1 << 2;
        const IS_REFLEXIVE           = 1 << 3;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct PronounContextFlags: u8 {
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
        let mut caps_buffer = String::new();

        for token in &template.tokens {
            let start_len = raw_output.len();
            let mut all_caps = false;

            match token {
                Token::Literal(text) => raw_output.push_str(text),
                Token::EntityRef {
                    key,
                    article,
                    p_type,
                    owner_key,
                    owner_flags,
                    adjectives,
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
                            owner_key: owner_key.as_deref(),
                            owner_flags: *owner_flags,
                            adjectives: adjectives.as_deref(),
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
                caps_buffer.clear();
                apply_all_caps(&raw_output[start_len..], &mut caps_buffer);
                raw_output.truncate(start_len);
                raw_output.push_str(&caps_buffer);
            }
        }

        if ctx.auto_clear {
            ctx.clear_anaphora();
        }

        // 2. Pass the fully assembled base-case string to the typography post-processor
        Ok(post_process_typography(&raw_output))
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

    fn try_adjective_disambiguation<'a>(
        entity: &dyn TemplateEntity,
        colliders: &[&dyn TemplateEntity],
        base_name: &str,
        limit: usize,
    ) -> Option<(std::borrow::Cow<'a, str>, bool)> {
        let entity_adjs = entity.adjectives().filter(|adjs| !adjs.is_empty())?;

        // Prevent bitshift overflow by capping the limit to the bit-width of our counter
        let safe_limit = limit.min((u128::BITS - 1) as usize);
        if safe_limit == 0 {
            return None;
        }

        // Filter out adjectives that are shared by ALL colliders, as they provide zero disambiguation value.
        let mut searchable_adjs = Vec::with_capacity(entity_adjs.len().min(safe_limit));
        for &adj in entity_adjs {
            let shared_by_all = colliders.iter().all(|other| {
                other
                    .adjectives()
                    .is_some_and(|other_adjs| other_adjs.contains(&adj))
            });

            if !shared_by_all {
                searchable_adjs.push(adj);
                if searchable_adjs.len() == safe_limit {
                    break;
                }
            }
        }

        if searchable_adjs.is_empty() {
            return None;
        }

        let mut best_combo: Option<Vec<&str>> = None;
        let mut min_colliders = colliders.len();

        let mut subset = Vec::with_capacity(searchable_adjs.len());

        // Iterate through all non-empty subsets of the entity's adjectives to find the smallest set
        // that minimizes the number of colliders sharing those adjectives.
        for i in 1_u128..(1_u128 << searchable_adjs.len()) {
            subset.clear();
            for (j, &adj) in searchable_adjs.iter().enumerate() {
                if (i >> j) & 1 == 1 {
                    subset.push(adj);
                }
            }

            let current_colliders = colliders
                .iter()
                .filter(|other| {
                    if let Some(other_adjs) = other.adjectives() {
                        subset.iter().all(|&adj| other_adjs.contains(&adj))
                    } else {
                        false
                    }
                })
                .count();

            if current_colliders < min_colliders {
                min_colliders = current_colliders;
                best_combo = Some(subset.clone());
            } else if current_colliders == min_colliders {
                // Prefer the smaller subset if it yields the same number of colliders.
                if let Some(best) = &best_combo
                    && subset.len() < best.len()
                {
                    best_combo = Some(subset.clone());
                }
            }
        }

        if let Some(combo) = best_combo {
            // To maintain a stable order, we sort the adjectives before joining.
            let mut sorted_combo = combo;
            sorted_combo.sort_unstable();
            let prefix = sorted_combo.join(" ");
            Some((
                std::borrow::Cow::Owned(format!("{prefix} {base_name}")),
                min_colliders > 0,
            ))
        } else {
            None
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn count_long_name_collisions(
        ctx: &RenderContext,
        effective_viewer: &str,
        recent_borrow: &[crate::models::RecentEntity],
        future_keys: &[&str],
        pre_resolved: &HashMap<&str, &dyn TemplateEntity>,
        params_key: &str,
        short_name: &str,
        long_name: &str,
    ) -> usize {
        let mut long_collisions = 0;

        // Verify how many times the long name collides
        let mut check_long = |other_key: &str, other_entity: &dyn TemplateEntity| {
            if other_key != params_key {
                let other_short = other_entity.display_name_for(effective_viewer);
                // Only consider the other entity's long name if its short name is in the
                // exact same collision group as our entity's short name (preventing phantoms).
                if other_short == long_name
                    || (other_short == short_name
                        && other_entity
                            .long_display_name_for(effective_viewer)
                            .as_deref()
                            == Some(long_name))
                {
                    long_collisions += 1;
                }
            }
        };

        for r in recent_borrow {
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

        long_collisions
    }

    #[inline]
    fn resolve_display_name<'a>(
        ctx: &'a RenderContext,
        entity: &'a dyn TemplateEntity,
        params: &EntityRefParams<'_>,
        effective_viewer: &str,
        future_keys: &[&str],
        pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
    ) -> (std::borrow::Cow<'a, str>, Option<usize>) {
        let mut name = entity.display_name_for(effective_viewer);
        let mut name_collision = false;

        if !params.flags.no_smart() {
            let mut short_collisions = 0;
            let mut unresolved_colliders: Vec<&'a dyn TemplateEntity> = Vec::new();
            let recent_borrow = ctx.recent_entities.borrow();

            // We use a closure here to iterate over the live `recent_borrow` and `future_keys`.
            // Evaluating collisions dynamically avoids allocations while ensuring we accurately
            // catch entities that were just introduced in this template, maintaining strict
            // left-to-right chronological accuracy.
            let mut check_collision = |other_key: &str, other_entity: &'a dyn TemplateEntity| {
                if other_key != params.key
                    && other_entity.display_name_for(effective_viewer) == name
                {
                    short_collisions += 1; // This counts all collisions, even those that will be resolved by long names.

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
                        unresolved_colliders.push(other_entity);
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

            name_collision = !unresolved_colliders.is_empty();

            if short_collisions > 0
                && let Some(long_name) = entity.long_display_name_for(effective_viewer)
                && long_name != name
            {
                let long_collisions = Self::count_long_name_collisions(
                    ctx,
                    effective_viewer,
                    &recent_borrow,
                    future_keys,
                    pre_resolved,
                    params.key,
                    name.as_ref(),
                    long_name.as_ref(),
                );

                // If the long name is strictly more specific (has fewer collisions), use it!
                if long_collisions < short_collisions {
                    name = long_name;
                    name_collision = long_collisions > 0;
                }
            }

            // If a collision still exists, try to disambiguate by prepending unique adjectives.
            if name_collision
                && !entity.is_proper_noun_for(effective_viewer)
                && let Some((adj_name, still_collides)) = Self::try_adjective_disambiguation(
                    entity,
                    &unresolved_colliders,
                    &name,
                    ctx.adjective_disambiguation_limit,
                )
            {
                name = adj_name;
                name_collision = still_collides;
            }
        }

        let mut ordinal = None;

        if !params.flags.no_smart() {
            let namespace = params
                .owner_key
                .or_else(|| params.key.rsplit_once(TAG_PROPERTY_SEP).map(|(p, _)| p));
            let ordinal_group_name = if let Some(ns) = namespace {
                std::borrow::Cow::Owned(format!("{ns}::{name}"))
            } else {
                std::borrow::Cow::Borrowed(name.as_ref())
            };

            ordinal =
                Self::assign_ordinal(ctx, ordinal_group_name.as_ref(), params.key, name_collision);
        }

        (name, ordinal)
    }

    #[inline]
    fn assign_ordinal(
        ctx: &RenderContext,
        name: &str,
        key: &str,
        name_collision: bool,
    ) -> Option<usize> {
        let mut ordinals = ctx.ordinals.borrow_mut();
        ctx.clear_target_cache();
        if !ordinals.contains_key(name) {
            ordinals.insert(
                name.to_string(),
                crate::models::OrdinalState {
                    next_ordinal: 1,
                    members: HashMap::new(),
                },
            );
        }

        if let Some(state) = ordinals.get_mut(name) {
            if name_collision {
                if !state.members.contains_key(key) {
                    let o = state.next_ordinal;
                    state.next_ordinal += 1;
                    state.members.insert(key.to_string(), o);
                }
                if let Some(&ord) = state.members.get(key) {
                    return Some(ord);
                }
            } else {
                state.members.clear();
                state.members.insert(key.to_string(), 1);
                state.next_ordinal = 2;
            }
        }
        None
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
                &EntityRefParams {
                    adjectives: None,
                    ..*params
                },
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
            .map_or_else(|| Self::get_entity(ctx, owner_key), Ok)?;

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
                effective_gender = Self::determine_group_singular_gender(members);
            } else if effective_gender == crate::models::Gender::Plural {
                effective_gender = crate::models::Gender::Neutral;
            }
        }

        let is_viewer = entity.contains_viewer(effective_viewer) && !extract_member;
        let is_active_subject =
            Self::check_is_active_subject(ctx, entity, params.key, pre_resolved);

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
                update_memory(&ctx.last_mentioned, params.key);
                track_recent_entity(ctx, params.key, entity, params.adjectives, None);
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
            && !Self::is_pronoun_ambiguous(ctx, key, effective_gender, pronoun_ctx, active_flags)
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
        let (name, ordinal) = Self::resolve_display_name(
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

        update_memory(&ctx.last_mentioned, params.key);
        track_recent_entity(
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
            adjectives: params.adjectives, // Pass the adjectives to the print functions
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
        if let Some(adj) = active_params.adjectives {
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
                active_subject_entity.is_some_and(|active| is_same_entity(active, member));

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
            let mut final_name = if (params.flags.is_capitalized()
                || params.flags.article_capitalized())
                && first_visible_item
            {
                std::borrow::Cow::Owned(crate::grammar::capitalize_first(reflex))
            } else {
                std::borrow::Cow::Borrowed(reflex)
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
            config.flags.contains(GroupMemberFlags::AFTER_POSSESSIVE),
        );
        article_flags.set(
            crate::grammar::ArticleFlags::IS_CAPITALIZED,
            params.flags.article_capitalized() && first_visible_item,
        );

        let mut adj_prefix = String::new();
        if first_visible_item && let Some(adj) = params.adjectives {
            if params.flags.contains(TagFlags::ALL_CAPS) {
                crate::typography::apply_all_caps(adj, &mut adj_prefix);
            } else {
                adj_prefix.push_str(adj);
            }
        }

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
            std::borrow::Cow::Owned(format!("{}{adj_prefix}{name}", resolved_art.as_ref()))
        } else if !adj_prefix.is_empty() {
            let cap_adj = if params.flags.article_capitalized() && first_visible_item {
                crate::grammar::capitalize_first(&adj_prefix)
            } else {
                adj_prefix
            };
            std::borrow::Cow::Owned(format!("{cap_adj}{name}"))
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
            // Prevent false positives when a sub-property is located at memory offset 0
            // of its parent struct, which causes their data pointers to be identical.
            if is_sub_property_path(active_key, key) || is_sub_property_path(key, active_key) {
                return false;
            }
            return is_same_entity(entity, active_entity);
        }
        false
    }

    #[inline]
    fn is_pronoun_ambiguous(
        ctx: &RenderContext,
        key: &str,
        effective_gender: crate::models::Gender,
        pronoun_ctx: PronounContextFlags,
        flags: TagFlags,
    ) -> bool {
        let is_plural = pronoun_ctx.contains(PronounContextFlags::IS_PLURAL);
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
            track_recent_entity(ctx, key, entity, None, None);
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
}

#[inline]
fn update_memory(memory: &std::cell::RefCell<Option<String>>, key: &str) {
    if memory.borrow().as_deref() != Some(key) {
        *memory.borrow_mut() = Some(key.to_string());
    }
}

#[inline]
fn track_recent_entity(
    ctx: &RenderContext<'_>,
    key: &str,
    entity: &dyn TemplateEntity,
    adjectives: Option<&str>,
    resolved_name: Option<&str>,
) {
    let mut recents = ctx.recent_entities.borrow_mut();
    ctx.clear_target_cache();

    let mut new_adjs = Vec::new();
    if let Some(adj_str) = adjectives {
        let stripped = crate::typography::strip_all_protocol_tags(adj_str);
        for word in stripped.split_whitespace() {
            let clean = word.trim().to_lowercase();
            if !clean.is_empty() {
                new_adjs.push(clean);
            }
        }
    }

    // Move to the back to represent the most recently used (LRU)
    if let Some(pos) = recents.iter().position(|r| r.key == key) {
        let mut item = recents.remove(pos);

        // Refresh grammatical properties in case the underlying entity mutated (e.g., GroupEntities)
        item.gender = entity.gender();
        item.flags.set(
            crate::models::RecentEntityFlags::IS_PLURAL,
            entity.is_plural(),
        );
        item.flags.set(
            crate::models::RecentEntityFlags::IS_VIEWER_NORMAL,
            entity.contains_viewer(ctx.viewer_id),
        );
        item.flags.set(
            crate::models::RecentEntityFlags::IS_VIEWER_FORCED,
            entity.contains_viewer(crate::models::NULL_VIEWER),
        );

        for adj in new_adjs {
            if !item.adjectives.contains(&adj) {
                item.adjectives.push(adj);
            }
        }
        if let Some(rn) = resolved_name {
            item.resolved_name = Some(rn.to_string());
        }
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
            adjectives: new_adjs,
            resolved_name: resolved_name.map(ToString::to_string),
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
fn is_sub_property_path(parent: &str, child: &str) -> bool {
    child.starts_with(parent)
        && child.len() > parent.len()
        && child.as_bytes().get(parent.len()) == Some(&b'.')
}

#[inline]
fn effective_viewer_id<'a>(ctx: &RenderContext<'a>, force_3rd_person: bool) -> &'a str {
    if force_3rd_person || ctx.stance == crate::models::ActorStance::ThirdPerson {
        NULL_VIEWER
    } else {
        ctx.viewer_id
    }
}
