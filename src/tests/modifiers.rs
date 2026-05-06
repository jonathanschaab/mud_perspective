use super::common::MockEntity;
use crate::cache::TemplateCache;
use crate::engine::PerspectiveEngine;
use crate::models::{Gender, RenderContext};

#[test]
fn test_force_director_stance() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let template_forced = cache
        .get_or_compile(
            "{a:+source:subj} [+source:attack] {the:target:obj} with {the:+source:poss} sword.",
        )
        .unwrap();

    // The player is the viewer, so normally this would render "You attack the goblin with your sword."
    // Because of the `+` prefix on the keys, it forces 3rd person logic even for the viewer!
    let out_forced =
        render_msg!("char_1", &template_forced, "source" => &player, "target" => &goblin).unwrap();
    assert_eq!(out_forced, "Aldran attacks the goblin with his sword.");

    // Can even force an article onto a forced-3rd-person proper noun (e.g. {+the:source})
    let template_double_force = cache.get_or_compile("{+The:source:subj} is here.").unwrap();
    let out_double_force =
        render_msg!("char_1", &template_double_force, "source" => &player).unwrap();
    assert_eq!(out_double_force, "The Aldran is here.");
}

#[test]
fn test_singular_overrides() {
    let orcs = MockEntity {
        id: "mob_1".to_string(),
        name: "orcs".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let goblin = MockEntity {
        id: "mob_2".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // 1. Force Singular Verb on a Plural Entity
    let t1 = cache
        .get_or_compile("{One of the:-orcs:Subj} [-orcs:bellow], and {-orcs:subj} [-orcs:charge]!")
        .expect("Failed to compile template");
    let ctx1 = RenderContext::new("viewer").with_entity("orcs", &orcs);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template"),
        "One of the orcs bellows, and it charges!"
    );

    // 2. Singular Override Pronoun Ambiguity Fallback
    // The `-` prefix on the pronoun forces `is_plural = false` and `effective_gender = Neutral`.
    // The goblin is Neutral. This causes an ambiguity!
    // The engine should fallback gracefully to "One of the orcs" instead of "Some orcs".
    let t2 = cache
        .get_or_compile(
            "{One of the:-orcs:Subj} and {a:goblin:subj} arrive. {-orcs:Subj} [-orcs:bellow].",
        )
        .expect("Failed to compile template");
    let ctx2 = RenderContext::new("viewer")
        .with_entity("orcs", &orcs)
        .with_entity("goblin", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx2).expect("Failed to render template"),
        "One of the orcs and a goblin arrive. One of the orcs bellows."
    );
}

#[test]
fn test_singular_override_tenses_and_stances() {
    let orcs = MockEntity {
        id: "mob_orcs".to_string(),
        name: "orcs".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{One of the:-orcs:Subj} [-orcs:charge].")
        .expect("Failed to compile template");

    // 1. Director Stance (Present, Past, Future)
    let ctx_director_pres = RenderContext::new("viewer").with_entity("orcs", &orcs);
    let ctx_director_past = ctx_director_pres
        .clone()
        .with_tense(crate::models::Tense::Past);
    let ctx_director_fut = ctx_director_pres
        .clone()
        .with_tense(crate::models::Tense::Future);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_director_pres)
            .expect("Failed to render template"),
        "One of the orcs charges."
    );

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_director_past)
            .expect("Failed to render template"),
        "One of the orcs charged."
    );

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_director_fut).expect("Failed to render template"),
        "One of the orcs will charge."
    );

    // 2. Actor Stance (First Person, Singular Override shifts "We" -> "I")
    let ctx_actor_1st = RenderContext::new("mob_orcs")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("orcs", &orcs);
    let ctx_actor_1st_past = ctx_actor_1st.clone().with_tense(crate::models::Tense::Past);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_actor_1st).unwrap(),
        "I charge."
    );

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_actor_1st_past).unwrap(),
        "I charged."
    );

    // Prove that without the override, it behaves as a standard plural first-person group ("We")
    let t_no_override = cache
        .get_or_compile("{a:orcs:Subj} [orcs:charge].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_no_override, &ctx_actor_1st).unwrap(),
        "We charge."
    );
}

