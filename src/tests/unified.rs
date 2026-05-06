use super::common::{ConfigurableMockEntity, MockEntity};
use crate::cache::TemplateCache;
use crate::engine::PerspectiveEngine;
use crate::models::{Gender, GroupEntity, RenderContext, TemplateEntity};
use serial_test::serial;
use std::borrow::Cow;

#[test]
fn test_generic_unified_tag() {
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

    // We explicitly specify the fallback article to be "the" instead of the default "a"!
    let template = cache
        .get_or_compile("{The:target:obj} [target:approach].")
        .unwrap();

    // 1. Unseen NPC -> It's the object, but hasn't been seen, so it falls back to a noun.
    let ctx1 = RenderContext::new("char_1").with_entity("target", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx1).unwrap(),
        "The goblin approaches."
    );

    // 2. Active Viewer -> Safely bypasses the fallback and outputs the pronoun.
    let ctx2 = RenderContext::new("char_1").with_entity("target", &player);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx2).unwrap(),
        "You approach."
    );
}

// --- UNIFIED TAG EQUIVALENT TESTS ---
// The following tests replicate the core behavior of the engine using ONLY unified tags
// ({article:key:case}), demonstrating that the unified syntax replaces both standard
// noun tags ({key}) and pronoun tags ({key:case}).

#[test]
fn test_actor_vs_director_stance_unified() {
    let aldran = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{a:source:subj} [source:be] looking around for {the:source:poss} sword.")
        .unwrap();

    let ctx_actor = RenderContext::new("char_1").with_entity("source", &aldran);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_actor).unwrap(),
        "You are looking around for your sword."
    );

    let ctx_director = RenderContext::new("char_2").with_entity("source", &aldran);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_director).unwrap(),
        "Aldran is looking around for his sword."
    );
}

#[test]
fn test_epistemological_masking_and_articles_unified() {
    let aldran = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{A:source:subj} [source:approach].")
        .unwrap();

    let ctx_stranger = RenderContext::new("stranger_1").with_entity("source", &aldran);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_stranger).unwrap(),
        "A tall man approaches."
    );
}

