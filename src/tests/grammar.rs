use super::common::MockEntity;
use crate::cache::TemplateCache;
use crate::engine::PerspectiveEngine;
use crate::models::{Gender, GroupEntity, RenderContext};
use serial_test::serial;

#[test]
fn test_irregular_verb_conjugations() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // 1. "fly" -> "flies"
    let template_fly = cache
        .get_or_compile("{*A:source:subj} [source:fly].")
        .expect("Failed to compile template");
    assert_eq!(
        render_msg!("char_1", &template_fly, "source" => &player)
            .expect("Failed to render template"),
        "You fly."
    );
    assert_eq!(
        render_msg!("char_2", &template_fly, "source" => &player)
            .expect("Failed to render template"),
        "Aldran flies."
    );

    // 2. "run" -> "ran" (Dynamic past tense shift)
    let template_run = cache
        .get_or_compile("{*A:source:subj} [source:run].")
        .expect("Failed to compile template");
    let ctx_actor_past = RenderContext::new("char_1")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &player);
    assert_eq!(
        PerspectiveEngine::render(&template_run, &ctx_actor_past)
            .expect("Failed to render template"),
        "You ran."
    );
    let ctx_director_past = RenderContext::new("char_2")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &player);
    assert_eq!(
        PerspectiveEngine::render(&template_run, &ctx_director_past)
            .expect("Failed to render template"),
        "Aldran ran."
    );

    // 3. "catch" -> "catches"
    let template_catch = cache
        .get_or_compile("{*A:source:subj} [source:catch] it.")
        .expect("Failed to compile template");
    assert_eq!(
        render_msg!("char_2", &template_catch, "source" => &player)
            .expect("Failed to render template"),
        "Aldran catches it."
    );

    // 4. Fallback rule: consonant + y -> ies ("try" -> "tries")
    let template_try = cache
        .get_or_compile("{*A:source:subj} [source:try].")
        .expect("Failed to compile template");
    assert_eq!(
        render_msg!("char_2", &template_try, "source" => &player)
            .expect("Failed to render template"),
        "Aldran tries."
    );

    // 5. Fallback rule: ends with x -> es ("box" -> "boxes")
    let template_box = cache
        .get_or_compile("{*A:source:subj} [source:box].")
        .expect("Failed to compile template");
    assert_eq!(
        render_msg!("char_2", &template_box, "source" => &player)
            .expect("Failed to render template"),
        "Aldran boxes."
    );

    // 6. Modal verbs natively injected via build.rs
    // This ensures colliding verbs (e.g. "cans" or "wills") don't overwrite modal behaviors.
    let modals = [
        "can", "could", "will", "would", "shall", "should", "may", "might", "must", "ought",
    ];
    for modal in modals {
        let template_str = format!("{{*A:source:subj}} [source:{modal}].");
        let template_modal = cache
            .get_or_compile(&template_str)
            .expect("Failed to compile template");
        assert_eq!(
            render_msg!("char_1", &template_modal, "source" => &player)
                .expect("Failed to render template"),
            format!("You {modal}.")
        );
        assert_eq!(
            render_msg!("char_2", &template_modal, "source" => &player)
                .expect("Failed to render template"),
            format!("Aldran {modal}.")
        );
    }
}

#[test]
fn test_first_person_and_be_verbs() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player);

    let ctx_second = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::SecondPerson) // Default
        .with_entity("source", &player);

    let ctx_third = RenderContext::new("char_2").with_entity("source", &player);

    // 1. "be" (Handled dynamically by stance and perspective overrides)
    let template_be = cache
        .get_or_compile("{*A:source:subj} [source:be] ready.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&template_be, &ctx_first).expect("Failed to render template"),
        "I am ready."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_be, &ctx_second).expect("Failed to render template"),
        "You are ready."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_be, &ctx_third).expect("Failed to render template"),
        "Aldran is ready."
    );

    // 2. "was" (Handled dynamically in past tense by stance and perspective overrides)
    let ctx_first_past = ctx_first.with_tense(crate::models::Tense::Past);
    let ctx_second_past = ctx_second.with_tense(crate::models::Tense::Past);
    let ctx_third_past = ctx_third.with_tense(crate::models::Tense::Past);

    assert_eq!(
        PerspectiveEngine::render(&template_be, &ctx_first_past)
            .expect("Failed to render template"),
        "I was ready."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_be, &ctx_second_past)
            .expect("Failed to render template"),
        "You were ready."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_be, &ctx_third_past)
            .expect("Failed to render template"),
        "Aldran was ready."
    );

    // 3. Ensure first person leaves irregular and algorithmically modified verbs uninflected
    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player);

    let template_fly = cache
        .get_or_compile("{*A:source:subj} [source:fly].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&template_fly, &ctx_first).expect("Failed to render template"),
        "I fly."
    );

    let template_catch = cache
        .get_or_compile("{*A:source:subj} [source:catch] it.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&template_catch, &ctx_first).expect("Failed to render template"),
        "I catch it."
    );

    let template_try = cache
        .get_or_compile("{*A:source:subj} [source:try].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&template_try, &ctx_first).expect("Failed to render template"),
        "I try."
    );
}

