use super::common::{ConfigurableMockEntity, MockEntity};
use crate::cache::TemplateCache;
use crate::engine::{PerspectiveEngine, Template};
use crate::models::{Gender, RenderContext, TemplateEntity};
use std::borrow::Cow;

#[test]
fn test_epistemological_masking_and_articles() {
    let aldran = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(), // Will be masked as "tall man" to strangers
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true, // is_proper_noun_for returns false for strangers
    };

    let template = Template::compile("{*a:source:subj} [source:approach].")
        .expect("Failed to compile template");

    let ctx_stranger = RenderContext::new("stranger_1").with_entity("source", &aldran);
    let stranger_output =
        PerspectiveEngine::render(&template, &ctx_stranger).expect("Failed to render template");

    // The engine should dynamically add the article "a", and capitalize the sentence
    assert_eq!(stranger_output, "A tall man approaches.");
}

#[test]
fn test_article_suppression() {
    let aldran = MockEntity {
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

    // --- SCENARIO 1: `{the:key}` on a common noun ---
    let template_the = cache
        .get_or_compile("{*the:source:subj} is here.")
        .expect("Failed to compile template");
    let output_the = render_msg!("char_2", &template_the, "source" => &goblin)
        .expect("Failed to render template");
    assert_eq!(output_the, "The goblin is here.");

    // --- SCENARIO 2: `{a:key}` suppressed for a proper noun ---
    let template_a_proper = cache
        .get_or_compile("{*a:source:subj} is here.")
        .expect("Failed to compile template");
    let output_a_proper = render_msg!("char_2", &template_a_proper, "source" => &aldran)
        .expect("Failed to render template");
    assert_eq!(output_a_proper, "Aldran is here.");

    // --- SCENARIO 3: `{the:key}` suppressed for a proper noun ---
    let template_the_proper = cache
        .get_or_compile("{*the:source:subj} is here.")
        .expect("Failed to compile template");
    let output_the_proper = render_msg!("char_2", &template_the_proper, "source" => &aldran)
        .expect("Failed to render template");
    assert_eq!(output_the_proper, "Aldran is here.");

    // --- SCENARIO 4: `{a:key}` suppressed for the viewer ---
    let template_a_viewer = cache
        .get_or_compile("{*a:source:subj} [source:be] here.")
        .expect("Failed to compile template");
    let output_a_viewer = render_msg!("char_1", &template_a_viewer, "source" => &aldran)
        .expect("Failed to render template");
    assert_eq!(output_a_viewer, "You are here.");

    // --- SCENARIO 5: `{the:key}` suppressed for the viewer ---
    let template_the_viewer = cache
        .get_or_compile("{*the:source:subj} [source:be] here.")
        .expect("Failed to compile template");
    let output_the_viewer = render_msg!("char_1", &template_the_viewer, "source" => &aldran)
        .expect("Failed to render template");
    assert_eq!(output_the_viewer, "You are here.");

    // --- SCENARIO 6: Plural proper nouns (e.g. "The Smiths", "The Avengers") ---
    // The correct way to handle these is to include "the" in the entity's name
    // and flag it as a proper noun so the engine doesn't inject redundant articles.
    let avengers = MockEntity {
        id: "mob_2".to_string(),
        name: "the Avengers".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: true,
    };

    // Indefinite article "a" is suppressed, leaving the base name "the Avengers" (capitalized by typography)
    let template_a_plural = cache
        .get_or_compile("{*a:source:subj} assemble!")
        .expect("Failed to compile template");
    let output_a_plural = render_msg!("char_2", &template_a_plural, "source" => &avengers)
        .expect("Failed to render template");
    assert_eq!(output_a_plural, "The Avengers assemble!");

    // Definite article "the" is suppressed, leaving the base name "the Avengers"
    let template_the_plural = cache
        .get_or_compile("{*The:source:subj} assemble!")
        .expect("Failed to compile template");
    let output_the_plural = render_msg!("char_2", &template_the_plural, "source" => &avengers)
        .expect("Failed to render template");
    assert_eq!(output_the_plural, "The Avengers assemble!");

    // --- SCENARIO 7: Plural common nouns (e.g. "wolves") ---
    let wolves = MockEntity {
        id: "mob_3".to_string(),
        name: "wolves".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    // Indefinite article "a" should evaluate to "some" for plural common nouns
    let template_a_common_plural = cache
        .get_or_compile("{*a:source:subj} howl.")
        .expect("Failed to compile template");
    let output_a_common_plural =
        render_msg!("char_2", &template_a_common_plural, "source" => &wolves)
            .expect("Failed to render template");
    assert_eq!(output_a_common_plural, "Some wolves howl.");

    // Definite article "the" should NOT be suppressed for plural common nouns
    let template_the_common_plural = cache
        .get_or_compile("{*The:source:subj} howl.")
        .expect("Failed to compile template");
    let output_the_common_plural =
        render_msg!("char_2", &template_the_common_plural, "source" => &wolves)
            .expect("Failed to render template");
    assert_eq!(output_the_common_plural, "The wolves howl.");
}

#[test]
fn test_disguised_plural_proper_nouns() {
    let avengers = MockEntity {
        id: "mob_2".to_string(),
        name: "the Avengers".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let template_a = cache
        .get_or_compile("{*a:source:subj} [source:arrive].")
        .unwrap();
    let template_the = cache
        .get_or_compile("{*the:source:subj} [source:arrive].")
        .unwrap();

    // 1. Friend's perspective (knows they are The Avengers)
    let out_friend_a = render_msg!("char_2", &template_a, "source" => &avengers).unwrap();
    let out_friend_the = render_msg!("char_2", &template_the, "source" => &avengers).unwrap();

    // Suppresses articles natively because they are recognized as a proper noun
    assert_eq!(out_friend_a, "The Avengers arrive.");
    assert_eq!(out_friend_the, "The Avengers arrive.");

    // 2. Stranger's perspective (sees "masked heroes")
    let out_stranger_a = render_msg!("stranger_1", &template_a, "source" => &avengers).unwrap();
    let out_stranger_the = render_msg!("stranger_1", &template_the, "source" => &avengers).unwrap();

    // Evaluates as a common plural noun, meaning `{a:source}` maps to "Some", and `{the:source}` maps to "The"
    assert_eq!(out_stranger_a, "Some masked heroes arrive.");
    assert_eq!(out_stranger_the, "The masked heroes arrive.");
}

#[test]
fn test_definite_description_upgrade() {
    let wolf1 = MockEntity {
        id: "mob_1".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{*A:source:subj} [source:walk] in. {*A:source:subj} [source:howl].")
        .expect("Failed to compile template");

    let ctx = RenderContext::new("char_1").with_entity("source", &wolf1);

    // First mention uses "A", second uses "The"
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
        "A wolf walks in. The wolf howls."
    );
}

#[test]
fn test_definite_description_upgrade_collision() {
    let wolf1 = MockEntity {
        id: "mob_1".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolf2 = MockEntity {
        id: "mob_2".to_string(),
        name: "wolf".to_string(), // Same display name!
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile(
            "{*A:source:subj} [source:walk] in. {*A:other:subj} [other:walk] in. {*A:source:subj} [source:howl].",
        )
        .expect("Failed to compile template");

    let ctx = RenderContext::new("char_1")
        .with_entity("source", &wolf1)
        .with_entity("other", &wolf2);

    // First is "A", second is "Another", third is "The first"
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
        "A wolf walks in. Another wolf walks in. The first wolf howls."
    );
}

#[test]
fn test_definite_description_upgrade_plural() {
    let wolves = MockEntity {
        id: "mob_1".to_string(),
        name: "wolves".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{*A:source:subj} [source:approach]. {*A:source:subj} [source:howl].")
        .expect("Failed to compile template");

    let ctx = RenderContext::new("char_1").with_entity("source", &wolves);

    // First is "Some wolves", second is "The wolves"
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
        "Some wolves approach. The wolves howl."
    );
}

#[test]
fn test_definite_description_synergy_with_pronouns() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    // Introduces with 'A', uses pronoun, then attempts 'A' again to see if it upgraded to 'The'
    let template = cache
        .get_or_compile("{*A:source:subj} [source:arrive]. {a:source:Subj} [source:wait]. {*A:source:subj} [source:attack]!")
        .expect("Failed to compile template");

    let ctx = RenderContext::new("char_1").with_entity("source", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
        "A goblin arrives. It waits. The goblin attacks!"
    );
}

#[test]
fn test_definite_description_upgrade_with_possessives() {
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
            "{*A:source's:poss} sword [source:fall]. {*A:source's:poss} shield [source:break].",
        )
        .expect("Failed to compile template");

    let ctx = RenderContext::new("char_1").with_entity("source", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
        "A goblin's sword falls. The goblin's shield breaks."
    );
}

#[test]
fn test_definite_description_viewer_suppression() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{*A:source:subj} [source:walk]. {*A:source:subj} [source:run].")
        .expect("Failed to compile template");

    let ctx = RenderContext::new("char_1").with_entity("source", &player);

    // The 'A' and 'The' upgrades are both cleanly suppressed because the entity is the viewer
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
        "You walk. You run."
    );
}

