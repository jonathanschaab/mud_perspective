use super::common::MockEntity;
use crate::cache::TemplateCache;
use crate::engine::PerspectiveEngine;
use crate::models::{Gender, RenderContext};

#[test]
fn test_anaphora_resolution() {
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

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_2")
        .with_entity("target", &goblin)
        .with_entity("other", &slime);

    // 1. First time using a pronoun tag: Automatically expands to the full name!
    let t1 = cache
        .get_or_compile("{a:target:Subj} [target:look] around.")
        .expect("Failed to compile template");
    let out1 = PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template");
    assert_eq!(out1, "A goblin looks around.");

    // 2. Second time using a pronoun tag: The context REMEMBERS the goblin and uses "It"!
    let t2 = cache
        .get_or_compile("{a:target:Subj} [target:attack]!")
        .expect("Failed to compile template");
    let out2 = PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template");
    assert_eq!(out2, "It attacks!");

    // 3. Clearing the context resets the memory, expanding it to the full name again.
    ctx.clear_anaphora();
    let out3 = PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template");
    assert_eq!(out3, "A goblin attacks!");

    // 4. Interruption by another entity prevents confusing pronouns
    ctx.clear_anaphora();
    let t4 = cache
        .get_or_compile(
            "{*The:target:subj} enters. {*The:other:subj} blinks. {a:target:Subj} [target:scream].",
        )
        .expect("Failed to compile template");
    let out4 = PerspectiveEngine::render(&t4, &ctx).expect("Failed to render template");
    // Because the slime (Neutral) was just introduced, the pronoun for the target (goblin, also Neutral)
    // is now ambiguous. The engine must expand it back to "The goblin" to prevent confusion.
    assert_eq!(
        out4,
        "The goblin enters. The slime blinks. The goblin screams."
    );

    // 5. Reflexive pronouns explicitly bypass Anaphora resolution.
    // Possessive pronouns fall back to possessive nouns!
    ctx.clear_anaphora();
    let t5 = cache
        .get_or_compile("{*A:other:poss} sword falls, and {a:other:subj} cuts {a:other:reflex}.")
        .expect("Failed to compile template");

    let out5 = PerspectiveEngine::render(&t5, &ctx).expect("Failed to render template");
    assert_eq!(out5, "A slime's sword falls, and it cuts itself.");
}