#[test]
fn test_past_tense_have_across_stances() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player);

    let ctx_second = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::SecondPerson) // Default
        .with_entity("source", &player);

    let ctx_third = RenderContext::new("char_2").with_entity("source", &player);

    let template_have = cache
        .get_or_compile("{*A:source:subj} [source:have] a sword.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&template_have, &ctx_first).expect("Failed to render template"),
        "I have a sword."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_have, &ctx_second).expect("Failed to render template"),
        "You have a sword."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_have, &ctx_third).expect("Failed to render template"),
        "Aldran has a sword."
    );

    let ctx_first_past = ctx_first.with_tense(crate::models::Tense::Past);
    let ctx_second_past = ctx_second.with_tense(crate::models::Tense::Past);
    let ctx_third_past = ctx_third.with_tense(crate::models::Tense::Past);

    assert_eq!(
        PerspectiveEngine::render(&template_have, &ctx_first_past)
            .expect("Failed to render template"),
        "I had a sword."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_have, &ctx_second_past)
            .expect("Failed to render template"),
        "You had a sword."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_have, &ctx_third_past)
            .expect("Failed to render template"),
        "Aldran had a sword."
    );
}

#[test]
#[serial]
fn test_custom_runtime_verbs() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // Add a completely new custom verb
    crate::grammar::add_irregular_verb("yeet", "yeetses", "yeeted")
        .expect("Failed to add custom verb");

    let template = cache
        .get_or_compile("{*A:source:subj} [source:yeet].")
        .expect("Failed to compile template");
    assert_eq!(
        render_msg!("char_2", &template, "source" => &player).expect("Failed to render template"),
        "Aldran yeetses."
    );

    // Test removing the custom verb
    assert!(crate::grammar::remove_irregular_verb("yeet"));
    assert!(!crate::grammar::remove_irregular_verb("yeet")); // Should return false the second time

    // After removal, it should conjugate as a regular verb "yeets"
    assert_eq!(
        render_msg!("char_2", &template, "source" => &player).expect("Failed to render template"),
        "Aldran yeets."
    );

    // Attempting to add an existing PHF verb should fail
    assert!(crate::grammar::add_irregular_verb("arise", "arises not", "arose not").is_err());

    // Forcing an existing PHF verb should succeed and override
    crate::grammar::force_add_irregular_verb("arise", "arizez", "arouze");

    let template_arise = cache
        .get_or_compile("{*A:source:subj} [source:arise].")
        .expect("Failed to compile template");
    assert_eq!(
        render_msg!("char_2", &template_arise, "source" => &player)
            .expect("Failed to render template"),
        "Aldran arizez."
    );

    // Test removing a forced verb
    assert!(crate::grammar::remove_irregular_verb("arise"));

    // It should revert back to the static irregular map
    assert_eq!(
        render_msg!("char_2", &template_arise, "source" => &player)
            .expect("Failed to render template"),
        "Aldran arises."
    );

    // Clean up the global state safely now that we are running serially
    crate::grammar::clear_irregular_verbs();
}

#[test]
#[serial]
fn test_macro_register_custom_verbs() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // Register multiple custom verbs at once
    crate::register_custom_verbs! {
        "bloop" => ("bloopses", "bloopeded"),
        "blarg" => ("blargs", "blarged"),
    };

    let ctx_pres = RenderContext::new("char_2").with_entity("source", &player);
    let ctx_past = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Past);

    let t1 = cache
        .get_or_compile("{*A:source:subj} [source:bloop].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx_pres).expect("Failed to render template"),
        "Aldran bloopses."
    );
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx_past).expect("Failed to render template"),
        "Aldran bloopeded."
    );

    let t2 = cache
        .get_or_compile("{*A:source:subj} [source:blarg].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx_pres).expect("Failed to render template"),
        "Aldran blargs."
    );
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx_past).expect("Failed to render template"),
        "Aldran blarged."
    );

    // Clean up the global state safely now that we are running serially
    crate::grammar::clear_irregular_verbs();
}

#[test]
fn test_forced_conjugation_in_template() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // 1. Force a pirate conjugation
    let template_pirate = cache
        .get_or_compile("{*A:source:subj} [source:be|be] looking tired.")
        .expect("Failed to compile template");
    assert_eq!(
        render_msg!("char_1", &template_pirate, "source" => &player)
            .expect("Failed to render template"),
        "You be looking tired."
    );
    assert_eq!(
        render_msg!("char_2", &template_pirate, "source" => &player)
            .expect("Failed to render template"),
        "Aldran be looking tired."
    );

    // 2. Force capitalization correctly
    let template_cap = cache
        .get_or_compile("[source:Look|gaze] at me!")
        .expect("Failed to compile template");
    assert_eq!(
        render_msg!("char_2", &template_cap, "source" => &player)
            .expect("Failed to render template"),
        "Gaze at me!"
    );
}

#[test]
fn test_phrasal_verbs() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let ctx_second = RenderContext::new("char_1").with_entity("source", &player);
    let ctx_third = RenderContext::new("char_2").with_entity("source", &player);

    let template = cache
        .get_or_compile("{*A:source:subj} [source:pick up] the sword.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_second).expect("Failed to render template"),
        "You pick up the sword."
    );
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_third).expect("Failed to render template"),
        "Aldran picks up the sword."
    );

    let template_cap = cache
        .get_or_compile("[source:Give up]!")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&template_cap, &ctx_third).expect("Failed to render template"),
        "Gives up!"
    );
}

