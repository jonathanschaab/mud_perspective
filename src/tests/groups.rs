use super::common::MockEntity;
use crate::cache::TemplateCache;
use crate::engine::PerspectiveEngine;
use crate::models::{Gender, GroupEntity, RenderContext, TemplateEntity};
use std::borrow::Cow;

#[test]
fn test_group_entity_perspectives() {
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
    let enemy = MockEntity {
        id: "mob_1".to_string(),
        name: "Goblin".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: false,
    };
    let stranger = MockEntity {
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
        members: vec![&player, &ally, &stranger],
    };

    let cache = TemplateCache::new(100);

    // --- SCENARIO 1: Verbs & Display Names ---
    let template_action = cache
        .get_or_compile("{*A:source:subj} [source:open] the door.")
        .expect("Failed to compile template");

    // Viewer is IN the party -> Expects "you" injection and uninflected verb
    let member_action = render_msg!("char_1", &template_action, "source" => &party)
        .expect("Failed to render template");
    assert_eq!(member_action, "You and Bob open the door.");

    // Viewer is OUTSIDE the party -> Expects 3rd-person names, but still an uninflected verb
    let observer_action = render_msg!("char_3", &template_action, "source" => &party)
        .expect("Failed to render template");
    assert_eq!(observer_action, "Aldran and Bob open the door.");

    // Oxford comma test for 3+ members
    let observer_big = render_msg!("mob_1", &template_action, "source" => &big_party)
        .expect("Failed to render template");
    assert_eq!(observer_big, "Aldran, Bob, and Charlie open the door.");

    // --- SCENARIO 2: Group Pronouns ---
    let template_pronoun = cache
        .get_or_compile("{*the:source:subj} [source:attack] {a:target:obj}!")
        .expect("Failed to compile template");

    // The group is the target, viewer is IN the group -> Expects 2nd-person "you"
    let member_pronoun =
        render_msg!("char_1", &template_pronoun, "source" => &enemy, "target" => &party)
            .expect("Failed to render template");
    assert_eq!(member_pronoun, "The Goblin attacks you and Bob!");

    // The group is the target, viewer is OUTSIDE the group -> Expects 3rd-person plural "them"
    let observer_pronoun =
        render_msg!("char_3", &template_pronoun, "source" => &enemy, "target" => &party)
            .expect("Failed to render template");
    assert_eq!(observer_pronoun, "The Goblin attacks Aldran and Bob!");

    // --- SCENARIO 3: Article Suppression ---
    let template_article = cache
        .get_or_compile("{*the:source:subj} [source:be] ready.")
        .expect("Failed to compile template");

    // Viewer IN party -> suppresses article (starts with "You")
    let member_article = render_msg!("char_1", &template_article, "source" => &party)
        .expect("Failed to render template");
    assert_eq!(member_article, "You and Bob are ready.");

    // Viewer OUTSIDE party -> suppresses article because the Group is treated as a proper noun
    let observer_article = render_msg!("char_3", &template_article, "source" => &party)
        .expect("Failed to render template");
    assert_eq!(observer_article, "Aldran and Bob are ready.");

    // --- SCENARIO 4: Mixed Recognition (Internal Articles) ---
    let mixed_party = GroupEntity {
        members: vec![&player, &enemy],
    };
    let template_mixed = cache
        .get_or_compile("{*the:source:subj} [source:prepare] for battle.")
        .expect("Failed to compile template");

    let observer_mixed = render_msg!("char_3", &template_mixed, "source" => &mixed_party)
        .expect("Failed to render template");
    // "Aldran" is a proper noun (no article), "Goblin" is a common noun (gets "the").
    assert_eq!(observer_mixed, "Aldran and the Goblin prepare for battle.");
}

#[test]
fn test_reflexive_plural_pronouns() {
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
    let template = cache
        .get_or_compile("{*A:source:subj} [source:defend] {a:source:reflex}!")
        .expect("Failed to compile template");

    // Plural Viewer (Actor Stance) -> tests the "yourselves" logic
    let plural_actor =
        render_msg!("char_1", &template, "source" => &party).expect("Failed to render template");
    assert_eq!(plural_actor, "You and Bob defend yourselves!");
}