#[test]
fn test_anaphora_ambiguity_resolution() {
    let bob = MockEntity {
        id: "char_2".to_string(),
        name: "Bob".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
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
    let jill = MockEntity {
        id: "char_4".to_string(),
        name: "Jill".to_string(),
        gender: Gender::Female,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_entity("bob", &bob)
        .with_entity("aldran", &aldran)
        .with_entity("goblin", &goblin)
        .with_entity("jill", &jill);

    // 1. Unambiguous object reference (Goblin -> Neutral, Aldran -> Male)
    let t1 = cache
        .get_or_compile(
            "{*the:goblin:subj} [goblin:hit] {*A:aldran:obj}. {a:aldran:Subj} [aldran:smile].",
        )
        .unwrap();
    let out1 = PerspectiveEngine::render(&t1, &ctx).unwrap();
    assert_eq!(out1, "The goblin hits Aldran. He smiles.");

    // 2. Ambiguous object reference (Bob -> Male, Aldran -> Male)
    // Using "He" for Aldran is ambiguous, so the engine must fall back to "Aldran"
    ctx.clear_anaphora();
    let t2 = cache
        .get_or_compile("{*A:bob:subj} [bob:hit] {*A:aldran:obj}. {a:aldran:Subj} [aldran:smile].")
        .unwrap();
    let out2 = PerspectiveEngine::render(&t2, &ctx).unwrap();
    assert_eq!(out2, "Bob hits Aldran. Aldran smiles.");

    // 3. Ambiguous object reference with 3+ entities (Jill -> Female, Bob -> Male, Aldran -> Male)
    // Active subject is Jill. Target is Aldran. Jill is Female, Aldran is Male (unambiguous vs subject).
    // BUT Bob is Male. So "He" is ambiguous between Bob and Aldran.
    ctx.clear_anaphora();
    let t3 = cache
        .get_or_compile(
            "{*A:jill:subj} [jill:tell] {*A:bob:obj} about {*A:aldran:obj}. {a:aldran:Subj} [aldran:smile].",
        )
        .unwrap();
    let out3 = PerspectiveEngine::render(&t3, &ctx).unwrap();
    // Because Bob is in the `recent_entities` memory, "He" is correctly bypassed!
    assert_eq!(out3, "Jill tells Bob about Aldran. Aldran smiles.");
}

#[test]
fn test_standalone_verb_anaphora_tracking() {
    let bob = MockEntity {
        id: "char_2".to_string(),
        name: "Bob".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let aldran = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let jill = MockEntity {
        id: "char_4".to_string(),
        name: "Jill".to_string(),
        gender: Gender::Female,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // 1. Ambiguous tracking via verb tag
    // Bob is ONLY introduced via a verb tag. If the engine fails to track him,
    // it will erroneously use "He" for Aldran because it forgets a second male is present.
    let ctx1 = RenderContext::new("viewer")
        .with_entity("bob", &bob)
        .with_entity("aldran", &aldran);
    let t1 = cache
        .get_or_compile("Bob [bob:attack] {*A:aldran:obj}. {a:aldran:Subj} [aldran:fall].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template"),
        "Bob attacks Aldran. Aldran falls."
    );

    // 2. Unambiguous tracking via verb tag
    // Jill is introduced via a verb tag. Because she is Female, Aldran (Male) can safely use "He".
    let ctx2 = RenderContext::new("viewer")
        .with_entity("jill", &jill)
        .with_entity("aldran", &aldran);
    let t2 = cache
        .get_or_compile("Jill [jill:attack] {*A:aldran:obj}. {a:aldran:Subj} [aldran:fall].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx2).expect("Failed to render template"),
        "Jill attacks Aldran. He falls."
    );
}

#[test]
fn test_anaphora_fallback_capitalization() {
    let monster = MockEntity {
        id: "mob_1".to_string(),
        name: "giant spider".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer").with_entity("target", &monster);

    // {target:Subj} requests a capitalized pronoun ("It").
    // Since it's the first mention, it falls back to the full noun with an article.
    // We expect "A giant spider", NOT "A Giant Spider" or "A Giant spider".
    let template = cache
        .get_or_compile("{a:target:Subj} [target:hiss].")
        .expect("Failed to compile template");
    let output = PerspectiveEngine::render(&template, &ctx).expect("Failed to render template");
    assert_eq!(output, "A giant spider hisses.");
}

#[test]
fn test_anaphora_across_contexts() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let t1 = cache
        .get_or_compile("{*the:target:subj} enters.")
        .expect("Failed to compile template");
    let t2 = cache
        .get_or_compile("{a:target:Subj} [target:look] around.")
        .expect("Failed to compile template");

    // Render the first template in context 1
    let ctx1 = RenderContext::new("char_2").with_entity("target", &goblin);
    let _ = PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template");

    // Extract the full narrative state from context 1 and inject it into a brand new context 2
    let state = ctx1.extract_anaphora();
    let ctx2 = RenderContext::new("char_2")
        .with_entity("target", &goblin)
        .with_anaphora(state);

    let out2 = PerspectiveEngine::render(&t2, &ctx2).expect("Failed to render template");
    // The engine natively uses "It" instead of "The goblin" because the context was seeded!
    assert_eq!(out2, "It looks around.");
}

#[test]
fn test_anaphora_state_preserves_ambiguity() {
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

    let cache = TemplateCache::new(100);
    let t1 = cache
        .get_or_compile("{*A:bob:subj} is standing next to {*A:aldran:obj}.")
        .expect("Failed to compile template");
    let t2 = cache
        .get_or_compile("{a:aldran:Subj} [aldran:wave].")
        .expect("Failed to compile template");

    // Render the first sentence
    let ctx1 = RenderContext::new("viewer")
        .with_entity("aldran", &aldran)
        .with_entity("bob", &bob);
    let out1 = PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template");
    assert_eq!(out1, "Bob is standing next to Aldran.");

    // Carry the state over to context 2
    let state = ctx1.extract_anaphora();
    let ctx2 = RenderContext::new("viewer")
        .with_entity("aldran", &aldran)
        .with_entity("bob", &bob)
        .with_anaphora(state);

    // Because the state includes Bob in the recent_entities memory, the engine knows
    // that Aldran and Bob are both male and prevents the ambiguous "He waves."
    let out2 = PerspectiveEngine::render(&t2, &ctx2).expect("Failed to render template");
    assert_eq!(out2, "Aldran waves.");
}

#[test]
fn test_anaphora_memory_limit() {
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
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // We set the memory limit to 2 for this test
    let ctx = RenderContext::new("viewer")
        .with_anaphora_limit(2)
        .with_entity("aldran", &aldran)
        .with_entity("bob", &bob)
        .with_entity("goblin", &goblin);

    // 1. Introduce Aldran and Bob (Memory: Aldran, Bob)
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:aldran:subj} [aldran:wave] at {*A:bob:obj}.")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");

    // 2. Introduce the goblin (Memory: Bob, Goblin). Aldran is evicted!
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*the:goblin:subj} [goblin:approach].")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");

    // 3. Request a pronoun for Bob. He is still in memory, so he is safely remembered as the subject.
    let out_bob = PerspectiveEngine::render(
        &cache
            .get_or_compile("{a:bob:Subj} [bob:smile].")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");
    assert_eq!(out_bob, "He smiles.");

    // 4. Request a pronoun for Aldran. Because he was evicted, the engine forgot he was Male,
    // and must fall back to his name!
    let out_aldran = PerspectiveEngine::render(
        &cache
            .get_or_compile("{a:aldran:Subj} [aldran:sigh].")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");
    assert_eq!(out_aldran, "Aldran sighs.");
}

#[test]
fn test_context_builder_methods() {
    let w1 = MockEntity {
        id: "mob_1".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let w2 = MockEntity {
        id: "mob_2".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let w3 = MockEntity {
        id: "mob_3".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    // Test with_ordinal_word_threshold and its effect on rendering
    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer")
        .with_ordinal_word_threshold(2) // 1 and 2 will be words ("first", "second"), 3+ will be numeric ("3rd")
        .with_entity("w1", &w1)
        .with_entity("w2", &w2)
        .with_entity("w3", &w3);

    let t1 = cache
        .get_or_compile("{*A:w1:subj} walks in. {*A:w2:subj} walks in. {*A:w3:subj} walks in.")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "A wolf walks in. Another wolf walks in. A 3rd wolf walks in."
    );

    // Test with_pinned_entity
    let ctx_pinned = RenderContext::new("viewer")
        .with_entity("w1", &w1)
        .with_pinned_entity("w1");

    assert_eq!(ctx_pinned.recent_entities.borrow().len(), 1);
    assert!(
        ctx_pinned.recent_entities.borrow()[0]
            .flags
            .contains(crate::models::RecentEntityFlags::IS_PINNED)
    );

    // Test without_anaphora
    let ctx_forgotten = ctx_pinned.without_anaphora("w1");
    assert_eq!(ctx_forgotten.recent_entities.borrow().len(), 0);
}

#[test]
fn test_in_place_anaphora_mutations() {
    let m1 = MockEntity {
        id: "m1".to_string(),
        name: "Bob".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let ctx = RenderContext::new("viewer").with_entity("bob", &m1);

    let ctx = ctx.with_last_mentioned("bob");
    assert!(
        !ctx.recent_entities
            .borrow()
            .last()
            .expect("Expected entity in recent memory")
            .flags
            .contains(crate::models::RecentEntityFlags::IS_PINNED)
    );

    ctx.pin_anaphora("bob");
    assert!(
        ctx.recent_entities
            .borrow()
            .last()
            .expect("Expected entity in recent memory")
            .flags
            .contains(crate::models::RecentEntityFlags::IS_PINNED)
    );

    ctx.unpin_anaphora("bob");
    assert!(
        !ctx.recent_entities
            .borrow()
            .last()
            .expect("Expected entity in recent memory")
            .flags
            .contains(crate::models::RecentEntityFlags::IS_PINNED)
    );
}

#[test]
fn test_pinned_and_forgotten_anaphora() {
    let m1 = MockEntity {
        id: "m1".to_string(),
        name: "Bob".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let m2 = MockEntity {
        id: "m2".to_string(),
        name: "Tom".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let m3 = MockEntity {
        id: "m3".to_string(),
        name: "Jim".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let m4 = MockEntity {
        id: "m4".to_string(),
        name: "Dan".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // Limit is 2. We pin Bob, then push him past the limit by introducing Tom, Jim, and Dan.
    let ctx = RenderContext::new("viewer")
        .with_anaphora_limit(2)
        .with_entity("bob", &m1)
        .with_entity("tom", &m2)
        .with_entity("jim", &m3)
        .with_entity("dan", &m4)
        .with_pinned_entity("bob"); // Bob is pinned to memory!

    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:tom:subj} arrives.")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:jim:subj} arrives.")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:dan:subj} arrives.")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");

    // Dan was the last mentioned. Bob is NOT the last mentioned, but is pinned in memory.
    // Memory has Dan (Male) and Bob (Male). Both are male. Bob's pronoun "He" is correctly recognized as ambiguous!
    let out_bob = PerspectiveEngine::render(
        &cache
            .get_or_compile("{a:bob:Subj} [bob:smile].")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");
    assert_eq!(out_bob, "Bob smiles.");

    // Now we explicitly forget Dan. The only male in memory is Bob.
    ctx.forget_anaphora("dan");

    // Now "He" for Bob should be unambiguous!
    let out_bob2 = PerspectiveEngine::render(
        &cache
            .get_or_compile("{a:bob:Subj} [bob:wave].")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");
    assert_eq!(out_bob2, "He waves.");

    // Now unpin Bob and let him naturally evict.
    ctx.unpin_anaphora("bob");

    // Add Tom, Jim, Dan to push Bob out of the LRU cache
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:tom:subj} arrives.")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:jim:subj} arrives.")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:dan:subj} arrives.")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");

    // Now memory should be [Jim, Dan]. Bob is gone.
    // Requesting Bob's pronoun will just treat him as a newly introduced entity and print his name.
    let out_bob3 = PerspectiveEngine::render(
        &cache
            .get_or_compile("{a:bob:Subj} [bob:nod].")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");
    assert_eq!(out_bob3, "Bob nods.");
}

#[test]
fn test_all_pinned_entities_exceed_limit() {
    let m1 = MockEntity {
        id: "m1".to_string(),
        name: "Bob".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let m2 = MockEntity {
        id: "m2".to_string(),
        name: "Tom".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let m3 = MockEntity {
        id: "m3".to_string(),
        name: "Jim".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // Limit is 2. We pin 3 entities.
    let ctx = RenderContext::new("viewer")
        .with_anaphora_limit(2)
        .with_entity("bob", &m1)
        .with_entity("tom", &m2)
        .with_entity("jim", &m3)
        .with_pinned_entity("bob")
        .with_pinned_entity("tom")
        .with_pinned_entity("jim");

    // Verify that all 3 pinned entities are retained, exceeding the limit of 2.
    assert_eq!(ctx.recent_entities.borrow().len(), 3);

    // Add an unpinned entity to trigger another eviction check
    let m4 = MockEntity {
        id: "m4".to_string(),
        name: "Dan".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let ctx = ctx.with_entity("dan", &m4);
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:dan:subj} arrives.")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");

    // The limit is still 2. The anaphora check sees 4 entities, but 3 are pinned.
    // It will evict Dan immediately because he is the only unpinned entity.
    assert_eq!(ctx.recent_entities.borrow().len(), 3);

    // Verify Dan was evicted and the remaining 3 are the pinned ones
    let keys: Vec<String> = ctx
        .recent_entities
        .borrow()
        .iter()
        .map(|r| r.key.clone())
        .collect();
    assert!(!keys.contains(&"dan".to_string()));
    assert!(keys.contains(&"bob".to_string()));
    assert!(keys.contains(&"tom".to_string()));
    assert!(keys.contains(&"jim".to_string()));
}

#[test]
fn test_anaphora_viewer_exemption() {
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
    // The goblin takes the anaphora focus in the middle of the sentence.
    let template = cache
        .get_or_compile(
            "{*A:Source:subj} [source:hit] {*the:target:obj}, then {a:source:subj} [source:step] back.",
        )
        .expect("Failed to compile template");

    // 1. Bystander (Director Stance)
    // Because "goblin" is Neutral and "Aldran" is Male, there is no pronoun collision.
    // The engine safely uses "he" for Aldran despite the goblin taking focus.
    let out_director = render_msg!("char_3", &template, "source" => &player, "target" => &goblin)
        .expect("Failed to render template");
    assert_eq!(out_director, "Aldran hits the goblin, then he steps back.");

    // 2. Player (Actor Stance)
    // Even though "goblin" took focus, "you" is immune to anaphora ambiguity. It stays "you".
    let out_actor = render_msg!("char_1", &template, "source" => &player, "target" => &goblin)
        .expect("Failed to render template");
    assert_eq!(out_actor, "You hit the goblin, then you step back.");
}

#[test]
fn test_empty_anaphora_extraction_and_injection() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    // 1. Extract from a brand new, empty context
    let ctx_empty = RenderContext::new("viewer");
    let empty_state = ctx_empty.extract_anaphora();

    assert!(empty_state.last_mentioned.is_none());
    assert!(empty_state.active_subject.is_none());
    assert!(empty_state.recent_entities.is_empty());

    // 2. Inject into a new context and verify behavior
    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{a:source:Subj} [source:nod].")
        .expect("Failed to compile template");

    let ctx_injected = RenderContext::new("viewer")
        .with_entity("source", &player)
        .with_anaphora(empty_state);

    // Because the state is completely empty, it should safely fall back to the full name instead of using a pronoun.
    let out =
        PerspectiveEngine::render(&template, &ctx_injected).expect("Failed to render template");
    assert_eq!(out, "Aldran nods.");
}

#[test]
fn test_with_last_mentioned_preserves_pinned_status() {
    let bob = MockEntity {
        id: "m1".to_string(),
        name: "Bob".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let tom = MockEntity {
        id: "m2".to_string(),
        name: "Tom".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    let ctx = RenderContext::new("viewer")
        .with_anaphora_limit(1) // Extremely strict limit
        .with_entity("bob", &bob)
        .with_entity("tom", &tom)
        .with_pinned_entity("bob") // Bob is pinned
        .with_last_mentioned("bob"); // Triggers the LRU refresh path

    // Render Tom to force an eviction check (Memory is now [Bob, Tom])
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:tom:subj} arrives.")
            .expect("Failed to compile template"),
        &ctx,
    )
    .expect("Failed to render template");

    // If `with_last_mentioned` accidentally cleared Bob's flags, he would have been evicted as the oldest.
    // Because his IS_PINNED flag was preserved, Tom (the newest but unpinned) is evicted instead.
    assert_eq!(ctx.recent_entities.borrow()[0].key, "bob");
}

#[test]
fn test_with_anaphora_preserves_pinned_status() {
    let bob = MockEntity {
        id: "m1".to_string(),
        name: "Bob".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let tom = MockEntity {
        id: "m2".to_string(),
        name: "Tom".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    // 1. Pin an entity and extract the state
    let ctx1 = RenderContext::new("viewer")
        .with_entity("bob", &bob)
        .with_pinned_entity("bob");
    let state = ctx1.extract_anaphora();

    // 2. Inject into a new context with a strict eviction limit
    let cache = TemplateCache::new(100);
    let ctx2 = RenderContext::new("viewer")
        .with_anaphora_limit(1)
        .with_entity("bob", &bob)
        .with_entity("tom", &tom)
        .with_anaphora(state);

    // Render Tom to force an eviction check. Memory becomes [Bob, Tom] and then limits are enforced.
    let _ = PerspectiveEngine::render(
        &cache
            .get_or_compile("{*A:tom:subj} arrives.")
            .expect("Failed to compile template"),
        &ctx2,
    )
    .expect("Failed to render template");

    // Because the state extraction/injection preserved Bob's IS_PINNED flag, Tom is evicted instead.
    assert_eq!(ctx2.recent_entities.borrow().len(), 1);
    assert_eq!(ctx2.recent_entities.borrow()[0].key, "bob");
}

#[test]
fn test_anaphora_with_stances() {
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
            "{*A:source:subj} [source:hit] {*the:target:obj}. {a:target:Subj} [target:hit] {a:source:obj} back.",
        )
        .expect("Failed to compile template");

    // First Person
    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player)
        .with_entity("target", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first).expect("Failed to render template"),
        "I hit the goblin. It hits me back."
    );

    // Third Person
    let ctx_third = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::ThirdPerson)
        .with_entity("source", &player)
        .with_entity("target", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_third).expect("Failed to render template"),
        "Aldran hits the goblin. It hits him back."
    );
}

#[test]
fn test_all_pronoun_cases_with_stances() {
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

    let party = crate::models::GroupEntity {
        members: vec![&player, &ally],
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("{a:source:Subj} [source:defend] {a:source:reflex}. {*The:target:subj} [target:strike] {a:source:obj}. It is {a:source:poss} fight, the victory is {:source:abs_poss}!")
        .expect("Failed to compile template");

    // 1. First Person Singular
    let ctx_first = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player)
        .with_entity("target", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first).expect("Failed to render template"),
        "I defend myself. The goblin strikes me. It is my fight, the victory is mine!"
    );

    // 2. First Person Plural
    let ctx_first_plural = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &party)
        .with_entity("target", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_first_plural).expect("Failed to render template"),
        "We defend ourselves. The goblin strikes us. It is our fight, the victory is ours!"
    );

    // 3. Second Person Singular
    let ctx_second = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::SecondPerson)
        .with_entity("source", &player)
        .with_entity("target", &goblin);
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_second).expect("Failed to render template"),
        "You defend yourself. The goblin strikes you. It is your fight, the victory is yours!"
    );

    // 4. Third Person Singular
    // By seeding the context with "source", we suppress the anaphora fallback to explicitly test 3rd-person pronouns.
    let ctx_third = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::ThirdPerson)
        .with_entity("source", &player)
        .with_entity("target", &goblin)
        .with_last_mentioned("source");
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_third).expect("Failed to render template"),
        "He defends himself. The goblin strikes him. It is his fight, the victory is his!"
    );
}