#[test]
#[serial]
fn test_complex_phrasal_and_hyphenated_verbs() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_2").with_entity("source", &player);

    // 1. Phrasal verb naturally split ("look around" -> "looks around")
    let t1 = cache
        .get_or_compile("{*A:source:subj} [source:look around].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "Aldran looks around."
    );

    // 2. Phrasal verb explicitly in PHF ("pinch run" -> "pinch runs")
    let t2 = cache
        .get_or_compile("{*A:source:subj} [source:pinch run].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template"),
        "Aldran pinch runs."
    );

    // 3. Hyphenated verb treated as single word ("cross-pollinate" -> "cross-pollinates")
    let t3 = cache
        .get_or_compile("{*A:source:subj} [source:cross-pollinate].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx).expect("Failed to render template"),
        "Aldran cross-pollinates."
    );

    // 4. Runtime dictionary multi-word override ("make do" -> "makes do")
    crate::grammar::add_irregular_verb("make do", "makes do", "made do")
        .expect("Failed to add custom verb");
    let t4 = cache
        .get_or_compile("{*A:source:subj} [source:make do].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t4, &ctx).expect("Failed to render template"),
        "Aldran makes do."
    );

    // Clean up the global state safely now that we are running serially
    crate::grammar::clear_irregular_verbs();
}

#[test]
fn test_dynamic_past_tense() {
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
    let template = cache
        .get_or_compile("{*A:source:subj} [source:hit] {*the:target:obj} and [source:laugh].")
        .expect("Failed to compile template");

    let ctx_present = RenderContext::new("char_1")
        .with_entity("source", &player)
        .with_entity("target", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_present).expect("Failed to render template"),
        "You hit the goblin and laugh."
    );

    let ctx_past = RenderContext::new("char_1")
        .with_entity("source", &player)
        .with_entity("target", &goblin)
        .with_tense(crate::models::Tense::Past);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_past).expect("Failed to render template"),
        "You hit the goblin and laughed."
    );

    let ctx_past_director = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_entity("target", &goblin)
        .with_tense(crate::models::Tense::Past);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_past_director)
            .expect("Failed to render template"),
        "Aldran hit the goblin and laughed."
    );
}

#[test]
fn test_dynamic_past_tense_with_groups() {
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
    let party = GroupEntity {
        members: vec![&player, &goblin],
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{*the:source:subj} [source:be] here, and [source:try] to escape.")
        .expect("Failed to compile template");

    let ctx_solo_actor = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &player);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_solo_actor).expect("Failed to render template"),
        "I was here, and tried to escape."
    );

    let ctx_solo_director = RenderContext::new("char_2")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_solo_director)
            .expect("Failed to render template"),
        "The goblin was here, and tried to escape."
    );

    let ctx_party_actor = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &party);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_party_actor).expect("Failed to render template"),
        "The goblin and I were here, and tried to escape."
    );
}

#[test]
fn test_dynamic_past_tense_regular_fallbacks() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let test_cases = vec![
        ("chase", "chased"),
        ("dry", "dried"),
        ("play", "played"),
        ("walk", "walked"),
        ("box", "boxed"),
    ];

    for (verb, expected_past) in test_cases {
        let template_str = format!("{{*A:source:subj}} [source:{verb}].");
        let template = cache
            .get_or_compile(&template_str)
            .expect("Failed to compile template");
        let ctx = RenderContext::new("char_2")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &player);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
            format!("Aldran {expected_past}.")
        );
    }
}

#[test]
fn test_dynamic_past_tense_irregular_phrasal() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let template = cache
        .get_or_compile("{*A:source:subj} [source:catch up].")
        .expect("Failed to compile template");
    let ctx = RenderContext::new("char_2")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &player);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
        "Aldran caught up."
    );
}

#[test]
fn test_dynamic_past_tense_forced_conjugation() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let ctx_director_present = RenderContext::new("char_2").with_entity("source", &player);
    let ctx_director_past = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Past);

    // 1. Present-only override (falls back to native algorithmic conjugation for past)
    let t_pres_only = cache
        .get_or_compile("{*A:source:subj} [source:freak out|freak out|freaks out].")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&t_pres_only, &ctx_director_present)
            .expect("Failed to render template"),
        "Aldran freaks out."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_pres_only, &ctx_director_past)
            .expect("Failed to render template"),
        "Aldran freaked out."
    );

    // 2. Both present and past overrides
    let t_both = cache
        .get_or_compile("{*A:source:subj} [source:be|am|are|is;was|were|was] here.")
        .expect("Failed to compile template");

    let ctx_actor_first_present = RenderContext::new("char_1")
        .with_entity("source", &player)
        .with_stance(crate::models::ActorStance::FirstPerson);
    let ctx_actor_first_past = RenderContext::new("char_1")
        .with_entity("source", &player)
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_tense(crate::models::Tense::Past);

    assert_eq!(
        PerspectiveEngine::render(&t_both, &ctx_actor_first_present)
            .expect("Failed to render template"),
        "I am here."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_both, &ctx_actor_first_past)
            .expect("Failed to render template"),
        "I was here."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_both, &ctx_director_present)
            .expect("Failed to render template"),
        "Aldran is here."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_both, &ctx_director_past).expect("Failed to render template"),
        "Aldran was here."
    );

    // 3. Past-only override (falls back to native algorithmic conjugation for present)
    let t_past_only = cache
        .get_or_compile("{*A:source:subj} [source:bloop|;blorped].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_past_only, &ctx_director_present)
            .expect("Failed to render template"),
        "Aldran bloops."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_past_only, &ctx_director_past)
            .expect("Failed to render template"),
        "Aldran blorped."
    );
}