#[test]
fn test_reflexive_group_object() {
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
        .get_or_compile("{A:player:subj} [player:slash] {*the:party:obj}.")
        .expect("Failed to compile template");

    // 1. First Person
    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("player", &player)
        .with_entity("party", &party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first).expect("Failed to render template"),
        "I slash the goblin and myself."
    );

    // 2. Second Person
    let ctx_second = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::SecondPerson)
        .with_entity("player", &player)
        .with_entity("party", &party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_second).expect("Failed to render template"),
        "You slash yourself and the goblin."
    );

    // 3. Third Person
    let ctx_third = RenderContext::new("char_3")
        .with_stance(crate::models::ActorStance::ThirdPerson)
        .with_entity("player", &player)
        .with_entity("party", &party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_third).expect("Failed to render template"),
        "Aldran slashes himself and the goblin."
    );
}

#[test]
fn test_nested_and_empty_group_entities() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let ally1 = MockEntity {
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
    let ally3 = MockEntity {
        id: "char_4".to_string(),
        name: "Dave".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let empty_group = GroupEntity { members: vec![] };
    let sub_group = GroupEntity {
        members: vec![&player, &ally1],
    };

    // top_group contains a nested group, an empty group, and regular entities
    let top_group = GroupEntity {
        members: vec![&sub_group, &empty_group, &ally2, &ally3],
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{*the:source:subj} [source:prepare].")
        .expect("Failed to compile template");

    // 1. Director Stance (bystander sees everyone)
    // Expects empty group to be completely ignored.
    // Nested group is flattened so it prints as a single cohesive list.
    let out_director = render_msg!("stranger_1", &template, "source" => &top_group)
        .expect("Failed to render template");
    assert_eq!(
        out_director,
        "The tall man, Bob, Charlie, and Dave prepare."
    );

    // 2. Actor Stance (Player is the viewer)
    // Expects "You" to be pulled to the front of the flattened list cleanly.
    let out_actor = render_msg!("char_1", &template, "source" => &top_group)
        .expect("Failed to render template");
    assert_eq!(out_actor, "You, Bob, Charlie, and Dave prepare.");
}

#[test]
fn test_single_member_group_grammar() {
    let aldran = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let solo_group = GroupEntity {
        members: vec![&aldran],
    };

    let cache = TemplateCache::new(100);

    // 1. Verb Conjugation
    // Because Aldran is singular, the verb "open" must conjugate to "opens"
    let template_verb = cache
        .get_or_compile("{*A:source:subj} [source:open] the door.")
        .expect("Failed to compile template");
    let out_verb = render_msg!("char_2", &template_verb, "source" => &solo_group)
        .expect("Failed to render template");
    assert_eq!(out_verb, "Aldran opens the door.");

    // 2. Pronoun Resolution
    // Because Aldran is male, the pronoun must be "his" instead of "their"
    let template_pronoun = cache
        .get_or_compile("{*A:source:subj} [source:open] {a:source:poss} door.")
        .expect("Failed to compile template");
    let out_pronoun = render_msg!("char_2", &template_pronoun, "source" => &solo_group)
        .expect("Failed to render template");
    assert_eq!(out_pronoun, "Aldran opens his door.");

    // 3. Articles for common noun wrapped in a group
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let goblin_group = GroupEntity {
        members: vec![&goblin],
    };
    let template_art = cache
        .get_or_compile("{*the:source:subj} [source:attack].")
        .expect("Failed to compile template");
    let out_art = render_msg!("char_2", &template_art, "source" => &goblin_group)
        .expect("Failed to render template");
    assert_eq!(out_art, "The goblin attacks.");
}

#[test]
fn test_group_entities_with_stances() {
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
    let template = cache
        .get_or_compile("{*A:source:subj} [source:open] the door.")
        .expect("Failed to compile template");

    // First Person
    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first).expect("Failed to render template"),
        "Bob and I open the door."
    );

    // Third Person
    let ctx_third = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::ThirdPerson)
        .with_entity("source", &party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_third).expect("Failed to render template"),
        "Aldran and Bob open the door."
    );
}