#[test]
fn test_article_suppression_unified() {
    let aldran = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let avengers = MockEntity {
        id: "mob_2".into(),
        name: "the Avengers".into(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: true,
    };
    let wolves = MockEntity {
        id: "mob_3".into(),
        name: "wolves".into(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let t1 = cache
        .get_or_compile("{The:source:subj} [source:be] here.")
        .unwrap();
    assert_eq!(
        render_msg!("char_2", &t1, "source" => &goblin).unwrap(),
        "The goblin is here."
    );
    assert_eq!(
        render_msg!("char_2", &t1, "source" => &aldran).unwrap(),
        "Aldran is here."
    );
    assert_eq!(
        render_msg!("char_1", &t1, "source" => &aldran).unwrap(),
        "You are here."
    );

    let t2 = cache
        .get_or_compile("{A:source:subj} [source:assemble]!")
        .unwrap();
    assert_eq!(
        render_msg!("char_2", &t2, "source" => &avengers).unwrap(),
        "The Avengers assemble!"
    );

    let t3 = cache
        .get_or_compile("{A:source:subj} [source:howl].")
        .unwrap();
    assert_eq!(
        render_msg!("char_2", &t3, "source" => &wolves).unwrap(),
        "Some wolves howl."
    );
}

#[test]
fn test_disguised_plural_proper_nouns_unified() {
    let avengers = MockEntity {
        id: "mob_2".into(),
        name: "the Avengers".into(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: true,
    };
    let cache = TemplateCache::new(100);

    let template_a = cache
        .get_or_compile("{A:source:subj} [source:arrive].")
        .unwrap();
    let template_the = cache
        .get_or_compile("{The:source:subj} [source:arrive].")
        .unwrap();

    assert_eq!(
        render_msg!("char_2", &template_a, "source" => &avengers).unwrap(),
        "The Avengers arrive."
    );
    assert_eq!(
        render_msg!("char_2", &template_the, "source" => &avengers).unwrap(),
        "The Avengers arrive."
    );

    assert_eq!(
        render_msg!("stranger_1", &template_a, "source" => &avengers).unwrap(),
        "Some masked heroes arrive."
    );
    assert_eq!(
        render_msg!("stranger_1", &template_the, "source" => &avengers).unwrap(),
        "The masked heroes arrive."
    );
}

#[test]
fn test_plurality_and_verb_binding_unified() {
    let wolves = MockEntity {
        id: "mob_1".into(),
        name: "pack of wolves".into(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };
    let player = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let cache = TemplateCache::new(100);

    let template = cache
        .get_or_compile("{The:target:subj} [target:watch] as {the:source:subj} [source:attack]!")
        .unwrap();
    let ctx = RenderContext::new("char_2")
        .with_entity("source", &wolves)
        .with_entity("target", &player);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).unwrap(),
        "Aldran watches as the pack of wolves attack!"
    );
}

#[test]
fn test_group_entity_perspectives_unified() {
    let player = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let ally = MockEntity {
        id: "char_2".into(),
        name: "Bob".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let enemy = MockEntity {
        id: "mob_1".into(),
        name: "Goblin".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: false,
    };
    let stranger = MockEntity {
        id: "char_3".into(),
        name: "Charlie".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let party = GroupEntity {
        members: vec![&player, &ally],
    };
    let big_party = GroupEntity {
        members: vec![&player, &ally, &stranger],
    };

    let cache = TemplateCache::new(100);

    let t_action = cache
        .get_or_compile("{A:source:subj} [source:open] the door.")
        .unwrap();
    assert_eq!(
        render_msg!("char_1", &t_action, "source" => &party).unwrap(),
        "You and Bob open the door."
    );
    assert_eq!(
        render_msg!("char_3", &t_action, "source" => &party).unwrap(),
        "Aldran and Bob open the door."
    );
    assert_eq!(
        render_msg!("mob_1", &t_action, "source" => &big_party).unwrap(),
        "Aldran, Bob, and Charlie open the door."
    );

    let t_pronoun = cache
        .get_or_compile("{The:source:subj} [source:attack] {a:target:obj}!")
        .unwrap();
    assert_eq!(
        render_msg!("char_1", &t_pronoun, "source" => &enemy, "target" => &party).unwrap(),
        "The Goblin attacks you and Bob!"
    );
    assert_eq!(
        render_msg!("char_3", &t_pronoun, "source" => &enemy, "target" => &party).unwrap(),
        "The Goblin attacks Aldran and Bob!"
    );
}

#[test]
fn test_modal_verbs_perspectives_unified() {
    let player = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "Goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let cache = TemplateCache::new(100);

    let t_must = cache
        .get_or_compile("{A:source:subj} [source:must] flee from {the:target:obj}!")
        .unwrap();
    assert_eq!(
        render_msg!("char_1", &t_must, "source" => &player, "target" => &goblin).unwrap(),
        "You must flee from the Goblin!"
    );
    assert_eq!(
        render_msg!("char_3", &t_must, "source" => &player, "target" => &goblin).unwrap(),
        "Aldran must flee from the Goblin!"
    );
}

#[test]
fn test_force_director_stance_unified() {
    let player = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let cache = TemplateCache::new(100);

    let t_forced = cache
        .get_or_compile(
            "{a:+source:subj} [+source:attack] {the:target:obj} with {the:+source:poss} sword.",
        )
        .unwrap();
    assert_eq!(
        render_msg!("char_1", &t_forced, "source" => &player, "target" => &goblin).unwrap(),
        "Aldran attacks the goblin with his sword."
    );
}

#[test]
fn test_anaphora_resolution_unified() {
    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let slime = MockEntity {
        id: "mob_2".into(),
        name: "slime".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_2")
        .with_entity("target", &goblin)
        .with_entity("other", &slime);

    let t1 = cache
        .get_or_compile("{a:target:Subj} [target:look] around.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "A goblin looks around."
    );

    let t2 = cache
        .get_or_compile("{A:target:Subj} [target:attack]!")
        .unwrap();
    assert_eq!(PerspectiveEngine::render(&t2, &ctx).unwrap(), "It attacks!");

    ctx.clear_anaphora();
    let t4 = cache
        .get_or_compile(
            "{*The:target:subj} enters. {*The:other:subj} blinks. {a:target:Subj} [target:scream].",
        )
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t4, &ctx).unwrap(),
        "The goblin enters. The slime blinks. The goblin screams."
    );
}

#[test]
fn test_anaphora_ambiguity_resolution_unified() {
    let bob = MockEntity {
        id: "char_2".into(),
        name: "Bob".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let aldran = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("bob", &bob)
        .with_entity("aldran", &aldran)
        .with_entity("goblin", &goblin);

    let t1 = cache
        .get_or_compile(
            "{*The:goblin:subj} [goblin:hit] {*a:aldran:obj}. {a:aldran:Subj} [aldran:smile].",
        )
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "The goblin hits Aldran. He smiles."
    );

    ctx.clear_anaphora();
    let t2 = cache
        .get_or_compile("{*A:bob:subj} [bob:hit] {*a:aldran:obj}. {a:aldran:Subj} [aldran:smile].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).unwrap(),
        "Bob hits Aldran. Aldran smiles."
    );
}

#[test]
fn test_definite_description_upgrade_unified() {
    let wolf1 = MockEntity {
        id: "mob_1".into(),
        name: "wolf".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let slime = MockEntity {
        id: "mob_2".into(),
        name: "slime".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let cache = TemplateCache::new(100);

    // We introduce ambiguity with `other` so `source` falls back to its noun form on the second mention.
    let template = cache
        .get_or_compile(
            "{A:source:subj} and {a:other:subj} walk in. {A:source:subj} [source:howl].",
        )
        .unwrap();
    let ctx = RenderContext::new("char_1")
        .with_entity("source", &wolf1)
        .with_entity("other", &slime);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).unwrap(),
        "A wolf and a slime walk in. The wolf howls."
    );
}

#[test]
fn test_definite_description_upgrade_collision_unified() {
    let wolf1 = MockEntity {
        id: "mob_1".into(),
        name: "wolf".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolf2 = MockEntity {
        id: "mob_2".into(),
        name: "wolf".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let cache = TemplateCache::new(100);

    let template = cache.get_or_compile("{A:source:subj} [source:walk] in. {A:other:subj} [other:walk] in. {A:source:subj} [source:howl].").unwrap();
    let ctx = RenderContext::new("char_1")
        .with_entity("source", &wolf1)
        .with_entity("other", &wolf2);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).unwrap(),
        "A wolf walks in. Another wolf walks in. The first wolf howls."
    );
}

#[test]
fn test_suppress_anaphora_upgrades_unified() {
    let wolf1 = MockEntity {
        id: "mob_1".into(),
        name: "wolf".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let slime = MockEntity {
        id: "mob_2".into(),
        name: "slime".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let cache = TemplateCache::new(100);

    let template = cache
        .get_or_compile(
            "{A:source:subj} and {a:other:subj} walk in. {*!A:source:subj} [source:howl].",
        )
        .unwrap();
    let ctx = RenderContext::new("char_1")
        .with_entity("source", &wolf1)
        .with_entity("other", &slime);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).unwrap(),
        "A wolf and a slime walk in. A wolf howls."
    );
}

#[test]
fn test_definite_description_upgrade_with_possessives_unified() {
    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let cache = TemplateCache::new(100);

    let t1 = cache
        .get_or_compile(
            "{A:source's:poss} sword [source:fall]. {*A:source's:poss} shield [source:break].",
        )
        .unwrap();
    let ctx = RenderContext::new("char_1").with_entity("source", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "A goblin's sword falls. The goblin's shield breaks."
    );
}

#[test]
fn test_singular_overrides_unified() {
    let orcs = MockEntity {
        id: "mob_1".into(),
        name: "orcs".into(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };
    let goblin = MockEntity {
        id: "mob_2".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let cache = TemplateCache::new(100);

    let t1 = cache
        .get_or_compile(
            "{*One of the:-orcs:subj} [-orcs:bellow], and {:-orcs:subj} [-orcs:charge]!",
        )
        .unwrap();
    let ctx1 = RenderContext::new("viewer").with_entity("orcs", &orcs);
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).unwrap(),
        "One of the orcs bellows, and it charges!"
    );

    let t2 = cache
        .get_or_compile(
            "{*One of the:-orcs:subj} and {*a:goblin:subj} arrive. {:-orcs:Subj} [-orcs:bellow].",
        )
        .unwrap();
    let ctx2 = RenderContext::new("viewer")
        .with_entity("orcs", &orcs)
        .with_entity("goblin", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx2).unwrap(),
        "One of the orcs and a goblin arrive. One of the orcs bellows."
    );
}

#[test]
fn test_ordinals_and_resets_unified() {
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

    let t1 = cache.get_or_compile("{A:w1:subj} walks in. {A:w2:subj} walks in. {The:w1:subj} [w1:howl]. {The:w2:subj} [w2:grin].").unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "A wolf walks in. Another wolf walks in. The first wolf howls. The second wolf grins."
    );

    ctx.forget_anaphora("w2");
    let t2 = cache.get_or_compile("{*The:w1:subj} [w1:sigh].").unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).unwrap(),
        "The wolf sighs."
    );

    let t3 = cache.get_or_compile("{*A:w2:subj} [w2:return]. {a:w1:Subj} [w1:growl] at {*the:w2:obj}. {a:w2:Subj} [w2:flee].").unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx).unwrap(),
        "Another wolf returns. The first wolf growls at the second wolf. The second wolf flees."
    );
}