#[test]
fn test_dynamic_past_tense_pronouns_and_possessives() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    // Tests combining dynamically shifted verbs with possessive pronouns, absolute possessives, and reflexive pronouns.
    let template = cache
        .get_or_compile("{*A:source:subj} [source:draw] {a:source:poss} sword to defend {a:source:reflex}. The victory [source:be] {:source:abs_poss}!")
        .expect("Failed to compile template");

    let ctx_present = RenderContext::new("char_2").with_entity("source", &player);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_present).expect("Failed to render template"),
        "Aldran draws his sword to defend himself. The victory is his!"
    );

    let ctx_past = RenderContext::new("char_2")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &player);

    // Pronouns should not be affected by tense, but all verbs ("draw" -> "drew", "be" -> "was") should shift.
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_past).expect("Failed to render template"),
        "Aldran drew his sword to defend himself. The victory was his!"
    );
}

#[test]
fn test_dynamic_past_tense_have_and_be() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile(
            "{*A:source:subj} [source:have] no choice, {a:source:subj} [source:be] trapped.",
        )
        .expect("Failed to compile template");

    let ctx_present = RenderContext::new("char_2").with_entity("source", &player);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_present).expect("Failed to render template"),
        "Aldran has no choice, he is trapped."
    );

    let ctx_past = RenderContext::new("char_2")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &player);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_past).expect("Failed to render template"),
        "Aldran had no choice, he was trapped."
    );
}

#[test]
fn test_dynamic_past_tense_anaphora() {
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
    let template = cache
        .get_or_compile(
            "{*the:source:subj} [source:strike] {*A:target:obj}. {a:target:Subj} [target:fall].",
        )
        .expect("Failed to compile template");

    let ctx_past = RenderContext::new("char_3")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &goblin)
        .with_entity("target", &player);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_past).expect("Failed to render template"),
        "The goblin struck Aldran. He fell."
    );
}

#[test]
fn test_colliding_verbs_disambiguation() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // 1. lie -> lay (e.g. resting)
    let t_lay = cache
        .get_or_compile("{*A:source:subj} [source:lie(lay)] down.")
        .expect("Failed to compile template");
    // 2. lie -> lied (e.g. deceiving)
    let t_lied = cache
        .get_or_compile("{*A:source:subj} [source:lie(lied)] to me.")
        .expect("Failed to compile template");

    let ctx_pres = RenderContext::new("char_2").with_entity("source", &player);
    let ctx_past = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Past);

    // In the present tense, both evaluate to "lies"
    assert_eq!(
        PerspectiveEngine::render(&t_lay, &ctx_pres).expect("Failed to render template"),
        "Aldran lies down."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_lied, &ctx_pres).expect("Failed to render template"),
        "Aldran lies to me."
    );

    // In the past tense, they diverge to their intended meanings!
    assert_eq!(
        PerspectiveEngine::render(&t_lay, &ctx_past).expect("Failed to render template"),
        "Aldran lay down."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_lied, &ctx_past).expect("Failed to render template"),
        "Aldran lied to me."
    );
}

#[test]
fn test_dynamic_past_tense_capitalization() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // 1. Regular Verb
    let t_walk = cache
        .get_or_compile("[source:Walk] away.")
        .expect("Failed to compile template");
    // 2. Irregular Verb
    let t_run = cache
        .get_or_compile("[source:Run] away.")
        .expect("Failed to compile template");
    // 3. Phrasal Verb
    let t_pick = cache
        .get_or_compile("[source:Pick up] the sword.")
        .expect("Failed to compile template");
    // 4. "To Be"
    let t_be = cache
        .get_or_compile("[source:Be] ready.")
        .expect("Failed to compile template");

    let ctx = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Past);

    // All of these should retain their first-letter capitalization despite dynamic shifting!
    assert_eq!(
        PerspectiveEngine::render(&t_walk, &ctx).expect("Failed to render template"),
        "Walked away."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_run, &ctx).expect("Failed to render template"),
        "Ran away."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_pick, &ctx).expect("Failed to render template"),
        "Picked up the sword."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_be, &ctx).expect("Failed to render template"),
        "Was ready."
    );
}

#[test]
fn test_dynamic_past_tense_modal_verbs() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Past);

    // Modal verbs naturally shift to past tense
    let t_can = cache
        .get_or_compile("{*A:source:subj} [source:can] win.")
        .expect("Failed to compile template");
    let t_will = cache
        .get_or_compile("{*A:source:subj} [source:will] win.")
        .expect("Failed to compile template");
    let t_shall = cache
        .get_or_compile("{*A:source:subj} [source:shall] win.")
        .expect("Failed to compile template");
    let t_may = cache
        .get_or_compile("{*A:source:subj} [source:may] win.")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&t_can, &ctx).expect("Failed to render template"),
        "Aldran could win."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_will, &ctx).expect("Failed to render template"),
        "Aldran would win."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_shall, &ctx).expect("Failed to render template"),
        "Aldran should win."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_may, &ctx).expect("Failed to render template"),
        "Aldran might win."
    );
}