#[test]
fn test_singular_override_ambiguity_and_possessives() {
    let orcs = MockEntity {
        id: "mob_orcs".to_string(),
        name: "orcs".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };
    let goblin = MockEntity {
        id: "mob_goblin".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("orcs", &orcs)
        .with_entity("goblin", &goblin);

    // Ambiguity Fallback! Singular override makes orcs "Neutral" gender. Goblin is also "Neutral".
    // The pronoun {-orcs:Subj} will be ambiguous with the goblin.
    // It should gracefully fall back to "One of the orcs".
    // However, because `[-orcs:draw]` makes the orcs the active subject, `{-orcs:poss}` naturally collapses to "its"!
    let t = cache
        .get_or_compile(
            "{A:goblin:Subj} snarls. {One of the:-orcs:subj} [-orcs:draw] {-orcs:poss} blade!",
        )
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).expect("Failed to render template"),
        "A goblin snarls. One of the orcs draws its blade!"
    );

    ctx.clear_anaphora();

    // If the orc WASN'T the active subject, the ambiguity would trigger the fallback.
    // But the builder can stack the `!` and `-` modifiers to force the pronoun anyway!
    let t2 = cache
        .get_or_compile("{A:goblin:Subj} snarls at {-orcs:obj} and steals {!-orcs:poss} blade!")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template"),
        "A goblin snarls at one of the orcs and steals its blade!"
    );
}

#[test]
fn test_singular_override_forced_conjugation_and_lookahead() {
    let orcs = MockEntity {
        id: "mob_orcs".to_string(),
        name: "orcs".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("orcs", &orcs)
        .with_lookahead(true);
    let ctx_future = ctx.clone().with_tense(crate::models::Tense::Future);

    // We use forced conjugation for a complex verb like "be" and "have".
    // The `-` prefix should correctly route the forced conjugation to the 3rd person singular slot.
    let t = cache.get_or_compile("{One of the:-orcs:Subj} [-orcs:be|am|are|is] here. {a:-orcs:Subj} [-orcs:have|have|have|has] arrived!").unwrap();

    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "One of the orcs is here. It has arrived!"
    );

    // Ensure that shifting to the future tense safely bypasses all overrides and relies on "will"
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx_future).unwrap(),
        "One of the orcs will be here. It will have arrived!"
    );
}

#[test]
fn test_modifier_stacking_order_independence() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_1").with_entity("source", &player);

    // The '+' forces 3rd person (ignoring the viewer ID).
    // The '!' suppresses the anaphora fallback ambiguity check.
    // The '-' forces singular.
    // We test three different stacking orders to prove the engine evaluates them identically!
    let t1 = cache
        .get_or_compile("{a:+!-source:Subj} [+source:nod].")
        .expect("Failed to compile template");
    let t2 = cache
        .get_or_compile("{a:-!+source:Subj} [+source:nod].")
        .expect("Failed to compile template");
    let t3 = cache
        .get_or_compile("{a:!+-source:Subj} [+source:nod].")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "He nods."
    );
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template"),
        "He nods."
    );
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx).expect("Failed to render template"),
        "He nods."
    );
}

#[test]
fn test_singular_override_reflexive_pronouns() {
    let orcs = MockEntity {
        id: "mob_1".to_string(),
        name: "orcs".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer").with_entity("orcs", &orcs);

    // Introduce the orcs so they are in anaphora memory
    let t_intro = cache
        .get_or_compile("{*The:orcs:subj} are here.")
        .expect("Failed to compile template");
    let _ = PerspectiveEngine::render(&t_intro, &ctx).expect("Failed to render template");

    // Without override (Standard Plural):
    let t1 = cache
        .get_or_compile("{a:orcs:Subj} [orcs:hurt] {a:orcs:reflex}.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "They hurt themselves."
    );

    // With override: Shifts from Plural -> Neutral (It/itself)
    let t2 = cache
        .get_or_compile("{-a:orcs:Subj} [-orcs:hurt] {-a:orcs:reflex}.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template"),
        "It hurts itself."
    );
}

#[test]
fn test_plural_proper_noun_with_singular_override() {
    let avengers = MockEntity {
        id: "char_1".to_string(),
        name: "the Avengers".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer").with_entity("avengers", &avengers);

    // Normally behaves as a plural entity
    let t1 = cache
        .get_or_compile(
            "{*A:avengers:subj} [avengers:assemble] and [avengers:defend] {a:avengers:reflex}.",
        )
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "The Avengers assemble and defend themselves."
    );

    // The singular override cleanly intercepts the verb and pronoun logic, even for proper nouns
    let t2 = cache
        .get_or_compile(
            "{*A:-avengers:subj} [-avengers:assemble] and [-avengers:defend] {a:-avengers:reflex}.",
        )
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template"),
        "The Avengers assembles and defends itself."
    );
}

#[test]
fn test_forced_stance_overrides_first_person() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{*a:+source:subj} [+source:draw] {a:+source:poss} sword.")
        .expect("Failed to compile template");

    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player);

    // The '+' prefix should safely override the First Person 'I/my' back to 'Aldran/his'
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first).expect("Failed to render template"),
        "Aldran draws his sword."
    );
}