#[test]
fn test_extract_group_member_override() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let ally = MockEntity {
        id: "char_2".to_string(),
        name: "Bob".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let ally2 = MockEntity {
        id: "char_3".to_string(),
        name: "Charlie".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let party = GroupEntity {
        members: vec![&player, &ally],
    };
    let big_party = GroupEntity {
        members: vec![&player, &ally, &ally2],
    };

    let cache = TemplateCache::new(100);

    let ctx = RenderContext::new("char_1")
        .with_entity("party", &party)
        .with_entity("big_party", &big_party);

    // 1. Without override: Uses the standard multi-member formatting
    let t1 = cache
        .get_or_compile("{party:Subj} [party:arrive].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "You and Bob arrive."
    );

    ctx.clear_anaphora();

    // 2. With override: Extracts one member generically using "or"
    let t2 = cache
        .get_or_compile("{^party:Subj} [^party:arrive].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).unwrap(),
        "You or Bob arrives."
    );

    ctx.clear_anaphora();

    // 3. Pronoun evaluation for the extracted member.
    // Because the member was forcibly extracted, it abandons the "You" mapping.
    // Natively, it evaluates the shared gender of the group (Male + Male = Male -> "He").
    let t3 = cache
        .get_or_compile("{^party} enters. {a:^party:Subj} [^party:smile].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx).unwrap(),
        "You or Bob enters. He smiles."
    );

    ctx.clear_anaphora();

    // 4. Large groups with Oxford comma "or" logic
    let t4 = cache
        .get_or_compile("{^big_party:Subj} [^big_party:arrive].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t4, &ctx).unwrap(),
        "You, Bob, or Charlie arrives."
    );

    // 5. Using the modifier on a non-group entity safely has no effect
    let t5 = cache
        .get_or_compile("{^player:Subj} [^player:smile].")
        .unwrap();
    let ctx2 = RenderContext::new("char_1").with_entity("player", &player);
    assert_eq!(PerspectiveEngine::render(&t5, &ctx2).unwrap(), "You smile.");
}

#[test]
fn test_ambiguous_plural_you_override() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let ally = MockEntity {
        id: "char_2".to_string(),
        name: "Bob".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let party = GroupEntity {
        members: vec![&player, &ally],
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_1").with_entity("party", &party);

    // With the ~ override, the engine permits "You" to refer to the whole party even though it's ambiguous
    let t1 = cache
        .get_or_compile("{~party:Subj} [~party:attack].")
        .unwrap();
    assert_eq!(PerspectiveEngine::render(&t1, &ctx).unwrap(), "You attack.");
}

// -------------------------------------------------------------------------
// --- COMPREHENSIVE UNIFIED TAG COVERAGE ---
// The following tests strictly utilize unified 3-part tags (e.g. {A:key:subj})
// and safe overrides to prove 1:1 feature parity with standard noun/pronoun tags.
// -------------------------------------------------------------------------

#[test]
fn test_e2e_combat_round_unified() {
    struct Weapon {
        name: &'static str,
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
            Cow::Borrowed(self.name)
        }
    }
    struct Combatant {
        id: &'static str,
        name: &'static str,
        gender: Gender,
        is_proper: bool,
        weapon: Option<Weapon>,
    }
    impl TemplateEntity for Combatant {
        fn contains_viewer(&self, vid: &str) -> bool {
            self.id == vid
        }
        fn gender(&self) -> Gender {
            self.gender
        }
        fn is_plural(&self) -> bool {
            false
        }
        fn is_proper_noun_for(&self, _: &str) -> bool {
            self.is_proper
        }
        fn display_name_for<'a>(&'a self, vid: &str) -> Cow<'a, str> {
            if self.contains_viewer(vid) {
                Cow::Borrowed("you")
            } else {
                Cow::Borrowed(self.name)
            }
        }
        fn get_property(&self, prop: &str) -> Option<&dyn TemplateEntity> {
            if prop == "weapon" {
                self.weapon.as_ref().map(|w| w as &dyn TemplateEntity)
            } else {
                None
            }
        }
    }

    let player = Combatant {
        id: "char_1",
        name: "Aldran",
        gender: Gender::Male,
        is_proper: true,
        weapon: Some(Weapon {
            name: "glowing sword",
        }),
    };
    let goblin1 = Combatant {
        id: "mob_1",
        name: "goblin",
        gender: Gender::Neutral,
        is_proper: false,
        weapon: Some(Weapon {
            name: "rusty dagger",
        }),
    };
    let goblin2 = Combatant {
        id: "mob_2",
        name: "goblin",
        gender: Gender::Neutral,
        is_proper: false,
        weapon: Some(Weapon {
            name: "wooden club",
        }),
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_1")
        .with_entity("player", &player)
        .with_entity("g1", &goblin1)
        .with_entity("g2", &goblin2);

    // {A:g1} and {a:g2} ambush {player}!
    let t_intro = cache
        .get_or_compile("{*A:g1:subj} and {*a:g2:subj} ambush {*a:player:obj}!")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_intro, &ctx).unwrap(),
        "A goblin and another goblin ambush you!"
    );

    // {player} [player:slash] {the:g1} with {player:poss} {player.weapon}.
    let t_attack = cache
        .get_or_compile(
            "{*A:player:subj} [player:slash] {*the:g1:obj} with {player's player.weapon:obj}.",
        )
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_attack, &ctx).unwrap(),
        "You slash the first goblin with your glowing sword."
    );

    // {The:g2} [g2:swing] {g2:poss} {g2.weapon} at {player:obj}!
    let t_retaliate = cache
        .get_or_compile("{*The:g2:subj} [g2:swing] {g2's g2.weapon:obj} at {a:player:obj}!")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_retaliate, &ctx).unwrap(),
        "The second goblin swings its wooden club at you!"
    );
}

#[test]
fn test_dot_notation_resolution_unified() {
    struct Weapon {
        name: &'static str,
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
            Cow::Borrowed(self.name)
        }
    }
    struct Actor {
        name: &'static str,
        weapon: Weapon,
    }
    impl TemplateEntity for Actor {
        fn contains_viewer(&self, vid: &str) -> bool {
            vid == "char_1"
        }
        fn gender(&self) -> Gender {
            Gender::Male
        }
        fn is_plural(&self) -> bool {
            false
        }
        fn is_proper_noun_for(&self, _: &str) -> bool {
            true
        }
        fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
            Cow::Borrowed(self.name)
        }
        fn get_property(&self, prop: &str) -> Option<&dyn TemplateEntity> {
            if prop == "weapon" {
                Some(&self.weapon)
            } else {
                None
            }
        }
    }

    let player = Actor {
        name: "Aldran",
        weapon: Weapon {
            name: "rusty sword",
        },
    };
    let cache = TemplateCache::new(100);

    // {Source} [source:draw] {a:source.weapon} and [source:swing] {source:poss} {source.weapon}!
    let t = cache.get_or_compile("{*A:Source:subj} [source:draw] {*a:source.weapon:obj} and [source:swing] {source's source.weapon:obj}!").unwrap();
    let ctx = RenderContext::new("char_2").with_entity("source", &player);
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "Aldran draws a rusty sword and swings it!"
    );
}