#[test]
fn test_dynamic_future_tense() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Future);

    // 1. Regular verb
    let t_walk = cache
        .get_or_compile("{*A:source:subj} [source:walk].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_walk, &ctx).expect("Failed to render template"),
        "Aldran will walk."
    );

    // 2. Irregular verb
    let t_be = cache
        .get_or_compile("{*A:source:subj} [source:be] ready.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_be, &ctx).expect("Failed to render template"),
        "Aldran will be ready."
    );

    // 3. Phrasal verb
    let t_pick = cache
        .get_or_compile("{*A:source:subj} [source:pick up] the sword.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_pick, &ctx).expect("Failed to render template"),
        "Aldran will pick up the sword."
    );

    // 4. Modal verbs (should remain unchanged, preventing "will can")
    let t_can = cache
        .get_or_compile("{*A:source:subj} [source:can] win.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_can, &ctx).expect("Failed to render template"),
        "Aldran can win."
    );

    // 5. Capitalization preservation
    let t_cap = cache
        .get_or_compile("[source:Attack]!")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_cap, &ctx).expect("Failed to render template"),
        "Will attack!"
    );

    // 6. Forced conjugation ignored natively
    let t_forced = cache
        .get_or_compile("{*A:source:subj} [source:freak out|freak out|freaks out]!")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_forced, &ctx).expect("Failed to render template"),
        "Aldran will freak out!"
    );

    // 7. Phrasal modal and quasi-modal edge cases
    let t_have_to = cache
        .get_or_compile("{*A:source:subj} [source:have to] win.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_have_to, &ctx).expect("Failed to render template"),
        "Aldran will have to win."
    );

    let t_ought_to = cache
        .get_or_compile("{*A:source:subj} [source:ought to] win.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_ought_to, &ctx).expect("Failed to render template"),
        "Aldran ought to win."
    );
}

#[test]
fn test_dynamic_future_tense_pronouns_and_possessives() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    // Tests combining dynamically shifted future verbs with possessive pronouns, absolute possessives, and reflexive pronouns.
    let template = cache
        .get_or_compile("{*A:source:subj} [source:draw] {a:source:poss} sword to defend {a:source:reflex}. The victory [source:be] {:source:abs_poss}!")
        .expect("Failed to compile template");

    let ctx_future = RenderContext::new("char_2")
        .with_tense(crate::models::Tense::Future)
        .with_entity("source", &player);

    // Pronouns should not be affected by tense, but all verbs ("draw" -> "will draw", "be" -> "will be") should shift.
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_future).expect("Failed to render template"),
        "Aldran will draw his sword to defend himself. The victory will be his!"
    );
}

#[test]
fn test_dynamic_future_tense_anaphora() {
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
    let template = cache
        .get_or_compile(
            "{*the:source:subj} [source:strike] {*A:target:obj}. {a:target:Subj} [target:fall].",
        )
        .expect("Failed to compile template");

    let ctx_future = RenderContext::new("char_3")
        .with_tense(crate::models::Tense::Future)
        .with_entity("source", &goblin)
        .with_entity("target", &player);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_future).expect("Failed to render template"),
        "The goblin will strike Aldran. He will fall."
    );
}

#[test]
fn test_dynamic_future_tense_with_groups() {
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
    let party = GroupEntity {
        members: vec![&player, &goblin],
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{*the:source:subj} [source:be] here, and [source:try] to escape.")
        .expect("Failed to compile template");

    let ctx_party_actor = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_tense(crate::models::Tense::Future)
        .with_entity("source", &party);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_party_actor).expect("Failed to render template"),
        "The goblin and I will be here, and will try to escape."
    );
}

#[test]
fn test_dynamic_future_tense_force_director_stance() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{*a:+source:subj} [+source:win] the battle.")
        .expect("Failed to compile template");

    let ctx_future = RenderContext::new("char_1") // Player is the viewer
        .with_tense(crate::models::Tense::Future)
        .with_entity("source", &player);

    // Even though the viewer is the actor, the `+` syntax forces third person logic
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_future).expect("Failed to render template"),
        "Aldran will win the battle."
    );
}

#[test]
fn test_dynamic_verb_injection() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{*A:source:subj} [source:$action].")
        .expect("Failed to compile template");
    let template_cap = cache
        .get_or_compile("{*A:source:subj} [source:$Action].")
        .expect("Failed to compile template");

    // 1. Single dynamic verb
    let ctx1 = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_variable("action", "smile");
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx1).expect("Failed to render template"),
        "Aldran smiles."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_cap, &ctx1).expect("Failed to render template"),
        "Aldran Smiles."
    );

    // 2. Multiple dynamic verbs (Oxford comma formatting)
    let ctx2 = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_variables("action", ["smile", "wave", "dance"]);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx2).expect("Failed to render template"),
        "Aldran smiles, waves, and dances."
    );

    // 3. Tense shifting on dynamic verbs natively applies to all list members
    let ctx3 = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Past)
        .with_variables("action", ["run", "jump"]);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx3).expect("Failed to render template"),
        "Aldran ran and jumped."
    );

    // 4. Missing variable fails gracefully
    let ctx4 = RenderContext::new("char_2").with_entity("source", &player);
    assert!(PerspectiveEngine::render(&template, &ctx4).is_err());
}

#[test]
fn test_dynamic_variable_injection() {
    let cache = TemplateCache::new(100);

    // 1. Single variable insertion
    let t1 = cache
        .get_or_compile("It is {$weather} today.")
        .expect("Failed to compile template");
    let ctx1 = RenderContext::new("viewer").with_variable("weather", "raining");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template"),
        "It is raining today."
    );

    // 2. Capitalization and Oxford Comma listing
    let t2 = cache
        .get_or_compile("{$Colors} are my favorite.")
        .expect("Failed to compile template");
    let ctx2 = RenderContext::new("viewer").with_variables("colors", ["red", "green", "blue"]);
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx2).expect("Failed to render template"),
        "Red, green, and blue are my favorite."
    );

    // 3. All Caps
    let t3 = cache
        .get_or_compile("IT IS {$WEATHER}!")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx1).expect("Failed to render template"),
        "IT IS RAINING!"
    );

    // 4. Missing variable fails gracefully
    let ctx_empty = RenderContext::new("viewer");
    assert!(PerspectiveEngine::render(&t1, &ctx_empty).is_err());

    // 5. Spacing resilience
    let t4 = cache
        .get_or_compile("It is { $weather } today.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t4, &ctx1).expect("Failed to render template"),
        "It is raining today."
    );
}