#[test]
fn test_plural_viewer_first_person_stance() {
    let wolves = MockEntity {
        id: "mob_1".to_string(),
        name: "wolves".to_string(),
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
    let template = cache
        .get_or_compile(
            "{*A:source:subj} [source:attack] {*the:target:obj} with {a:source:poss} claws!",
        )
        .expect("Failed to compile template");

    let ctx = RenderContext::new("mob_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &wolves)
        .with_entity("target", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
        "We attack the goblin with our claws!"
    );

    // Group with plural viewer
    let party = GroupEntity {
        members: vec![&wolves, &goblin],
    };

    let group_template = cache
        .get_or_compile("{*the:source:subj} [source:attack]!")
        .expect("Failed to compile template");
    let group_ctx = RenderContext::new("mob_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &party);

    assert_eq!(
        PerspectiveEngine::render(&group_template, &group_ctx).expect("Failed to render template"),
        "You, the goblin, and I attack!"
    );

    // Objective pronouns
    let obj_template = cache
        .get_or_compile("{*the:target:subj} [target:ambush] {a:source:obj}!")
        .expect("Failed to compile template");

    // 1. Solo plural viewer
    let obj_ctx_solo = RenderContext::new("mob_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &wolves)
        .with_entity("target", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&obj_template, &obj_ctx_solo).expect("Failed to render template"),
        "The goblin ambushes us!"
    );

    // 2. Mixed group containing plural viewer
    let obj_ctx_group = RenderContext::new("mob_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &party)
        .with_entity("target", &goblin);

    // A pronoun referring to a group that includes a 1st-person viewer correctly collapses to "us"
    assert_eq!(
        PerspectiveEngine::render(&obj_template, &obj_ctx_group)
            .expect("Failed to render template"),
        "The goblin ambushes us!"
    );
}

#[test]
fn test_group_entity_possessives() {
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
    let slime = MockEntity {
        id: "mob_2".to_string(),
        name: "slime".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolves = MockEntity {
        id: "mob_3".to_string(),
        name: "wolves".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let mixed_party = GroupEntity {
        members: vec![&player, &goblin],
    };
    let solo_party = GroupEntity {
        members: vec![&player],
    };
    let big_mixed_party = GroupEntity {
        members: vec![&player, &goblin, &slime],
    };
    let solo_wolves_party = GroupEntity {
        members: vec![&wolves],
    };
    let mixed_wolves_party = GroupEntity {
        members: vec![&wolves, &goblin],
    };

    let template = cache
        .get_or_compile("You take {*the:source's:poss} gold.")
        .expect("Failed to compile template");

    // 1. Second Person Mixed -> "your and the goblin's"
    let ctx_second = RenderContext::new("char_1").with_entity("source", &mixed_party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_second).expect("Failed to render template"),
        "You take your and the goblin's gold."
    );

    // 2. First Person Mixed -> "the goblin's and my"
    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &mixed_party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first).expect("Failed to render template"),
        "You take the goblin's and my gold."
    );

    // 3. Second Person Solo -> "your"
    let ctx_solo_second = RenderContext::new("char_1").with_entity("source", &solo_party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_solo_second).expect("Failed to render template"),
        "You take your gold."
    );

    // 4. First Person Solo -> "my"
    let ctx_solo_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &solo_party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_solo_first).expect("Failed to render template"),
        "You take my gold."
    );

    // 5. Third Person Mixed -> "Aldran and the goblin's"
    let ctx_third = RenderContext::new("char_2").with_entity("source", &mixed_party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_third).expect("Failed to render template"),
        "You take Aldran and the goblin's gold."
    );

    // 6. First Person Big Mixed -> "the goblin's, the slime's, and my"
    let ctx_first_big = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &big_mixed_party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first_big).expect("Failed to render template"),
        "You take the goblin's, the slime's, and my gold."
    );

    // 7. Second Person Big Mixed -> "your, the goblin's, and the slime's"
    let ctx_second_big = RenderContext::new("char_1").with_entity("source", &big_mixed_party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_second_big).expect("Failed to render template"),
        "You take your, the goblin's, and the slime's gold."
    );

    // 8. First Person Plural Solo -> "our"
    let ctx_first_plural_solo = RenderContext::new("mob_3")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &solo_wolves_party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first_plural_solo)
            .expect("Failed to render template"),
        "You take our gold."
    );

    // 9. First Person Plural Mixed -> "your, the goblin's, and my"
    let ctx_first_plural_mixed = RenderContext::new("mob_3")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &mixed_wolves_party);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first_plural_mixed)
            .expect("Failed to render template"),
        "You take your, the goblin's, and my gold."
    );
}