#[test]
fn test_deeply_nested_properties_unified() {
    struct Node {
        name: String,
        child: Option<Box<Node>>,
    }
    impl TemplateEntity for Node {
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
        fn get_property(&self, prop: &str) -> Option<&dyn TemplateEntity> {
            if prop == "child" {
                self.child.as_deref().map(|c| c as &dyn TemplateEntity)
            } else {
                None
            }
        }
    }

    let tree = Node {
        name: "root".into(),
        child: Some(Box::new(Node {
            name: "branch".into(),
            child: Some(Box::new(Node {
                name: "leaf".into(),
                child: None,
            })),
        })),
    };
    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer").with_entity("tree", &tree);

    // You look at {the:tree.child.child}.
    let t = cache
        .get_or_compile("You look at {*the:tree.child.child:obj}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "You look at the leaf."
    );
}

#[test]
fn test_nested_properties_returning_group_entities_unified() {
    let g1 = MockEntity {
        id: "m1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let g2 = MockEntity {
        id: "m2".into(),
        name: "slime".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    struct Boss<'a> {
        name: String,
        minions: GroupEntity<'a>,
    }
    impl TemplateEntity for Boss<'_> {
        fn contains_viewer(&self, _: &str) -> bool {
            false
        }
        fn gender(&self) -> Gender {
            Gender::Male
        }
        fn is_plural(&self) -> bool {
            false
        }
        fn is_proper_noun_for(&self, _: &str) -> bool {
            false
        }
        fn display_name_for<'b>(&'b self, _: &str) -> Cow<'b, str> {
            Cow::Borrowed(&self.name)
        }
        fn get_property(&self, prop: &str) -> Option<&dyn TemplateEntity> {
            if prop == "minions" {
                Some(&self.minions)
            } else {
                None
            }
        }
    }
    let boss = Boss {
        name: "boss".into(),
        minions: GroupEntity::new(vec![&g1, &g2]),
    };
    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer").with_entity("boss", &boss);

    // {The:boss.minions} [boss.minions:attack]!
    let t = cache
        .get_or_compile("{*the:boss.minions:subj} [boss.minions:attack]!")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "The goblin and the slime attack!"
    );
}

#[test]
fn test_nested_properties_returning_proper_nouns_unified() {
    struct Excalibur {
        name: String,
    }
    impl TemplateEntity for Excalibur {
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
            true
        }
        fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
            Cow::Borrowed(&self.name)
        }
    }
    struct King {
        name: String,
        weapon: Excalibur,
    }
    impl TemplateEntity for King {
        fn contains_viewer(&self, _: &str) -> bool {
            false
        }
        fn gender(&self) -> Gender {
            Gender::Male
        }
        fn is_plural(&self) -> bool {
            false
        }
        fn is_proper_noun_for(&self, _: &str) -> bool {
            true
        }
        fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
            Cow::Borrowed(&self.name)
        }
        fn get_property(&self, prop: &str) -> Option<&dyn TemplateEntity> {
            if prop == "weapon" {
                Some(&self.weapon)
            } else {
                None
            }
        }
    }
    let arthur = King {
        name: "Arthur".into(),
        weapon: Excalibur {
            name: "Excalibur".into(),
        },
    };
    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer").with_entity("source", &arthur);

    // {A:source} draws {a:source.weapon}.
    let t = cache
        .get_or_compile("{*A:source:subj} draws {*a:source.weapon:obj}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "Arthur draws Excalibur."
    );
}

#[test]
fn test_plural_ordinals_with_collective_noun_unified() {
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
    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("p1", &p1)
        .with_entity("p2", &p2);

    // {A:p1} arrive. {A:p2} arrive.
    let t = cache
        .get_or_compile("{*A:p1:subj} arrive. {*A:p2:subj} arrive.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).unwrap(),
        "Some wolves arrive. A second pack of wolves arrive."
    );
}