#[test]
fn test_all_twelve_english_tenses() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let t_simple = cache
        .get_or_compile("{*A:source:subj} [source:walk].")
        .expect("Failed to compile template");
    let t_continuous = cache
        .get_or_compile("{*A:source:subj} [source:be] walking.")
        .expect("Failed to compile template");
    let t_perfect = cache
        .get_or_compile("{*A:source:subj} [source:have] walked.")
        .expect("Failed to compile template");
    let t_perfect_continuous = cache
        .get_or_compile("{*A:source:subj} [source:have] been walking.")
        .expect("Failed to compile template");

    let ctx_pres = RenderContext::new("char_2").with_entity("source", &player);
    let ctx_past = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Past);
    let ctx_future = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Future);

    // 1. Simple Tenses
    assert_eq!(
        PerspectiveEngine::render(&t_simple, &ctx_pres).expect("Failed to render template"),
        "Aldran walks."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_simple, &ctx_past).expect("Failed to render template"),
        "Aldran walked."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_simple, &ctx_future).expect("Failed to render template"),
        "Aldran will walk."
    );

    // 2. Continuous Tenses
    assert_eq!(
        PerspectiveEngine::render(&t_continuous, &ctx_pres).expect("Failed to render template"),
        "Aldran is walking."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_continuous, &ctx_past).expect("Failed to render template"),
        "Aldran was walking."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_continuous, &ctx_future).expect("Failed to render template"),
        "Aldran will be walking."
    );

    // 3. Perfect Tenses
    assert_eq!(
        PerspectiveEngine::render(&t_perfect, &ctx_pres).expect("Failed to render template"),
        "Aldran has walked."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_perfect, &ctx_past).expect("Failed to render template"),
        "Aldran had walked."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_perfect, &ctx_future).expect("Failed to render template"),
        "Aldran will have walked."
    );

    // 4. Perfect Continuous Tenses
    assert_eq!(
        PerspectiveEngine::render(&t_perfect_continuous, &ctx_pres)
            .expect("Failed to render template"),
        "Aldran has been walking."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_perfect_continuous, &ctx_past)
            .expect("Failed to render template"),
        "Aldran had been walking."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_perfect_continuous, &ctx_future)
            .expect("Failed to render template"),
        "Aldran will have been walking."
    );
}

#[test]
fn test_dynamic_future_tense_do_support() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let ctx_pres = RenderContext::new("char_2").with_entity("source", &player);
    let ctx_past = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Past);
    let ctx_future = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_tense(crate::models::Tense::Future);

    // 1. Negative Sentence
    let t_neg = cache
        .get_or_compile("{*A:source:subj} [source:do(aux)] not run.")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&t_neg, &ctx_pres).expect("Failed to render template"),
        "Aldran does not run."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_neg, &ctx_past).expect("Failed to render template"),
        "Aldran did not run."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_neg, &ctx_future).expect("Failed to render template"),
        "Aldran will not run."
    );

    // 2. Question Sentence (Capitalized)
    let t_question = cache
        .get_or_compile("[source:Do(aux)] {a:source:subj} run?")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&t_question, &ctx_pres).expect("Failed to render template"),
        "Does he run?"
    );
    assert_eq!(
        PerspectiveEngine::render(&t_question, &ctx_past).expect("Failed to render template"),
        "Did he run?"
    );
    assert_eq!(
        PerspectiveEngine::render(&t_question, &ctx_future).expect("Failed to render template"),
        "Will he run?"
    );

    // 3. Main Verb (Unannotated "do")
    let t_main = cache
        .get_or_compile("{*A:source:subj} [source:do] the laundry.")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&t_main, &ctx_pres).expect("Failed to render template"),
        "Aldran does the laundry."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_main, &ctx_past).expect("Failed to render template"),
        "Aldran did the laundry."
    );
    assert_eq!(
        PerspectiveEngine::render(&t_main, &ctx_future).expect("Failed to render template"),
        "Aldran will do the laundry."
    );
}

#[test]
fn test_modal_verbs_perspectives() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "Goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // --- TEST 1: The modal verb "must" ---
    let template_must = cache
        .get_or_compile("{*A:source:subj} [source:must] flee from {*the:target:obj}!")
        .unwrap();

    // Actor Stance (Player is the one fleeing)
    let actor_must =
        render_msg!("char_1", &template_must, "source" => &player, "target" => &goblin).unwrap();
    assert_eq!(actor_must, "You must flee from the Goblin!");

    // Director Stance (A bystander is watching the Player flee)
    // The engine should output "must", NOT "musts"
    let director_must =
        render_msg!("char_3", &template_must, "source" => &player, "target" => &goblin).unwrap();
    assert_eq!(director_must, "Aldran must flee from the Goblin!");

    // --- TEST 2: Multiple modal verbs ("can" and "will") in a complex sentence ---
    let template_can = cache
        .get_or_compile(
            "if {*a:source:subj} [source:can] catch {*the:target:obj}, {a:source:subj} [source:will] win.",
        )
        .unwrap();

    // Actor Stance
    let actor_can =
        render_msg!("char_1", &template_can, "source" => &player, "target" => &goblin).unwrap();
    assert_eq!(actor_can, "If you can catch the Goblin, you will win.");

    // Director Stance
    // The engine should output "can" and "will", NOT "cans" and "wills"
    let director_can =
        render_msg!("char_3", &template_can, "source" => &player, "target" => &goblin).unwrap();
    assert_eq!(director_can, "If Aldran can catch the Goblin, he will win.");

    // --- TEST 3: Modal verbs interacting with plural targets ---
    let wolves = MockEntity {
        id: "mob_2".to_string(),
        name: "pack of wolves".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let template_should = cache
        .get_or_compile(
            "{*the:source:subj} [source:should] be careful, or {*the:target:subj} [target:might] attack.",
        )
        .expect("Failed to compile template");

    let observer_should =
        render_msg!("char_3", &template_should, "source" => &player, "target" => &wolves)
            .expect("Failed to render template");
    assert_eq!(
        observer_should,
        "Aldran should be careful, or the pack of wolves might attack."
    );
}

