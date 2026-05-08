use std::collections::HashMap;

use crate::engine::PronounContextFlags;
use crate::models::{
    NULL_VIEWER, RecentEntity, RecentEntityFlags, RenderContext, TemplateEntity, is_same_entity,
};
use crate::parser::TagFlags;

#[inline]
pub(crate) fn update_memory(memory: &std::cell::RefCell<Option<String>>, key: &str) {
    if memory.borrow().as_deref() != Some(key) {
        *memory.borrow_mut() = Some(key.to_string());
    }
}

#[inline]
pub(crate) fn track_recent_entity(
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
        item.flags
            .set(RecentEntityFlags::IS_PLURAL, entity.is_plural());
        item.flags.set(
            RecentEntityFlags::IS_VIEWER_NORMAL,
            entity.contains_viewer(ctx.viewer_id),
        );
        item.flags.set(
            RecentEntityFlags::IS_VIEWER_FORCED,
            entity.contains_viewer(NULL_VIEWER),
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
        let mut flags = RecentEntityFlags::empty();
        flags.set(RecentEntityFlags::IS_PLURAL, entity.is_plural());
        flags.set(
            RecentEntityFlags::IS_VIEWER_NORMAL,
            entity.contains_viewer(ctx.viewer_id),
        );
        flags.set(
            RecentEntityFlags::IS_VIEWER_FORCED,
            entity.contains_viewer(NULL_VIEWER),
        );

        recents.push(RecentEntity {
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
pub(crate) fn check_is_active_subject(
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
            .map_or_else(|| crate::evaluator::get_entity(ctx, active_key), Ok)
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
pub(crate) fn is_pronoun_ambiguous(
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
                other.flags.contains(RecentEntityFlags::IS_VIEWER_FORCED)
            } else {
                other.flags.contains(RecentEntityFlags::IS_VIEWER_NORMAL)
            };

            if !other_is_viewer
                && effective_gender == other.gender
                && is_plural == other.flags.contains(RecentEntityFlags::IS_PLURAL)
            {
                return true;
            }
        }
    }
    false
}

#[inline]
pub(crate) fn is_sub_property_path(parent: &str, child: &str) -> bool {
    child.starts_with(parent)
        && child.len() > parent.len()
        && child.as_bytes().get(parent.len()) == Some(&b'.')
}