#[test]
fn test_suppress_anaphora_upgrades() {
    let wolf1 = MockEntity {
        id: "mob_1".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolf2 = MockEntity {
        id: "mob_2".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // 1. Suppress article upgrade
    let t_article = cache
        .get_or_compile("{*A:source:subj} [source:walk] in. {*!A:source:subj} [source:howl].")
        .expect("Failed to compile template");
    let ctx1 = RenderContext::new("char_1").with_entity("source", &wolf1);
    assert_eq!(
        PerspectiveEngine::render(&t_article, &ctx1).expect("Failed to render template"),
        "A wolf walks in. A wolf howls." // The ! prefix successfully suppressed "The"
    );

    // 2. Suppress pronoun fallback (Ambiguity between wolf1 and wolf2)
    let t_pronoun = cache
        .get_or_compile("{*A:source:subj} [source:walk] in. {*A:other:subj} [other:walk] in. {a:source:!Subj} [source:howl].")
        .expect("Failed to compile template");
    let ctx2 = RenderContext::new("char_1")
        .with_entity("source", &wolf1)
        .with_entity("other", &wolf2);

    // Because of `!`, the engine forces "It howls." instead of falling back to "The wolf howls."
    assert_eq!(
        PerspectiveEngine::render(&t_pronoun, &ctx2).expect("Failed to render template"),
        "A wolf walks in. Another wolf walks in. It howls."
    );
}

#[test]
fn test_pronoun_fallback_article_upgrade() {
    let wolf1 = MockEntity {
        id: "mob_1".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let wolf2 = MockEntity {
        id: "mob_2".to_string(),
        name: "wolf".to_string(), // Name collision with wolf1!
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let slime = MockEntity {
        id: "mob_3".to_string(),
        name: "slime".to_string(), // Pronoun collision (Neutral), but unique name!
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // Scenario 1: Unseen -> "A"
    // The wolf hasn't been introduced, so the pronoun falls back to "A wolf".
    let t1 = cache
        .get_or_compile("{a:source:Subj} [source:howl].")
        .unwrap();
    let ctx1 = RenderContext::new("char_1").with_entity("source", &wolf1);
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).unwrap(),
        "A wolf howls."
    );

    // Scenario 2: Pronoun Collision, but Unique Name -> "The"
    // The slime makes the pronoun "It" ambiguous. It falls back to "A", which sees it's unique and upgrades to "The".
    let t2 = cache
        .get_or_compile(
            "{*A:source:subj} and {*a:slime:subj} arrive. {a:source:Subj} [source:howl].",
        )
        .unwrap();
    let ctx2 = RenderContext::new("char_1")
        .with_entity("source", &wolf1)
        .with_entity("slime", &slime);
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx2).unwrap(),
        "A wolf and a slime arrive. The wolf howls."
    );

    // Scenario 3: Pronoun Collision AND Name Collision -> "A"
    // The second wolf makes the pronoun "It" ambiguous. It falls back to "A", but sees "wolf" is no longer a unique description, so it stays "A".
    let t3 = cache
        .get_or_compile(
            "{*a:source:subj} and {*a:other:subj} arrive. {a:source:Subj} [source:howl].",
        )
        .unwrap();
    let ctx3 = RenderContext::new("char_1")
        .with_entity("source", &wolf1)
        .with_entity("other", &wolf2);
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx3).unwrap(),
        "A wolf and another wolf arrive. The first wolf howls."
    );
}