#[test]
fn test_extended_forced_conjugation() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let wolves = MockEntity {
        id: "mob_1".to_string(),
        name: "wolves".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // 1. Two-part syntax (Base/Plural | 3rd Singular)
    let template_2 = cache
        .get_or_compile("{*the:source:subj} [source:freak out|freak out|freaks out].")
        .expect("Failed to compile template");
    assert_eq!(
        render_msg!("char_1", &template_2, "source" => &player).expect("Failed to render template"),
        "You freak out."
    );
    assert_eq!(
        render_msg!("char_2", &template_2, "source" => &player).expect("Failed to render template"),
        "Aldran freaks out."
    );
    assert_eq!(
        render_msg!("char_2", &template_2, "source" => &wolves).expect("Failed to render template"),
        "The wolves freak out."
    );

    // 2. Three-part syntax (1st Singular | 2nd/Plural | 3rd Singular)
    let template_3 = cache
        .get_or_compile("{*the:source:subj} [source:be|was|were|was] here.")
        .expect("Failed to compile template");
    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player);
    let ctx_second = RenderContext::new("char_1").with_entity("source", &player);
    let ctx_third = RenderContext::new("char_2").with_entity("source", &player);
    let ctx_plural = RenderContext::new("char_2").with_entity("source", &wolves);

    assert_eq!(
        PerspectiveEngine::render(&template_3, &ctx_first).expect("Failed to render template"),
        "I was here."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_3, &ctx_second).expect("Failed to render template"),
        "You were here."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_3, &ctx_third).expect("Failed to render template"),
        "Aldran was here."
    );
    assert_eq!(
        PerspectiveEngine::render(&template_3, &ctx_plural).expect("Failed to render template"),
        "The wolves were here."
    );
}

#[test]
fn test_capitalized_irregular_verbs() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // --- TEST 1: "Be" -> "Is" ---
    // The post-processor will capitalize the 'A' of Aldran. The conjugation logic
    // should capitalize the 'I' of 'is' because the base verb "Be" is capitalized.
    let template_be = cache
        .get_or_compile("{*A:source:subj} [source:Be] here.")
        .expect("Failed to compile template");
    let director_be = render_msg!("char_3", &template_be, "source" => &player)
        .expect("Failed to render template");
    assert_eq!(director_be, "Aldran Is here.");

    // --- TEST 2: "Have" -> "Has" ---
    let template_have = cache
        .get_or_compile("{*A:source:subj} [source:Have] a sword.")
        .expect("Failed to compile template");
    let director_have = render_msg!("char_3", &template_have, "source" => &player)
        .expect("Failed to render template");
    assert_eq!(director_have, "Aldran Has a sword.");
}

#[test]
fn test_exceptionally_short_verbs() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let template_y = cache
        .get_or_compile("{*A:source:subj} [source:y].")
        .expect("Failed to compile template");
    let output_y =
        render_msg!("char_3", &template_y, "source" => &player).expect("Failed to render template");
    assert_eq!(output_y, "Aldran ys.");

    let template_empty = cache
        .get_or_compile("{*A:source:subj} [source:].")
        .expect("Failed to compile template");
    let output_empty = render_msg!("char_3", &template_empty, "source" => &player)
        .expect("Failed to render template");
    assert_eq!(output_empty, "Aldran s.");
}

#[test]
fn test_demonstratives_and_past_tense() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolves = MockEntity {
        id: "mob_2".to_string(),
        name: "wolves".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // 1. Demonstratives (this -> these) combined with Past Tense "To Be" (was -> were)
    let template = cache
        .get_or_compile("{*This:source:subj} [source:be] angry.")
        .expect("Failed to compile template");

    let ctx_singular = RenderContext::new("char_2")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &goblin);
    let out_singular =
        PerspectiveEngine::render(&template, &ctx_singular).expect("Failed to render template");
    assert_eq!(out_singular, "This goblin was angry.");

    let ctx_plural = RenderContext::new("char_2")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &wolves);
    let out_plural =
        PerspectiveEngine::render(&template, &ctx_plural).expect("Failed to render template");
    assert_eq!(out_plural, "These wolves were angry.");

    // 2. Automatically suppresses the demonstrative for the viewer just like an article
    let ctx_viewer = RenderContext::new("mob_2")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &wolves);
    let out_viewer =
        PerspectiveEngine::render(&template, &ctx_viewer).expect("Failed to render template");
    assert_eq!(out_viewer, "You were angry.");

    // 3. Forcing an article for a proper noun using the `+` prefix
    let template_force = cache
        .get_or_compile("{*+This:source:subj} [source:be] angry.")
        .expect("Failed to compile template");
    let aldran = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let out_forced = render_msg!("char_2", &template_force, "source" => &aldran)
        .expect("Failed to render template");
    assert_eq!(out_forced, "This Aldran is angry.");
}