#[test]
fn test_group_entity_anaphora_resolution() {
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
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let slime = MockEntity {
        id: "mob_2".to_string(),
        name: "slime".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let party = GroupEntity {
        members: vec![&player, &ally],
    };
    let monsters = GroupEntity {
        members: vec![&goblin, &slime],
    };

    let cache = TemplateCache::new(100);

    // 1. Unambiguous Group Pronoun
    let t1 = cache
        .get_or_compile(
            "{*the:goblin:subj} [goblin:ambush] {*a:party:obj}. {a:party:Subj} [party:retaliate]!",
        )
        .expect("Failed to compile template");
    let ctx1 = RenderContext::new("char_3")
        .with_entity("party", &party)
        .with_entity("goblin", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template"),
        "The goblin ambushes Aldran and Bob. They retaliate!"
    );

    // 2. Ambiguous Group Pronoun (Monsters and Party are both Plural)
    let t2 = cache
        .get_or_compile(
            "{*the:monsters:subj} [monsters:ambush] {*a:party:obj}. {a:party:Subj} [party:retaliate]!",
        )
        .expect("Failed to compile template");
    let ctx2 = RenderContext::new("char_3")
        .with_entity("party", &party)
        .with_entity("monsters", &monsters);

    // The anaphora ambiguity check should safely catch the collision and fall back to the group's name.
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx2).expect("Failed to render template"),
        "The goblin and the slime ambush Aldran and Bob. Aldran and Bob retaliate!"
    );

    // 3. Ambiguous Group Pronoun with Viewer Included (Actor Stance)
    // Because "you" (or "we") is unambiguous regardless of other entities, it securely bypasses ambiguity checks!
    let ctx3 = RenderContext::new("char_1")
        .with_entity("party", &party)
        .with_entity("monsters", &monsters);

    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx3).expect("Failed to render template"),
        "The goblin and the slime ambush you and Bob. You and Bob retaliate!"
    );

    // 4. First Person Stance with Viewer
    let ctx4 = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("party", &party)
        .with_entity("monsters", &monsters);

    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx4).expect("Failed to render template"),
        "The goblin and the slime ambush Bob and me. We retaliate!"
    );
}

