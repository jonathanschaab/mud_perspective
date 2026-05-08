use std::borrow::Cow;
use std::collections::HashMap;

use crate::engine::EntityRefParams;
use crate::models::{OrdinalState, RecentEntity, RenderContext, TemplateEntity};
use crate::parser::TAG_PROPERTY_SEP;

#[inline]
pub(crate) fn resolve_display_name<'a>(
    ctx: &'a RenderContext,
    entity: &'a dyn TemplateEntity,
    params: &EntityRefParams<'_>,
    effective_viewer: &str,
    future_keys: &[&str],
    pre_resolved: &HashMap<&str, &'a dyn TemplateEntity>,
) -> (Cow<'a, str>, Option<usize>) {
    let mut name = entity.display_name_for(effective_viewer);
    let mut name_collision = false;

    if !params.flags.no_smart() {
        let mut short_collisions = 0;
        let mut unresolved_colliders: Vec<&'a dyn TemplateEntity> = Vec::new();
        let recent_borrow = ctx.recent_entities.borrow();

        // Evaluate collisions dynamically to avoid allocations while ensuring we accurately
        // catch entities that were just introduced in this template, maintaining strict
        // left-to-right chronological accuracy.
        let mut check_collision = |other_key: &str, other_entity: &'a dyn TemplateEntity| {
            if other_key != params.key && other_entity.display_name_for(effective_viewer) == name {
                short_collisions += 1;

                // Determine if this other entity will vacate the short name by using its own long name
                if !check_will_vacate(
                    ctx,
                    effective_viewer,
                    &recent_borrow,
                    future_keys,
                    pre_resolved,
                    other_key,
                    other_entity,
                    name.as_ref(),
                    name.as_ref(),
                ) {
                    unresolved_colliders.push(other_entity);
                }
            }
        };

        for r in recent_borrow.iter() {
            if let Ok(other_entity) = crate::evaluator::get_entity(ctx, &r.key) {
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
            let long_collisions = count_long_name_collisions(
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
            && let Some((adj_name, still_collides)) = try_adjective_disambiguation(
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
            Cow::Owned(format!("{ns}::{name}"))
        } else {
            Cow::Borrowed(name.as_ref())
        };

        ordinal = assign_ordinal(ctx, ordinal_group_name.as_ref(), params.key, name_collision);
    }

    (name, ordinal)
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn check_will_vacate(
    ctx: &RenderContext,
    effective_viewer: &str,
    recent_borrow: &[RecentEntity],
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
            if let Ok(eval_entity) = crate::evaluator::get_entity(ctx, &r.key) {
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
) -> Option<(Cow<'a, str>, bool)> {
    let entity_adjs = entity.adjectives().filter(|adjs| !adjs.is_empty())?;

    // Prevent bitshift overflow by capping the limit to the bit-width of our counter
    let safe_limit = limit.min((u64::BITS - 1) as usize);
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
    for i in 1_u64..(1_u64 << searchable_adjs.len()) {
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
            Cow::Owned(format!("{prefix} {base_name}")),
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
    recent_borrow: &[RecentEntity],
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
        if let Ok(other_entity) = crate::evaluator::get_entity(ctx, &r.key) {
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
            OrdinalState {
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
