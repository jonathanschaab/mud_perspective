use super::common::MockEntity;
use crate::cache::TemplateCache;
use crate::engine::{PerspectiveEngine, Template};
use crate::models::{Gender, RenderContext};

#[test]
fn test_actor_vs_director_stance() {
    let aldran = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    // Note: We use lowercase text. The post-processor will handle capitalization safely.
    let template =
        Template::compile("{*A:source:subj} [source:be] looking around for {a:source:poss} sword.")
            .expect("Failed to compile template");

    // 1. Actor Stance (Aldran is the viewer)
    let ctx_actor = RenderContext::new("char_1").with_entity("source", &aldran);
    let actor_output =
        PerspectiveEngine::render(&template, &ctx_actor).expect("Failed to render template");
    assert_eq!(actor_output, "You are looking around for your sword.");

    // 2. Director Stance (A third-party observer)
    let ctx_director = RenderContext::new("char_2").with_entity("source", &aldran);
    let director_output =
        PerspectiveEngine::render(&template, &ctx_director).expect("Failed to render template");
    assert_eq!(director_output, "Aldran is looking around for his sword.");
}

#[test]
fn test_plurality_and_verb_binding() {
    let wolves = MockEntity {
        id: "mob_1".to_string(),
        name: "pack of wolves".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    // Testing the "active subject fallacy" fix by explicitly binding the verb to the wolves
    let template =
        Template::compile("{*the:target:subj} watches as {*the:source:subj} [source:attack]!")
            .expect("Failed to compile template");

    let ctx = RenderContext::new("char_2")
        .with_entity("source", &wolves)
        .with_entity("target", &player);

    let output = PerspectiveEngine::render(&template, &ctx).expect("Failed to render template");

    // Because wolves are plural, the verb "attack" should NOT become "attacks",
    // even though it's evaluating in the third person.
    assert_eq!(output, "Aldran watches as the pack of wolves attack!");
}

#[test]
fn test_actor_stances() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{*A:source:subj} [source:walk] forward.")
        .expect("Failed to compile template");

    // Second Person (Default)
    let out_second =
        render_msg!("char_1", &template, "source" => &player).expect("Failed to render template");
    assert_eq!(out_second, "You walk forward.");

    // First Person
    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player);
    let out_first =
        PerspectiveEngine::render(&template, &ctx_first).expect("Failed to render template");
    assert_eq!(out_first, "I walk forward.");

    // Third Person
    let ctx_third = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::ThirdPerson)
        .with_entity("source", &player);
    let out_third =
        PerspectiveEngine::render(&template, &ctx_third).expect("Failed to render template");
    assert_eq!(out_third, "Aldran walks forward.");
}

#[test]
fn test_first_person_conjugation_and_pronouns() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let template_be = cache
        .get_or_compile("{*A:source:subj} [source:be] looking for {a:source:poss} sword.")
        .expect("Failed to compile template");
    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player);
    assert_eq!(
        PerspectiveEngine::render(&template_be, &ctx_first).expect("Failed to render template"),
        "I am looking for my sword."
    );

    let template_past = cache
        .get_or_compile("Before, {*A:source:subj} [source:be] looking.")
        .expect("Failed to compile template");
    let ctx_past = ctx_first.with_tense(crate::models::Tense::Past);
    assert_eq!(
        PerspectiveEngine::render(&template_past, &ctx_past).expect("Failed to render template"),
        "Before, I was looking."
    );
}

#[test]
fn test_possessive_nouns_with_stances() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    // Tests whether `{source's}` evaluates to "my", "your", or "Aldran's"
    let template = cache
        .get_or_compile("They take {*a:source's:poss} gold.")
        .expect("Failed to compile template");

    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first).expect("Failed to render template"),
        "They take my gold."
    );

    let ctx_second = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::SecondPerson)
        .with_entity("source", &player);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_second).expect("Failed to render template"),
        "They take your gold."
    );

    let ctx_third = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::ThirdPerson)
        .with_entity("source", &player);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_third).expect("Failed to render template"),
        "They take Aldran's gold."
    );
}