#[test]
fn test_definite_description_upgrade_with_nested_properties() {
    struct Weapon {
        name: String,
    }
    impl TemplateEntity for Weapon {
        fn contains_viewer(&self, _: &str) -> bool {
            false
        }
        fn gender(&self) -> Gender {
            Gender::Neutral
        }
        fn is_plural(&self) -> bool {
            false
        }
        fn is_proper_noun_for(&self, _: &str) -> bool {
            false
        }
        fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
            Cow::Borrowed(&self.name)
        }
    }

    let rusty_sword = Weapon {
        name: "rusty sword".into(),
    };

    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile(
            "{*A:source:subj} [source:draw] {*a:weapon:obj}. {*A:weapon:subj} [weapon:be] sharp.",
        )
        .expect("Failed to compile template");

    let ctx = RenderContext::new("char_1")
        .with_entity("source", &goblin)
        .with_entity("weapon", &rusty_sword);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
        "A goblin draws a rusty sword. The rusty sword is sharp."
    );
}

#[test]
fn test_long_description_disambiguation() {
    let wolf1 = MockEntity {
        id: "mob_1".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolf2 = MockEntity {
        id: "mob_2_long".to_string(),
        name: "wolf".to_string(), // Collides with wolf1!
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // 1. Direct Entity Tags
    let t1 = cache
        .get_or_compile("{*A:source:subj} and {*a:other:subj} arrive.")
        .expect("Failed to compile template");
    let ctx1 = RenderContext::new("char_1")
        .with_entity("source", &wolf1)
        .with_entity("other", &wolf2);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template"),
        "A wolf and a large wolf arrive."
    );

    // Clear the anaphora memory so the second template evaluates as a fresh encounter!
    ctx1.clear_anaphora();

    // 2. Pronoun Fallback Upgrades
    let t2 = cache
        .get_or_compile("{*A:source:subj} and {*a:other:subj} arrive. {a:other:Subj} [other:howl].")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx1).expect("Failed to render template"),
        "A wolf and a large wolf arrive. The large wolf howls."
    );
}