#[test]
fn test_auto_reflexive_pronoun_upgrade() {
    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // The template uses `obj`, which naturally outputs "me" in 1st person, or "him" in 3rd.
    // But because `target` is the active subject (source = target), it should upgrade to "myself" / "himself".
    let template = cache
        .get_or_compile("{*A:source:subj} [source:hit] {a:target:obj} with {a:source:poss} sword and {a:target:subj} [target:scream]!")
        .unwrap();

    let ctx_actor = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("source", &player)
        .with_entity("target", &player);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_actor).unwrap(),
        "I hit myself with my sword and I scream!"
    );

    let ctx_director = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_entity("target", &player);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_director).unwrap(),
        "Aldran hits himself with his sword and he screams!"
    );
}

#[test]
fn test_first_person_objective_anaphora_fallback() {
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
        .get_or_compile("The trap [strike] {a:target:obj}!")
        .unwrap();

    // 1. Viewer is the target -> engine natively resolves to the objective pronoun
    let ctx_viewer = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("target", &player);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_viewer).unwrap(),
        "The trap strikes me!"
    );

    // 2. NPC is the target -> Anaphora intercepts the pronoun and falls back to an indefinite noun!
    let ctx_npc = RenderContext::new("char_1")
        .with_stance(crate::models::ActorStance::FirstPerson)
        .with_entity("target", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx_npc).unwrap(),
        "The trap strikes a goblin!"
    );
}