#[test]
fn test_unbound_forced_director_verbs() {
    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer");

    // [+smile] should output "smiles" (director stance, which is default anyway, but tests parser stripping)
    let out_director = PerspectiveEngine::render(
        &cache
            .get_or_compile("he [+smile].")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");
    assert_eq!(out_director, "He smiles.");
}

#[test]
fn test_absolute_possessive_pronouns() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let wolves = MockEntity {
        id: "mob_1".to_string(),
        name: "wolves".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("The victory is {:source:abs_poss}!")
        .expect("Failed to compile template");

    let out_actor =
        render_msg!("char_1", &template, "source" => &player).expect("Failed to render template");
    assert_eq!(out_actor, "The victory is yours!");

    // Seed the anaphora memory so it evaluates the pronoun instead of falling back to "Aldran's"
    let ctx_director = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_last_mentioned("source");
    let out_director =
        PerspectiveEngine::render(&template, &ctx_director).expect("Failed to render template");
    assert_eq!(out_director, "The victory is his!");

    let ctx_plural = RenderContext::new("char_2")
        .with_entity("source", &wolves)
        .with_last_mentioned("source");
    let out_plural =
        PerspectiveEngine::render(&template, &ctx_plural).expect("Failed to render template");
    assert_eq!(out_plural, "The victory is theirs!");
}

#[test]
fn test_unbound_verbs() {
    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer");

    // Without a subject, verbs safely default to 3rd-person singular conjugation
    let template = cache
        .get_or_compile("a shadow [loom] in the distance, and [approach].")
        .expect("Failed to compile template");
    let out = PerspectiveEngine::render(&template, &ctx).expect("Failed to render template");
    assert_eq!(out, "A shadow looms in the distance, and approaches.");
}

#[test]
fn test_dynamic_possessive_nouns() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let wolves = MockEntity {
        id: "mob_1".to_string(),
        name: "wolves".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };
    let boss = MockEntity {
        id: "mob_2".to_string(),
        name: "boss".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("You take {*the:target's:poss} gold.")
        .expect("Failed to compile template");

    // 1. Viewer
    let out_viewer =
        render_msg!("char_1", &template, "target" => &player).expect("Failed to render template");
    assert_eq!(out_viewer, "You take your gold.");

    // 2. Singular Proper Noun
    let out_proper =
        render_msg!("char_2", &template, "target" => &player).expect("Failed to render template");
    assert_eq!(out_proper, "You take Aldran's gold.");

    // 3. Plural common noun ending in 's'
    let out_plural =
        render_msg!("char_2", &template, "target" => &wolves).expect("Failed to render template");
    assert_eq!(out_plural, "You take the wolves' gold.");

    // 4. Singular common noun ending in 's'
    let out_boss =
        render_msg!("char_2", &template, "target" => &boss).expect("Failed to render template");
    assert_eq!(out_boss, "You take the boss's gold.");

    // 5. Group Entities with possessive suffixes
    // English attaches joint possessives to the final noun. The engine natively looks
    // at the end of the formatted list to determine if it should use 's or just '.
    let wolf_party = GroupEntity::new(vec![&player, &wolves]);
    let out_wolf_party = render_msg!("char_2", &template, "target" => &wolf_party)
        .expect("Failed to render template");
    assert_eq!(out_wolf_party, "You take Aldran and the wolves' gold.");

    // 6. Forced Director Stance with Possessive Suffixes
    let template_forced = cache
        .get_or_compile("You take {*a:+target's:poss} gold.")
        .expect("Failed to compile template");
    // Even though the viewer is char_1 (the player), the + prefix overrides "your" to "Aldran's"
    let out_forced = render_msg!("char_1", &template_forced, "target" => &player)
        .expect("Failed to render template");
    assert_eq!(out_forced, "You take Aldran's gold.");
}

#[test]
fn test_number_to_ordinal_word() {
    assert_eq!(crate::grammar::number_to_ordinal_word(3, 9999), "third");
    assert_eq!(
        crate::grammar::number_to_ordinal_word(21, 9999),
        "twenty-first"
    );
    assert_eq!(crate::grammar::number_to_ordinal_word(50, 9999), "fiftieth");
    assert_eq!(
        crate::grammar::number_to_ordinal_word(108, 9999),
        "one hundred and eighth"
    );
    assert_eq!(
        crate::grammar::number_to_ordinal_word(111, 9999),
        "one hundred and eleventh"
    );
    assert_eq!(
        crate::grammar::number_to_ordinal_word(999, 9999),
        "nine hundred and ninety-ninth"
    );
    assert_eq!(
        crate::grammar::number_to_ordinal_word(1000, 9999),
        "one thousandth"
    );
    assert_eq!(
        crate::grammar::number_to_ordinal_word(1001, 9999),
        "one thousand and first"
    );
    assert_eq!(
        crate::grammar::number_to_ordinal_word(2022, 9999),
        "two thousand and twenty-second"
    );
    assert_eq!(
        crate::grammar::number_to_ordinal_word(1_234_567, usize::MAX),
        "one million two hundred and thirty-four thousand five hundred and sixty-seventh"
    );
    assert_eq!(
        crate::grammar::number_to_ordinal_word(18_000_000_000_000_000_001, usize::MAX),
        "eighteen quintillion and first"
    );

    // Test threshold behavior
    assert_eq!(crate::grammar::number_to_ordinal_word(3, 0), "3rd");
    assert_eq!(crate::grammar::number_to_ordinal_word(21, 20), "21st");
}