#[test]
fn test_long_description_disambiguation_collision() {
    let wolf1 = MockEntity {
        id: "mob_2_long".to_string(), // Has long name "large wolf"
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolf2 = MockEntity {
        id: "mob_3_long_collide".to_string(), // Also has long name "large wolf"
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let t1 = cache
        .get_or_compile("{*A:source:subj} and {*a:other:subj} arrive.")
        .expect("Failed to compile template");
    let ctx1 = RenderContext::new("char_1")
        .with_entity("source", &wolf1)
        .with_entity("other", &wolf2);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template"),
        "A wolf and another wolf arrive."
    );
}

#[test]
fn test_long_description_partial_disambiguation() {
    let wolf_scrawny = MockEntity {
        id: "mob_1_scrawny".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolf_large1 = MockEntity {
        id: "mob_2_long".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolf_large2 = MockEntity {
        id: "mob_3_long_collide".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let t1 = cache
        .get_or_compile("{*A:w1:subj} enters. {*A:w2:subj} enters. {*A:w3:subj} enters.")
        .expect("Failed to compile template");
    let ctx1 = RenderContext::new("char_1")
        .with_entity("w1", &wolf_scrawny)
        .with_entity("w2", &wolf_large1)
        .with_entity("w3", &wolf_large2);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template"),
        "A wolf enters. A large wolf enters. Another large wolf enters."
    );
}

#[test]
fn test_long_description_phantom_collision() {
    let jim = MockEntity {
        id: "char_jim".to_string(), // Has the long name "large wolf"
        name: "Jim".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let wolf_scrawny = MockEntity {
        id: "mob_1_scrawny".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolf_large1 = MockEntity {
        id: "mob_2_long".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolf_large2 = MockEntity {
        id: "mob_3_long_collide".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // Jim's hidden long name should NOT prevent the wolves from disambiguating to "large wolf"
    let t1 = cache
        .get_or_compile(
            "{*A:jim:subj} enters. {*A:w1:subj} enters. {*A:w2:subj} enters. {*A:w3:subj} enters.",
        )
        .expect("Failed to compile template");
    let ctx1 = RenderContext::new("char_1")
        .with_entity("jim", &jim)
        .with_entity("w1", &wolf_scrawny)
        .with_entity("w2", &wolf_large1)
        .with_entity("w3", &wolf_large2);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template"),
        "Jim enters. A wolf enters. A large wolf enters. Another large wolf enters."
    );
}

#[test]
fn test_long_description_mutual_exclusion_fallback() {
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: Some("large wolf".into()),
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);
    let t = cache
        .get_or_compile(
            "{*A:w1:subj} and {*a:w2:subj} arrive. {a:w1:Subj} [w1:growl]. {a:w2:Subj} [w2:bark].",
        )
        .unwrap();
    let ctx = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2);

    // Because w2 vacates the "wolf" namespace to become "large wolf", w1 correctly
    // realizes it is unique, resolving its pronoun fallback to "The wolf" rather than "A wolf"!
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "A wolf and a large wolf arrive. The wolf growls. The large wolf barks."
    );
}

#[test]
fn test_long_description_identical_to_unrelated_short() {
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: Some("dire wolf".into()),
        gender: Gender::Neutral,
    };
    let d1 = ConfigurableMockEntity {
        id: "d1".into(),
        name: "dire wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);
    let t = cache
        .get_or_compile("{*A:w1:subj}, {*a:d1:subj}, and {*a:w2:subj} arrive.")
        .unwrap();
    let ctx = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2)
        .with_entity("d1", &d1);

    // w2 tries to use its long name ("dire wolf"). But doing so causes 1 collision (with d1).
    // Its short name ("wolf") also causes 1 collision (with w1).
    // Since the long name does not strictly DECREASE collisions (1 is not less than 1), it stays "wolf".
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "A wolf, a dire wolf, and another wolf arrive."
    );
}

#[test]
fn test_long_description_identical_long_names() {
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: Some("large wolf".into()),
        gender: Gender::Neutral,
    };
    let w3 = ConfigurableMockEntity {
        id: "w3".into(),
        name: "wolf".into(),
        long_name: Some("large wolf".into()),
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);
    let t = cache
        .get_or_compile("{*A:w2:subj} and {*a:w3:subj} arrive.")
        .unwrap();
    let ctx = RenderContext::new("viewer")
        .with_entity("w2", &w2)
        .with_entity("w3", &w3);

    // Both have the same short name (1 collision). Both have the same long name (1 collision).
    // Because 1 is not less than 1, neither uses their long name!
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "A wolf and another wolf arrive."
    );
}