#[test]
fn test_anaphora_dynamic_entity_mutation() {
    let singular_wolf = MockEntity {
        id: "mob_1".into(),
        name: "wolf".into(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    // Mimics the same entity transforming, or a GroupEntity gaining a member
    let plural_wolves = MockEntity {
        id: "mob_1".into(),
        name: "wolves".into(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // 1. Initial State: Singular
    let ctx = RenderContext::new("viewer").with_entity("target", &singular_wolf);

    // Introduce the entity to memory
    let t1 = cache
        .get_or_compile("{*A:target:subj} [target:howl].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "A wolf howls."
    );

    // Verify it evaluates as a singular pronoun
    let t2 = cache
        .get_or_compile("{A:target:Subj} [target:bite].")
        .unwrap();
    assert_eq!(PerspectiveEngine::render(&t2, &ctx).unwrap(), "It bites.");

    // 2. Mutate State: Replace the entity with its plural version
    // We do NOT clear the anaphora memory!
    let ctx_mutated = ctx.with_entity("target", &plural_wolves);

    // The anaphora memory should refresh the cached grammatical flags (Singular -> Plural)
    // upon the next interaction, dynamically switching the pronoun and verb conjugation!
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx_mutated).unwrap(),
        "They bite."
    );
}

#[test]
fn test_anaphora_dynamic_epistemological_mutation() {
    let known_aldran = MockEntity {
        id: "char_1".into(),
        name: "Aldran".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let disguised_aldran = MockEntity {
        id: "char_1".into(),
        name: "tall man".into(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: false, // No longer a proper noun!
    };

    let cache = TemplateCache::new(100);

    // 1. Initial State: Known Identity
    let ctx = RenderContext::new("viewer").with_entity("target", &known_aldran);

    // First mention: evaluates to proper noun (suppresses article)
    let t1 = cache
        .get_or_compile("{*a:target:subj} [target:smile].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).unwrap(),
        "Aldran smiles."
    );

    // Second mention: uses pronoun
    let t2 = cache
        .get_or_compile("{a:target:Subj} [target:wave].")
        .unwrap();
    assert_eq!(PerspectiveEngine::render(&t2, &ctx).unwrap(), "He waves.");

    // 2. Mutate State: Don a disguise!
    // We replace the entity with the disguised version, BUT keep the anaphora memory intact.
    let ctx_disguised = ctx.with_entity("target", &disguised_aldran);

    // Force a noun fallback (using the `*` "prefer noun" modifier to bypass the pronoun).
    // Since it's still in memory, the indefinite article should safely upgrade to "The",
    // and it should dynamically query the new live name ("tall man")!
    let t3 = cache
        .get_or_compile("{*a:target:subj} [target:flee].")
        .unwrap();
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx_disguised).unwrap(),
        "The tall man flees."
    );
}