#[test]
fn test_unified_anaphora_equivalents() {
    let cache = TemplateCache::new(100);
    let aldran = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let bob = MockEntity {
        id: "char_2".into(),
        name: "Bob".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let jill = MockEntity {
        id: "char_4".into(),
        name: "Jill".into(),
        gender: Gender::Female,
        is_plural: false,
        is_proper_noun: true,
    };
    let tom = MockEntity {
        id: "m2".into(),
        name: "Tom".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let dan = MockEntity {
        id: "m4".into(),
        name: "Dan".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    // standalone_verb_anaphora_tracking
    let ctx_track = RenderContext::new("viewer")
        .with_entity("bob", &bob)
        .with_entity("aldran", &aldran);
    let t_track = cache
        .get_or_compile(
            "{*A:bob:subj} [bob:attack] {*A:aldran:obj}. {A:aldran:Subj} [aldran:fall].",
        )
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_track, &ctx_track).unwrap(),
        "Bob attacks Aldran. Aldran falls."
    );

    let ctx_track2 = RenderContext::new("viewer")
        .with_entity("jill", &jill)
        .with_entity("aldran", &aldran);
    let t_track2 = cache
        .get_or_compile(
            "{*A:jill:subj} [jill:attack] {*A:aldran:obj}. {A:aldran:Subj} [aldran:fall].",
        )
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_track2, &ctx_track2).unwrap(),
        "Jill attacks Aldran. He falls."
    );

    // anaphora_fallback_capitalization
    let ctx_cap = RenderContext::new("viewer").with_entity("target", &goblin);
    let t_cap = cache
        .get_or_compile("{a:target:Subj} [target:hiss].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_cap, &ctx_cap).unwrap(),
        "A goblin hisses."
    );

    // anaphora_across_contexts
    let ctx_across1 = RenderContext::new("char_2").with_entity("target", &goblin);
    let t_across1 = cache.get_or_compile("{*the:target:subj} enters.").unwrap();
    let _ = PerspectiveEngine::render(&t_across1, &ctx_across1).unwrap();

    let ctx_across2 = RenderContext::new("char_2")
        .with_entity("target", &goblin)
        .with_anaphora(ctx_across1.extract_anaphora());
    let t_across2 = cache
        .get_or_compile("{A:target:Subj} [target:look] around.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_across2, &ctx_across2).unwrap(),
        "It looks around."
    );

    // anaphora_state_preserves_ambiguity
    let ctx_ambig = RenderContext::new("viewer")
        .with_entity("aldran", &aldran)
        .with_entity("bob", &bob);
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:bob:subj} is standing next to {*A:aldran:obj}.")
            .unwrap(),
        &ctx_ambig,
    )
    .unwrap();
    let ctx_ambig2 = RenderContext::new("viewer")
        .with_entity("aldran", &aldran)
        .with_entity("bob", &bob)
        .with_anaphora(ctx_ambig.extract_anaphora());
    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{A:aldran:Subj} [aldran:wave].")
                .unwrap(),
            &ctx_ambig2
        )
        .unwrap(),
        "Aldran waves."
    );

    // anaphora_memory_limit
    let ctx_mem = RenderContext::new("viewer")
        .with_anaphora_limit(2)
        .with_entity("aldran", &aldran)
        .with_entity("bob", &bob)
        .with_entity("goblin", &goblin);
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:aldran:subj} [aldran:wave] at {*A:bob:obj}.")
            .unwrap(),
        &ctx_mem,
    )
    .unwrap();
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*the:goblin:subj} [goblin:approach].")
            .unwrap(),
        &ctx_mem,
    )
    .unwrap();
    assert_eq!(
        PerspectiveEngine::render(
            &cache.get_or_compile("{A:bob:Subj} [bob:smile].").unwrap(),
            &ctx_mem
        )
        .unwrap(),
        "He smiles."
    );
    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{A:aldran:Subj} [aldran:sigh].")
                .unwrap(),
            &ctx_mem
        )
        .unwrap(),
        "Aldran sighs."
    );

    // pinned_and_forgotten_anaphora
    let ctx_pin = RenderContext::new("viewer")
        .with_anaphora_limit(2)
        .with_entity("bob", &bob)
        .with_entity("tom", &tom)
        .with_entity("dan", &dan)
        .with_pinned_entity("bob");
    let _ = PerspectiveEngine::render(
        &cache.get_or_compile("{*A:tom:subj} arrives.").unwrap(),
        &ctx_pin,
    )
    .unwrap();
    let _ = PerspectiveEngine::render(
        &cache.get_or_compile("{*A:dan:subj} arrives.").unwrap(),
        &ctx_pin,
    )
    .unwrap();
    assert_eq!(
        PerspectiveEngine::render(
            &cache.get_or_compile("{A:bob:Subj} [bob:smile].").unwrap(),
            &ctx_pin
        )
        .unwrap(),
        "Bob smiles."
    );
    ctx_pin.forget_anaphora("dan");
    assert_eq!(
        PerspectiveEngine::render(
            &cache.get_or_compile("{A:bob:Subj} [bob:wave].").unwrap(),
            &ctx_pin
        )
        .unwrap(),
        "He waves."
    );

    // all_pinned_entities_exceed_limit
    let ctx_exceed = RenderContext::new("viewer")
        .with_anaphora_limit(2)
        .with_entity("bob", &bob)
        .with_entity("tom", &tom)
        .with_pinned_entity("bob")
        .with_pinned_entity("tom")
        .with_entity("dan", &dan);
    let _ = PerspectiveEngine::render(
        &cache.get_or_compile("{*A:dan:subj} arrives.").unwrap(),
        &ctx_exceed,
    )
    .unwrap();
    assert_eq!(ctx_exceed.recent_entities.borrow().len(), 2);

    // anaphora_viewer_exemption
    let t_exempt = cache.get_or_compile("{*A:Source:subj} [source:hit] {*the:target:obj}, then {a:source:subj} [source:step] back.").unwrap();
    assert_eq!(
        render_msg!("char_3", &t_exempt, "source" => &aldran, "target" => &goblin).unwrap(),
        "Aldran hits the goblin, then he steps back."
    );
    assert_eq!(
        render_msg!("char_1", &t_exempt, "source" => &aldran, "target" => &goblin).unwrap(),
        "You hit the goblin, then you step back."
    );

    // first_person_objective_anaphora_fallback
    let t_obj = cache
        .get_or_compile("The trap [strike] {a:target:obj}!")
        .unwrap();
    let ctx_obj1 = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("target", &aldran);
    assert_eq!(
        PerspectiveEngine::render(&t_obj, &ctx_obj1).unwrap(),
        "The trap strikes me!"
    );
    let ctx_obj2 = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("target", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&t_obj, &ctx_obj2).unwrap(),
        "The trap strikes a goblin!"
    );
}

#[test]
fn test_unified_stance_tense_equivalents() {
    let cache = TemplateCache::new(100);
    let aldran = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    // actor_stances
    let t_stance = cache
        .get_or_compile("{*A:source:subj} [source:walk] forward.")
        .unwrap();
    assert_eq!(
        render_msg!("char_1", &t_stance, "source" => &aldran).unwrap(),
        "You walk forward."
    );

    // first_person_conjugation_and_pronouns
    let t_first = cache
        .get_or_compile("{A:source:subj} [source:be] looking for {a:source:poss} sword.")
        .unwrap();
    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &aldran);
    assert_eq!(
        PerspectiveEngine::render(&t_first, &ctx_first).unwrap(),
        "I am looking for my sword."
    );

    // forced_stance_overrides_first_person
    let t_force = cache
        .get_or_compile("{*A:+source:subj} [+source:draw] {a:+source:poss} sword.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_force, &ctx_first).unwrap(),
        "Aldran draws his sword."
    );

    // all_pronoun_cases_with_stances
    let t_cases = cache.get_or_compile("{A:source:Subj} [source:defend] {a:source:reflex}. {*The:target:subj} [target:strike] {a:source:obj}. It is {a:source:poss} fight, the victory is {a:source:abs_poss}!").unwrap();
    let ctx_cases = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &aldran)
        .with_entity("target", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&t_cases, &ctx_cases).unwrap(),
        "I defend myself. The goblin strikes me. It is my fight, the victory is mine!"
    );

    // possessive_nouns_with_stances / dynamic_possessive_nouns
    let t_poss = cache
        .get_or_compile("They take {*a:source's:poss} gold.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_poss, &ctx_first).unwrap(),
        "They take my gold."
    );

    // dynamic_past_tense
    let t_past = cache
        .get_or_compile("{*A:source:subj} [source:hit] {*the:target:obj} and [source:laugh].")
        .unwrap();
    let ctx_past = RenderContext::new("char_1")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &aldran)
        .with_entity("target", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&t_past, &ctx_past).unwrap(),
        "You hit the goblin and laughed."
    );

    // dynamic_past_tense_regular_fallbacks
    let t_chase = cache
        .get_or_compile("{*A:source:subj} [source:chase].")
        .unwrap();
    let ctx_director_past = RenderContext::new("char_2")
        .with_tense(crate::models::Tense::Past)
        .with_entity("source", &aldran);
    assert_eq!(
        PerspectiveEngine::render(&t_chase, &ctx_director_past).unwrap(),
        "Aldran chased."
    );

    // dynamic_past_tense_forced_conjugation
    let t_forced_past = cache
        .get_or_compile("{*A:source:subj} [source:freak out|freak out|freaks out].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_forced_past, &ctx_director_past).unwrap(),
        "Aldran freaked out."
    );
    let t_both = cache
        .get_or_compile("{*A:source:subj} [source:be|am|are|is;was|were|was] here.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_both, &ctx_director_past).unwrap(),
        "Aldran was here."
    );

    // dynamic_past_tense_pronouns_and_possessives
    let t_draw = cache.get_or_compile("{*A:source:subj} [source:draw] {a:source:poss} sword to defend {a:source:reflex}. The victory [source:be] {a:source:abs_poss}!").unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_draw, &ctx_director_past).unwrap(),
        "Aldran drew his sword to defend himself. The victory was his!"
    );

    // dynamic_past_tense_have_and_be
    let t_have = cache
        .get_or_compile(
            "{*A:source:subj} [source:have] no choice, {a:source:subj} [source:be] trapped.",
        )
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_have, &ctx_director_past).unwrap(),
        "Aldran had no choice, he was trapped."
    );

    // dynamic_past_tense_modal_verbs
    let t_can = cache
        .get_or_compile("{*A:source:subj} [source:can] win.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_can, &ctx_director_past).unwrap(),
        "Aldran could win."
    );

    // dynamic_future_tense
    let ctx_future = RenderContext::new("char_2")
        .with_tense(crate::models::Tense::Future)
        .with_entity("source", &aldran)
        .with_entity("target", &goblin);
    let t_walk = cache
        .get_or_compile("{*A:source:subj} [source:walk].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_walk, &ctx_future).unwrap(),
        "Aldran will walk."
    );

    // dynamic_future_tense_force_director_stance
    let t_future_force = cache
        .get_or_compile("{*A:+source:subj} [+source:win] the battle.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_future_force, &ctx_future).unwrap(),
        "Aldran will win the battle."
    );

    // dynamic_future_tense_do_support
    let t_do = cache
        .get_or_compile("{*A:source:subj} [source:do(aux)] not run.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_do, &ctx_future).unwrap(),
        "Aldran will not run."
    );

    // all_twelve_english_tenses
    assert_eq!(
        PerspectiveEngine::render(
            &t_walk,
            &RenderContext::new("char_2").with_entity("source", &aldran)
        )
        .unwrap(),
        "Aldran walks."
    );
}