#[test]
fn test_long_description_empty_or_same() {
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: Some("wolf".into()),
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);
    let t = cache
        .get_or_compile("{*A:w1:subj} and {*a:w2:subj} arrive.")
        .unwrap();
    let ctx = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2);

    // w2's long name is exactly the same as its short name. The engine should bypass
    // evaluation entirely and output "another wolf".
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "A wolf and another wolf arrive."
    );
}

#[test]
fn test_long_description_mixed_availability_order_a() {
    // w1 has a long name. w2 does not.
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        long_name: Some("large wolf".into()),
        gender: Gender::Neutral,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);

    // w1 goes FIRST. It doesn't know w2 exists yet, so it uses its short name.
    let t = cache
        .get_or_compile(
            "{*A:w1:subj} and {*a:w2:subj} arrive. {a:w1:Subj} [w1:growl]. {a:w2:Subj} [w2:bark].",
        )
        .expect("Failed to compile template");
    let ctx = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2);

    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).expect("Failed to render template"),
        "A wolf and another wolf arrive. The large wolf growls. The wolf barks."
    );
}

#[test]
fn test_long_description_mixed_availability_order_b() {
    // w1 has a long name. w2 does not.
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        long_name: Some("large wolf".into()),
        gender: Gender::Neutral,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);

    // w2 goes FIRST. When w1 goes second, it sees w2 and upgrades to its long name immediately!
    let t = cache
        .get_or_compile(
            "{*A:w2:subj} and {*a:w1:subj} arrive. {a:w2:Subj} [w2:bark]. {a:w1:Subj} [w1:growl].",
        )
        .expect("Failed to compile template");
    let ctx = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2);

    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).expect("Failed to render template"),
        "A wolf and a large wolf arrive. The wolf barks. The large wolf growls."
    );
}

