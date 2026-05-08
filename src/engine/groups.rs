use std::borrow::Cow;
use std::collections::HashMap;

use crate::engine::{EntityRefParams, PerspectiveEngine};
use crate::grammar::{resolve_article, resolve_pronoun};
use crate::models::{RenderContext, TemplateEntity, is_same_entity};

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct GroupMemberFlags: u8 {
        const AFTER_POSSESSIVE       = 1 << 0;
        const FIRST_VISIBLE_ITEM     = 1 << 1;
        const DISTRIBUTE_POSSESSIVES = 1 << 2;
        const IS_REFLEXIVE           = 1 << 3;
    }
}

pub(crate) struct GroupMemberFormatConfig<'a> {
    pub(crate) flags: GroupMemberFlags,
    pub(crate) article_to_use: Option<&'a str>,
}

#[inline]
pub(crate) fn determine_group_singular_gender(
    members: &[&dyn TemplateEntity],
) -> crate::models::Gender {
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
pub(crate) fn render_group_entity<'a>(
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
                .map_or_else(|| crate::evaluator::get_entity(ctx, active_key).ok(), Some)
        });

    let viewer_is_active_subject =
        active_subject_entity.is_some_and(|active| active.contains_viewer(effective_viewer));
    let is_objective = params.p_type == Some("obj");

    let mut ends_with_possessive_pronoun = false;
    let mut decomposed_we = false;
    let mut formatted_names = Vec::with_capacity(total_visible + 1);

    if let Some(viewer) = viewer_entity
        && let Some(prefix) = format_group_viewer_prefix(
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

        formatted_names.push(format_group_member(
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
        let suffix = format_group_viewer_suffix(
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
    if params.flags.is_possessive() && !ends_with_possessive_pronoun && !distribute_possessives {
        final_str.push_str(PerspectiveEngine::get_possessive_suffix(
            &final_str,
            entity.is_plural(),
        ));
    }

    crate::grammar::push_capitalized_if(raw_output, &final_str, cap_whole);
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
) -> Option<Cow<'static, str>> {
    if ctx.stance == crate::models::ActorStance::SecondPerson {
        if params.flags.is_possessive() {
            if visible_is_empty {
                *ends_with_possessive_pronoun = true;
            }
            return Some(Cow::Borrowed("your"));
        } else if is_objective && viewer_is_active_subject {
            let reflex = resolve_pronoun(
                crate::models::Gender::Neutral,
                "reflex",
                true,
                viewer.is_plural(),
                ctx.stance,
            )
            .unwrap_or("yourself");
            return Some(Cow::Borrowed(reflex));
        }
        return Some(Cow::Borrowed("you"));
    } else if ctx.stance == crate::models::ActorStance::FirstPerson && viewer.is_plural() {
        if visible_is_empty {
            if params.flags.is_possessive() {
                *ends_with_possessive_pronoun = true;
                return Some(Cow::Borrowed("our"));
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
                    return Some(Cow::Borrowed(reflex));
                }
                return Some(Cow::Borrowed("us"));
            }
            return Some(Cow::Borrowed("we"));
        }

        *decomposed_we = true;
        if params.flags.is_possessive() {
            return Some(Cow::Borrowed("your"));
        } else if is_objective && viewer_is_active_subject {
            let reflex = resolve_pronoun(
                crate::models::Gender::Neutral,
                "reflex",
                true,
                false,
                crate::models::ActorStance::SecondPerson,
            )
            .unwrap_or("yourself");
            return Some(Cow::Borrowed(reflex));
        }
        return Some(Cow::Borrowed("you"));
    }
    None
}

#[inline]
fn format_group_viewer_suffix(
    is_possessive: bool,
    viewer_is_active_subject: bool,
    is_objective: bool,
    ends_with_possessive_pronoun: &mut bool,
) -> Cow<'static, str> {
    if is_possessive {
        *ends_with_possessive_pronoun = true;
        Cow::Borrowed("my")
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
            Cow::Borrowed(reflex)
        } else {
            Cow::Borrowed("me")
        }
    } else {
        Cow::Borrowed("I")
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn format_group_member<'a>(
    ctx: &RenderContext,
    entity: &dyn TemplateEntity,
    member: &'a dyn TemplateEntity,
    name: Cow<'a, str>,
    effective_viewer: &str,
    params: &EntityRefParams<'_>,
    config: &GroupMemberFormatConfig<'_>,
) -> Cow<'a, str> {
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
            Cow::Owned(crate::grammar::capitalize_first(reflex))
        } else {
            Cow::Borrowed(reflex)
        };

        if config
            .flags
            .contains(GroupMemberFlags::DISTRIBUTE_POSSESSIVES)
        {
            let suffix = PerspectiveEngine::get_possessive_suffix(&final_name, member.is_plural());
            let mut owned = final_name.into_owned();
            owned.push_str(suffix);
            final_name = Cow::Owned(owned);
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
    if first_visible_item
        && let Some(adj) = params.adjectives
        && !adj.is_empty()
    {
        adj_prefix.push_str(adj);
        adj_prefix.push(' ');
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
        Cow::Owned(format!("{}{adj_prefix}{name}", resolved_art.as_ref()))
    } else if !adj_prefix.is_empty() {
        let cap_adj = if params.flags.article_capitalized() && first_visible_item {
            crate::grammar::capitalize_first(&adj_prefix)
        } else {
            adj_prefix
        };
        Cow::Owned(format!("{cap_adj}{name}"))
    } else {
        name
    };

    if config
        .flags
        .contains(GroupMemberFlags::DISTRIBUTE_POSSESSIVES)
    {
        let suffix = PerspectiveEngine::get_possessive_suffix(&final_name, member.is_plural());
        let mut owned = final_name.into_owned();
        owned.push_str(suffix);
        final_name = Cow::Owned(owned);
    }

    final_name
}