#[test]
fn test_unified_possessives_with_long_descriptions_on_proper_nouns() {
    struct LongProperNoun {
        short_name: &'static str,
        long_name: &'static str,
    }
    impl TemplateEntity for LongProperNoun {
        fn contains_viewer(&self, _: &str) -> bool {
            false
        }
        fn gender(&self) -> Gender {
            Gender::Male
        }
        fn is_plural(&self) -> bool {
            false
        }
        fn is_proper_noun_for(&self, _: &str) -> bool {
            true
        }
        fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
            Cow::Borrowed(self.short_name)
        }
        fn long_display_name_for<'a>(&'a self, _: &str) -> Option<Cow<'a, str>> {
            Some(Cow::Borrowed(self.long_name))
        }
    }

    // Two kings named Arthur and two swords named Excalibur to force collisions
    // and trigger the long descriptions dynamically!
    let arthur1 = LongProperNoun {
        short_name: "Arthur",
        long_name: "Arthur the Elder",
    };
    let arthur2 = LongProperNoun {
        short_name: "Arthur",
        long_name: "Arthur the Younger",
    };
    let excalibur1 = LongProperNoun {
        short_name: "Excalibur",
        long_name: "Excalibur of the Lake",
    };
    let excalibur2 = LongProperNoun {
        short_name: "Excalibur",
        long_name: "Excalibur of the Stone",
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("a1", &arthur1)
        .with_entity("a2", &arthur2)
        .with_entity("e1", &excalibur1)
        .with_entity("e2", &excalibur2)
        .with_last_mentioned("a2")
        .with_last_mentioned("e2");

    // 1. Both owner and target use long descriptions with injected adjectives
    let t1 = cache.get_or_compile("{A1's glowing e1:obj}.").unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "Arthur the Elder's glowing Excalibur of the Lake."
    );

    // 2. Drop possessive override (@) correctly drops the long owner and the adjectives
    let t2 = cache.get_or_compile("{A1's glowing @e1:obj}.").unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).unwrap(),
        "Excalibur of the Lake."
    );
}

#[test]
fn test_unified_possessives_with_dot_notation_and_overrides() {
    struct Item {
        name: &'static str,
        is_proper: bool,
    }
    impl TemplateEntity for Item {
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
            self.is_proper
        }
        fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
            Cow::Borrowed(self.name)
        }
    }

    struct Actor {
        name: &'static str,
        item: Item,
    }
    impl TemplateEntity for Actor {
        fn contains_viewer(&self, _: &str) -> bool {
            false
        }
        fn gender(&self) -> Gender {
            Gender::Male
        }
        fn is_plural(&self) -> bool {
            false
        }
        fn is_proper_noun_for(&self, _: &str) -> bool {
            true
        }
        fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
            Cow::Borrowed(self.name)
        }
        fn get_property(&self, prop: &str) -> Option<&dyn TemplateEntity> {
            if prop == "item" {
                Some(&self.item)
            } else {
                None
            }
        }
    }

    let arthur = Actor {
        name: "Arthur",
        item: Item {
            name: "Excalibur",
            is_proper: true,
        },
    };
    let aldran = Actor {
        name: "Aldran",
        item: Item {
            name: "sword",
            is_proper: false,
        },
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("arthur", &arthur)
        .with_entity("aldran", &aldran);

    // 1. Target is a proper noun via dot notation -> Drops owner
    let t1 = cache
        .get_or_compile("{*A:arthur's @arthur.item:obj}.")
        .unwrap();
    assert_eq!(PerspectiveEngine::render(&t1, &ctx).unwrap(), "Excalibur.");

    // 2. Target is a common noun via dot notation -> Keeps owner
    let t2 = cache
        .get_or_compile("{*A:aldran's @aldran.item:obj}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).unwrap(),
        "Aldran's sword."
    );

    // 3. Owner is via dot notation, Target is proper -> Drops owner
    let t3 = cache
        .get_or_compile("{*A:aldran.item's @arthur.item:obj}.")
        .unwrap();
    assert_eq!(PerspectiveEngine::render(&t3, &ctx).unwrap(), "Excalibur.");

    // 4. Owner is via dot notation, Target is common -> Keeps owner
    let t4 = cache
        .get_or_compile("{*A:arthur.item's @aldran.item:obj}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t4, &ctx).unwrap(),
        "Excalibur's sword."
    );
}