#[test]
fn test_nested_group_anaphora() {
    let aldran = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let bob = MockEntity {
        id: "char_2".to_string(),
        name: "Bob".to_string(),
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

    let empty_group = GroupEntity { members: vec![] };

    // A nested group containing an empty group and one person.
    // Because there is only one leaf member, it should evaluate as Singular Male!
    let nested_solo = GroupEntity {
        members: vec![&empty_group, &aldran],
    };

    // A nested group containing the solo group and another person.
    // Should evaluate as Plural.
    let nested_plural = GroupEntity {
        members: vec![&nested_solo, &bob],
    };

    let cache = TemplateCache::new(100);

    // 1. Nested Solo -> Acts as Singular Male
    let t1 = cache
        .get_or_compile("{*the:target:subj} [target:nod]. {a:target:Subj} [target:smile].")
        .expect("Failed to compile template");
    let ctx1 = RenderContext::new("viewer").with_entity("target", &nested_solo);
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template"),
        "Aldran nods. He smiles."
    );

    // 2. Ambiguity with Nested Solo (Male) and Bob (Male)
    let t2 = cache
        .get_or_compile(
            "{*A:bob:subj} [bob:look] at {*A:target:obj}. {a:target:Subj} [target:smile].",
        )
        .expect("Failed to compile template");
    let ctx2 = RenderContext::new("viewer")
        .with_entity("bob", &bob)
        .with_entity("target", &nested_solo);
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx2).expect("Failed to render template"),
        "Bob looks at Aldran. Aldran smiles."
    );

    // 3. Nested Plural -> Acts as Plural
    let t3 = cache
        .get_or_compile("{*the:party:subj} [party:nod]. {a:party:Subj} [party:smile].")
        .expect("Failed to compile template");
    let ctx3 = RenderContext::new("viewer").with_entity("party", &nested_plural);
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx3).expect("Failed to render template"),
        "Aldran and Bob nod. They smile."
    );

    // 4. Ambiguity with Nested Plural (Plural) and Wolves (Plural)
    let t4 = cache
        .get_or_compile(
            "{*the:wolves:subj} [wolves:look] at {*A:party:obj}. {a:party:Subj} [party:smile].",
        )
        .expect("Failed to compile template");
    let ctx4 = RenderContext::new("viewer")
        .with_entity("wolves", &wolves)
        .with_entity("party", &nested_plural);
    assert_eq!(
        PerspectiveEngine::render(&t4, &ctx4).expect("Failed to render template"),
        "The wolves look at Aldran and Bob. Aldran and Bob smile."
    );
}

#[test]
fn test_nested_properties_returning_group_entities() {
    let goblin1 = MockEntity {
        id: "m1".into(),
        name: "goblin".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let goblin2 = MockEntity {
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
    impl<'a> TemplateEntity for Boss<'a> {
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
        minions: GroupEntity::new(vec![&goblin1, &goblin2]),
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer").with_entity("boss", &boss);

    // The dot notation safely traverses into the GroupEntity and triggers the Oxford comma formatter
    // and correctly routes the plural 'attack' verb!
    let t1 = cache
        .get_or_compile("{The:boss.minions:subj} [boss.minions:attack]!")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "The goblin and the slime attack!"
    );
}

#[test]
fn test_explicit_capitalization_after_possessive_in_groups() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let sword = MockEntity {
        id: "item_1".to_string(),
        name: "sword".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let shield = MockEntity {
        id: "item_2".to_string(),
        name: "shield".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let weapons = GroupEntity::new(vec![&sword, &shield]);

    let ctx = RenderContext::new("char_2")
        .with_entity("player", &player)
        .with_entity("weapons", &weapons);

    // 1. Uncapitalized explicit noun after possessive
    let t_normal = cache
        .get_or_compile("{player's weapons}.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_normal, &ctx).expect("Failed to render template"),
        "Aldran's sword and shield."
    );

    // 2. Explicitly capitalized noun {Weapons} after possessive
    let t_cap = cache
        .get_or_compile("{player's} {Weapons}.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_cap, &ctx).expect("Failed to render template"),
        "Aldran's Sword and shield."
    );
}

#[test]
fn test_group_entity_forced_3rd_person() {
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

    let party = GroupEntity::new(vec![&player, &ally]);
    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_1").with_entity("party", &party);

    let t1 = cache
        .get_or_compile("{*A:party:subj} [party:arrive].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "You and Bob arrive."
    );

    // With the `+` modifier, the viewer check naturally resolves the entire group evaluation into Director Stance!
    let t2 = cache
        .get_or_compile("{*A:+party:subj} [+party:arrive].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template"),
        "Aldran and Bob arrive."
    );
}