#[test]
fn test_long_description_lookahead() {
    // w1 has a long name. w2 does not.
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        long_name: Some("large wolf".into()),
        gender: Gender::Neutral,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);

    let t = cache
        .get_or_compile(
            "{*A:w1:subj} and {*a:w2:subj} arrive. {a:w1:Subj} [w1:growl]. {a:w2:Subj} [w2:bark].",
        )
        .expect("Failed to compile template");

    // Without lookahead (left-to-right causal pop-in)
    let ctx_default = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2);
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx_default).expect("Failed to render template"),
        "A wolf and another wolf arrive. The large wolf growls. The wolf barks."
    );

    // With lookahead: w1 realizes w2 is coming and will cause a collision.
    // It immediately preempts the ambiguity and uses its long name on the very first mention!
    let ctx_lookahead = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2)
        .with_lookahead(true);
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx_lookahead).expect("Failed to render template"),
        "A large wolf and a wolf arrive. The large wolf growls. The wolf barks."
    );
}

#[test]
fn test_indefinite_article_extended_occurrences() {
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };
    let w3 = ConfigurableMockEntity {
        id: "w3".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };
    let w4 = ConfigurableMockEntity {
        id: "w4".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);
    let t = cache
        .get_or_compile(
            "{*A:w1:subj} enters. {*A:w2:subj} enters. {*A:w3:subj} enters. {*A:w4:subj} enters.",
        )
        .expect("Failed to compile template");
    let ctx = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2)
        .with_entity("w3", &w3)
        .with_entity("w4", &w4);

    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).expect("Failed to render template"),
        "A wolf enters. Another wolf enters. A third wolf enters. A fourth wolf enters."
    );
}

#[test]
fn test_ordinals_and_resets() {
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2);

    // First encounter
    let t1 = cache
        .get_or_compile("{*A:w1:subj} walks in. {*A:w2:subj} walks in. {*The:w1:subj} howls. {*The:w2:subj} grins.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "A wolf walks in. Another wolf walks in. The first wolf howls. The second wolf grins."
    );

    // Forget w2. Now only w1 is in the scene. The engine gracefully drops the ordinals for the lone entity!
    ctx.forget_anaphora("w2");

    let t2 = cache.get_or_compile("{*The:w1:subj} sighs.").unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).unwrap(),
        "The wolf sighs."
    );

    // Now add w2 back. W1 gets re-assigned to "1" and W2 gets "2".
    // We also test the pronoun fallback ordinal synergy!
    let t3 = cache
        .get_or_compile(
            "{*A:w2:subj} returns. {a:w1:Subj} [w1:growl] at {*the:w2:obj}. {a:w2:Subj} [w2:flee].",
        )
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx).expect("Failed to render template"),
        "Another wolf returns. The first wolf growls at the second wolf. The second wolf flees."
    );
}

#[test]
fn test_lookahead_prevents_silent_bob() {
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        long_name: Some("large wolf".into()),
        gender: Gender::Neutral,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);

    // We bind BOTH w1 and w2 to the context, and enable lookahead.
    let ctx = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2)
        .with_lookahead(true);

    // However, the template ONLY mentions w1.
    let t = cache.get_or_compile("{*A:w1:subj} howls.").unwrap();

    // If the lookahead blindly evaluated the entire context map, it would panic about the invisible w2
    // and inappropriately force w1 to use its long name ("A large wolf howls.").
    // By scoping strictly to the AST Pre-Pass, it safely ignores w2!
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "A wolf howls."
    );
}