#[test]
fn test_double_possessive_chains_unified() {
    let player = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let hilt = MockEntity {
        id: "item_1".into(),
        name: "hilt".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_entity("target", &hilt);

    // 1. Literal chained possessives!
    // The parser intelligently isolates `source` as the dynamic owner, and safely absorbs `sword's` into the adjectives string.
    let t1 = cache
        .get_or_compile("{*A:source's sword's target:obj}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "Aldran's sword's hilt."
    );

    // 2. Drop the entire chain!
    // If the target is the active viewer, the engine gracefully drops the owner AND all possessive adjectives!
    let ctx_viewer = RenderContext::new("item_1")
        .with_entity("source", &player)
        .with_entity("target", &hilt);
    assert_eq!(PerspectiveEngine::render(&t1, &ctx_viewer).unwrap(), "You.");

    // 3. Anaphora Pronoun Drop
    // If the target resolves to a pronoun, the entire double-possessive chain drops.
    let ctx_anaphora = ctx.with_last_mentioned("target");
    let t2 = cache
        .get_or_compile("{*A:source:subj} grabs {source's sword's target:obj}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx_anaphora).unwrap(),
        "Aldran grabs it."
    );
}

#[test]
fn test_unified_possessives_isolated_ordinals() {
    let goblin = MockEntity {
        id: "g1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let player = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let s1 = MockEntity {
        id: "s1".into(),
        name: "sword".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let s2 = MockEntity {
        id: "s2".into(),
        name: "sword".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let s3 = MockEntity {
        id: "s3".into(),
        name: "sword".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_1")
        .with_entity("g", &goblin)
        .with_entity("p", &player)
        .with_entity("s1", &s1)
        .with_entity("s2", &s2)
        .with_entity("s3", &s3)
        .with_lookahead(true);

    // 1. Separate owners -> Namespaced successfully, neither prints an ordinal string!
    let t1 = cache
        .get_or_compile("{*A:g:subj} [g:grab] {g's s1:obj} and you grab {p's s2:obj}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "A goblin grabs its sword and you grab your sword."
    );

    ctx.clear_anaphora();

    // 2. Same owner -> Both share the `g::sword` namespace bucket. Triggers ordinals!
    let t2 = cache
        .get_or_compile("{*A:g:subj} [g:grab] {g's s1:obj} and {g's s3:obj}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).unwrap(),
        "A goblin grabs its sword and its second sword."
    );
}

#[test]
fn test_unified_possessives_with_target_ordinals() {
    let s1 = MockEntity {
        id: "s1".into(),
        name: "sword".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let s2 = MockEntity {
        id: "s2".into(),
        name: "sword".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let player = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_entity("s1", &s1)
        .with_entity("s2", &s2)
        .with_lookahead(true); // Enable lookahead so ordinals are seeded immediately

    // Both swords collide on the name "sword", triggering ordinals 1 and 2.
    let t1 = cache
        .get_or_compile("{*A:source:subj} grabs {source's s1:obj} and {source's s2:obj}.")
        .unwrap();

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "Aldran grabs his sword and his second sword."
    );
}
#[test]
fn test_unified_possessives_with_independent_modifiers() {
    let player = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let sword = MockEntity {
        id: "item_1".into(),
        name: "sword".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_1") // Player is viewer!
        .with_entity("source", &player)
        .with_entity("target", &sword)
        .with_last_mentioned("target"); // Target is in memory, naturally resolves to "it"

    // 1. Normal unified possessive (Owner is viewer -> "your", target in memory -> drops owner -> "it")
    let t1 = cache.get_or_compile("{A:source's target:obj}.").unwrap();
    assert_eq!(PerspectiveEngine::render(&t1, &ctx).unwrap(), "It.");

    // 2. Modifiers: `+` on owner (forces "Aldran's"), `*` on target (forces noun "sword", keeps owner!)
    let t2 = cache.get_or_compile("{*A:+source's *target:obj}.").unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).unwrap(),
        "Aldran's sword."
    );
}

#[test]
fn test_unified_possessives_with_ordinals() {
    let goblin1 = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let goblin2 = MockEntity {
        id: "mob_2".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let sword = MockEntity {
        id: "item_1".into(),
        name: "sword".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("g1", &goblin1)
        .with_entity("g2", &goblin2)
        .with_entity("sword", &sword);

    // Seed ordinals so g1 becomes "the first goblin"
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:g1:subj} and {*a:g2:subj} arrive.")
            .unwrap(),
        &ctx,
    )
    .unwrap();

    // The `{A:...}` article naturally bounds to the owner `g1`, pulling its ordinal state
    // to output "The first goblin's", while natively suppressing the article for "sword".
    let t1 = cache.get_or_compile("{*A:g1's sword:obj}.").unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "The first goblin's sword."
    );
}

#[test]
fn test_unified_possessives_multiple_adjectives() {
    let player = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let sword = MockEntity {
        id: "item_1".into(),
        name: "sword".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("source", &player)
        .with_entity("target", &sword);

    // Multiple adjectives should be cleanly parsed and preserved
    let t1 = cache
        .get_or_compile("{A:source's big red glowing target:obj}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "Aldran's big red glowing sword."
    );

    // If the target drops the owner (e.g., pronoun fallback), it must drop ALL adjectives
    let ctx2 = ctx.with_last_mentioned("target");
    assert_eq!(PerspectiveEngine::render(&t1, &ctx2).unwrap(), "It.");
}

#[test]
fn test_demarcated_adjectives_unified() {
    let player = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let sword = MockEntity {
        id: "item_1".into(),
        name: "sword".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("source", &player)
        .with_entity("iron sword", &sword);

    // By using the explicit `:` separator, the engine unambiguously bounds the target key
    // to exactly "iron sword" and safely isolates "big red" as the adjectives!
    let t1 = cache
        .get_or_compile("{*A:source's big red:iron sword:obj}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "Aldran's big red sword."
    );
}

#[test]
#[serial]
fn test_unified_grammar_equivalents() {
    let cache = TemplateCache::new(100);
    let aldran = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let ctx = RenderContext::new("char_2").with_entity("source", &aldran);

    // custom_runtime_verbs
    crate::grammar::add_irregular_verb("yeet", "yeetses", "yeeted").unwrap();
    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{*A:source:subj} [source:yeet].")
                .unwrap(),
            &ctx
        )
        .unwrap(),
        "Aldran yeetses."
    );
    ctx.clear_anaphora();

    // macro_register_custom_verbs
    crate::register_custom_verbs! { "bloop" => ("bloopses", "bloopeded") };
    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{*A:source:subj} [source:bloop].")
                .unwrap(),
            &ctx
        )
        .unwrap(),
        "Aldran bloopses."
    );
    ctx.clear_anaphora();

    // complex_phrasal_and_hyphenated_verbs
    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{*A:source:subj} [source:look around].")
                .unwrap(),
            &ctx
        )
        .unwrap(),
        "Aldran looks around."
    );
    ctx.clear_anaphora();

    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{*A:source:subj} [source:cross-pollinate].")
                .unwrap(),
            &ctx
        )
        .unwrap(),
        "Aldran cross-pollinates."
    );
    ctx.clear_anaphora();

    // colliding_verbs_disambiguation
    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{*A:source:subj} [source:lie(lay)] down.")
                .unwrap(),
            &ctx.clone().with_tense(crate::models::Tense::Past)
        )
        .unwrap(),
        "Aldran lay down."
    );
    ctx.clear_anaphora();

    // irregular_verb_conjugations
    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{*A:source:subj} [source:fly].")
                .unwrap(),
            &ctx
        )
        .unwrap(),
        "Aldran flies."
    );

    crate::grammar::clear_irregular_verbs();
}

#[test]
fn test_unified_group_equivalents() {
    let cache = TemplateCache::new(100);
    let aldran = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let bob = MockEntity {
        id: "char_2".into(),
        name: "Bob".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolves = MockEntity {
        id: "mob_3".into(),
        name: "wolves".into(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let party = GroupEntity::new(vec![&aldran, &bob]);

    // group_entities_with_stances
    let t_stance = cache
        .get_or_compile("{A:source:subj} [source:open] the door.")
        .unwrap();
    assert_eq!(
        render_msg!("char_1", &t_stance, "source" => &party).unwrap(),
        "You and Bob open the door."
    );
    assert_eq!(
        render_msg!("char_3", &t_stance, "source" => &party).unwrap(),
        "Aldran and Bob open the door."
    );

    // plural_viewer_first_person_stance
    let t_plural = cache
        .get_or_compile(
            "{*A:source:subj} [source:attack] {*the:target:obj} with {a:source:poss} claws!",
        )
        .unwrap();
    let ctx_plural = RenderContext::new("mob_3")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &wolves)
        .with_entity("target", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&t_plural, &ctx_plural).unwrap(),
        "We attack the goblin with our claws!"
    );

    // group_entity_possessives
    let t_poss = cache
        .get_or_compile("You take {*the:source's:poss} gold.")
        .unwrap();
    assert_eq!(
        render_msg!("char_1", &t_poss, "source" => &party).unwrap(),
        "You take your and Bob's gold."
    );

    // nested_group_anaphora
    let empty_group = GroupEntity::new(vec![]);
    let nested = GroupEntity::new(vec![&empty_group, &aldran]);
    let ctx_nested = RenderContext::new("viewer").with_entity("target", &nested);
    let t_nested = cache
        .get_or_compile("{*the:target:subj} [target:nod]. {A:target:Subj} [target:smile].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_nested, &ctx_nested).unwrap(),
        "Aldran nods. He smiles."
    );

    // article_upgrades_for_plural_viewers
    let ctx_pack = RenderContext::new("mob_3")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &wolves);
    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{A:source:subj} [source:howl].")
                .unwrap(),
            &ctx_pack
        )
        .unwrap(),
        "We howl."
    );
    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{Some:source:subj} [source:howl].")
                .unwrap(),
            &ctx_pack
        )
        .unwrap(),
        "We howl."
    );
    assert_eq!(
        PerspectiveEngine::render(
            &cache
                .get_or_compile("{One of the:source:subj} [source:howl].")
                .unwrap(),
            &ctx_pack
        )
        .unwrap(),
        "We howl."
    );
}

#[test]
fn test_unified_resolution_equivalents() {
    let cache = TemplateCache::new(100);
    let w1 = MockEntity {
        id: "w1".into(),
        name: "wolf".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let w2 = MockEntity {
        id: "w2".into(),
        name: "wolf".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let w3 = MockEntity {
        id: "w3".into(),
        name: "wolf".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let orcs = MockEntity {
        id: "mob_4".into(),
        name: "orcs".into(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };
    let goblin = MockEntity {
        id: "mob_1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let avengers = MockEntity {
        id: "char_a".into(),
        name: "the Avengers".into(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: true,
    };

    // plural_ordinals_and_demonstratives
    let ctx_ord = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2)
        .with_entity("w3", &w3);
    let t_ord1 = cache
        .get_or_compile(
            "{*A:w1:subj} [w1:arrive]. {*A:w2:subj} [w2:arrive]. {*A:w3:subj} [w3:arrive].",
        )
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_ord1, &ctx_ord).unwrap(),
        "A wolf arrives. Another wolf arrives. A third wolf arrives."
    );

    let t_ord2 = cache
        .get_or_compile("{*This:w1:subj} [w1:howl]. {*That:w2:subj} [w2:howl].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_ord2, &ctx_ord).unwrap(),
        "This first wolf howls. That second wolf howls."
    );

    // no_smart_modifier_bypasses_ordinals
    let t_no_smart = cache
        .get_or_compile("{*!A:w1:subj}, {*!another:w2:subj}, and {*!a:w3:subj} arrive.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_no_smart, &ctx_ord).unwrap(),
        "A wolf, another wolf, and a wolf arrive."
    );

    // singular_override_reflexive_pronouns
    let ctx_orcs = RenderContext::new("viewer")
        .with_entity("orcs", &orcs)
        .with_entity("goblin", &goblin);
    let _ = PerspectiveEngine::render(
        &cache.get_or_compile("{*The:orcs:subj} are here.").unwrap(),
        &ctx_orcs,
    )
    .unwrap();
    let t_reflex_pl = cache
        .get_or_compile("{A:orcs:Subj} [orcs:hurt] {a:orcs:reflex}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_reflex_pl, &ctx_orcs).unwrap(),
        "They hurt themselves."
    );
    let t_reflex_sg = cache
        .get_or_compile("{A:-orcs:Subj} [-orcs:hurt] {a:-orcs:reflex}.")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_reflex_sg, &ctx_orcs).unwrap(),
        "It hurts itself."
    );

    // plural_proper_noun_with_singular_override
    let ctx_avengers = RenderContext::new("viewer").with_entity("avengers", &avengers);
    let t_avenge1 = cache
        .get_or_compile(
            "{*A:avengers:subj} [avengers:assemble] and [avengers:defend] {a:avengers:reflex}.",
        )
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_avenge1, &ctx_avengers).unwrap(),
        "The Avengers assemble and defend themselves."
    );
    let t_avenge2 = cache
        .get_or_compile(
            "{*A:-avengers:subj} [-avengers:assemble] and [-avengers:defend] {a:-avengers:reflex}.",
        )
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_avenge2, &ctx_avengers).unwrap(),
        "The Avengers assembles and defends itself."
    );

    ctx_orcs.clear_anaphora();

    // singular_override_tenses_and_stances
    let t_charge = cache
        .get_or_compile("{One of the:-orcs:Subj} [-orcs:charge].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_charge, &ctx_orcs).unwrap(),
        "One of the orcs charges."
    );

    ctx_orcs.clear_anaphora();

    // singular_override_ambiguity_and_possessives
    let t_ambig = cache
        .get_or_compile(
            "{A:goblin:Subj} snarls. {One of the:-orcs:subj} [-orcs:draw] {-orcs:poss} blade!",
        )
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_ambig, &ctx_orcs).unwrap(),
        "A goblin snarls. One of the orcs draws its blade!"
    );

    ctx_orcs.clear_anaphora();

    // singular_override_forced_conjugation_and_lookahead
    let ctx_lookahead = ctx_orcs.clone().with_lookahead(true);
    let t_lookahead = cache.get_or_compile("{*One of the:-orcs:Subj} [-orcs:be|am|are|is] here. {:-orcs:Subj} [-orcs:have|have|have|has] arrived!").unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_lookahead, &ctx_lookahead).unwrap(),
        "One of the orcs is here. It has arrived!"
    );

    ctx_orcs.clear_anaphora();

    // modifier_stacking_order_independence
    let t_stack = cache
        .get_or_compile("{A:+!-orcs:subj} [+!-orcs:nod].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t_stack, &ctx_orcs).unwrap(),
        "It nods."
    );

    // lookahead_prevents_silent_bob
    let ctx_silent = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2)
        .with_lookahead(true);
    assert_eq!(
        PerspectiveEngine::render(
            &cache.get_or_compile("{*A:w1:subj} howls.").unwrap(),
            &ctx_silent
        )
        .unwrap(),
        "A wolf howls."
    );
}