#[test]
fn test_article_upgrades_for_plural_viewers() {
    let wolves = MockEntity {
        id: "pack_1".to_string(),
        name: "wolves".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("pack_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &wolves);

    // As the active viewer, the engine should completely skip tracking occurrence permutations
    // for 'A', 'Some', and 'One of the' and simply inject the viewer pronoun "We".
    let t1 = cache
        .get_or_compile("{*A:source:subj} [source:howl].")
        .expect("Failed to compile template");
    let t2 = cache
        .get_or_compile("{*Some:source:subj} [source:howl].")
        .expect("Failed to compile template");
    let t3 = cache
        .get_or_compile("{*One of the:source:subj} [source:howl].")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "We howl."
    );
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template"),
        "We howl."
    );
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx).expect("Failed to render template"),
        "We howl."
    );

    // But if the singular override is attached, it should accurately treat the pack as an individual "I"!
    let t4 = cache
        .get_or_compile("{-a:source:subj} [-source:howl].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t4, &ctx).expect("Failed to render template"),
        "I howl."
    );
}

#[test]
fn test_plural_ordinals_and_demonstratives() {
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolves".into(),
        long_name: None,
        gender: Gender::Plural,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolves".into(),
        long_name: None,
        gender: Gender::Plural,
    };
    let w3 = ConfigurableMockEntity {
        id: "w3".into(),
        name: "wolves".into(),
        long_name: None,
        gender: Gender::Plural,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2)
        .with_entity("w3", &w3);

    // 1. Plural Indefinite Upgrades (Some -> A second set -> A third set)
    let t1 = cache
        .get_or_compile("{*A:w1:subj} arrive. {*A:w2:subj} arrive. {*A:w3:subj} arrive.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "Some wolves arrive. A second set of wolves arrive. A third set of wolves arrive."
    );

    // 2. Plural Demonstratives (This first set, That second set)
    let t2 = cache
        .get_or_compile("{*This:w1:subj} howl. {*That:w2:subj} howl.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template"),
        "This first set of wolves howl. That second set of wolves howl."
    );

    // 3. "One of the" and "Some" explicitly preserving ordinals
    let t3 = cache
        .get_or_compile("{*One of the:w1:subj} howls. {*Some:w2:subj} howl.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx).expect("Failed to render template"),
        "One of the first set of wolves howls. A second set of wolves howl."
    );
}

#[test]
fn test_plural_ordinals_with_collective_noun() {
    struct Pack<'a> {
        name: &'a str,
        collective: &'a str,
    }

    impl TemplateEntity for Pack<'_> {
        fn contains_viewer(&self, _: &str) -> bool {
            false
        }
        fn gender(&self) -> Gender {
            Gender::Plural
        }
        fn is_plural(&self) -> bool {
            true
        }
        fn is_proper_noun_for(&self, _: &str) -> bool {
            false
        }
        fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
            Cow::Borrowed(self.name)
        }
        fn collective_noun(&self) -> Option<&str> {
            Some(self.collective)
        }
    }

    let p1 = Pack {
        name: "wolves",
        collective: "pack",
    };
    let p2 = Pack {
        name: "wolves",
        collective: "pack",
    };
    let p3 = Pack {
        name: "wolves",
        collective: "pack",
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("p1", &p1)
        .with_entity("p2", &p2)
        .with_entity("p3", &p3);

    let t1 = cache
        .get_or_compile("{*A:p1:subj} arrive. {*A:p2:subj} arrive. {*A:p3:subj} arrive.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "Some wolves arrive. A second pack of wolves arrive. A third pack of wolves arrive."
    );
}

#[test]
fn test_no_smart_modifier_bypasses_ordinals() {
    let w1 = ConfigurableMockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };
    let w2 = ConfigurableMockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };
    let w3 = ConfigurableMockEntity {
        id: "w3".into(),
        name: "wolf".into(),
        long_name: None,
        gender: Gender::Neutral,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2)
        .with_entity("w3", &w3);

    // Normally this would evaluate to "A wolf, another wolf, and a third wolf."
    // The `!` prefix completely disables smart anaphora, bypassing collision tracking entirely.
    let t1 = cache
        .get_or_compile("{*!A:w1:subj}, {*!another:w2:subj}, and {*!a:w3:subj} arrive.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "A wolf, another wolf, and a wolf arrive."
    );

    let t2 = cache
        .get_or_compile("{*!The:w1:subj} howls.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template"),
        "The wolf howls."
    );
}
