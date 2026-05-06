use super::common::MockEntity;
use crate::cache::TemplateCache;
use crate::engine::PerspectiveEngine;
use crate::models::{Gender, GroupEntity, RenderContext};

#[test]
#[cfg(feature = "ansi")]
fn test_typography_skips_ansi() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let template_ansi = cache
        .get_or_compile("\x1b[31m{*the:source:subj} [source:attack].")
        .expect("Failed to compile template");
    let output_ansi = render_msg!("char_2", &template_ansi, "source" => &goblin)
        .expect("Failed to render template");
    assert_eq!(output_ansi, "\x1b[31mThe goblin attacks.");
}

#[test]
#[cfg(feature = "mxp")]
fn test_typography_skips_mxp() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let template_mxp = cache
        .get_or_compile("<COLOR red>{*a:source:subj} [source:approach].")
        .expect("Failed to compile template");
    let output_mxp = render_msg!("char_2", &template_mxp, "source" => &goblin)
        .expect("Failed to render template");
    assert_eq!(output_mxp, "<COLOR red>A goblin approaches.");
}

#[test]
#[cfg(feature = "mxp")]
fn test_typography_mxp_with_periods() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // The period inside `red.blue` should be safely ignored and not trigger a sentence boundary.
    let template_mxp = cache
        .get_or_compile("a <COLOR red.blue>fierce {*source} [source:approach].")
        .expect("Failed to compile template");
    let output_mxp = render_msg!("char_2", &template_mxp, "source" => &goblin)
        .expect("Failed to render template");
    assert_eq!(output_mxp, "A <COLOR red.blue>fierce goblin approaches.");
}

#[test]
#[cfg(feature = "msp")]
fn test_typography_skips_msp() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let template_msp = cache
        .get_or_compile("!!SOUND(roar.wav){*the:source:subj} [source:roar].")
        .expect("Failed to compile template");
    let output_msp = render_msg!("char_2", &template_msp, "source" => &goblin)
        .expect("Failed to render template");
    assert_eq!(output_msp, "!!SOUND(roar.wav)The goblin roars.");
}

#[test]
#[cfg(all(feature = "ansi", feature = "mxp"))]
fn test_typography_skips_mixed() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let template_mixed = cache
        .get_or_compile(
            "\x1b[1;32m<SEND href=\"look\">{*the:source:subj} [source:wave].\x1b[0m <COLOR blue>{a:source:subj} [source:smile].",
        )
        .expect("Failed to compile template");

    let output_mixed = render_msg!("char_2", &template_mixed, "source" => &goblin)
        .expect("Failed to render template");
    assert_eq!(
        output_mixed,
        "\x1b[1;32m<SEND href=\"look\">The goblin waves.\x1b[0m <COLOR blue>It smiles."
    );
}

#[test]
#[cfg(all(feature = "mxp", feature = "msp"))]
fn test_compiler_skips_tags() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // If the compiler doesn't skip the MXP and MSP tags, it will mistakenly parse
    // the `[` and `{` inside them as verb or entity tags, resulting in a syntax error.
    let template = cache
        .get_or_compile("<SEND HREF=\"[look]\">{*the:source:subj} triggers a !!SOUND({roar})!")
        .expect("Failed to compile template");

    let output =
        render_msg!("char_2", &template, "source" => &goblin).expect("Failed to render template");
    assert_eq!(
        output,
        "<SEND HREF=\"[look]\">The goblin triggers a !!SOUND({roar})!"
    );
}

#[test]
#[cfg(feature = "ansi")]
fn test_compiler_skips_ansi() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // If the compiler doesn't skip the ANSI OSC sequence, it will mistakenly parse
    // the `[` and `{` inside the URL as verb or entity tags, resulting in a syntax error.
    let template = cache
        .get_or_compile("\x1b]8;;https://example.com/?q={123}&v=[456]\x07{*the:source:subj} [source:attack].\x1b]8;;\x07")
        .expect("Failed to compile template");

    let output =
        render_msg!("char_2", &template, "source" => &goblin).expect("Failed to render template");
    assert_eq!(
        output,
        "\x1b]8;;https://example.com/?q={123}&v=[456]\x07The goblin attacks.\x1b]8;;\x07"
    );
}

#[test]
#[cfg(feature = "ansi")]
fn test_unterminated_ansi_fallback() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // 1. Unterminated OSC sequence
    // Since there is no terminator (\x07), it falls back to literal text.
    // The {the:source} tag is parsed successfully rather than being swallowed.
    let template = cache
        .get_or_compile("\x1b]8;;unterminated {*the:source:subj} [source:attack].")
        .unwrap();

    let output = render_msg!("char_2", &template, "source" => &goblin).unwrap();
    // The 'u' in 'unterminated' is capitalized by the post-processor because
    // the sequence was treated as literal text.
    assert_eq!(output, "\x1b]8;;Unterminated the goblin attacks.");

    // 2. Unterminated CSI sequence
    // Falls back to literal text, but the `[` immediately triggers the verb tag parser.
    // Since there's no `]`, it safely fails with a syntax error instead of skipping the string.
    let err = crate::engine::Template::compile("\x1b[31").unwrap_err();
    assert_eq!(err, "Unclosed verb tag starting at index 1");
}

#[test]
#[cfg(feature = "mxp")]
fn test_unterminated_mxp_fallback() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // Unterminated MXP sequence (missing closing '>')
    // Falls back to literal text. The {the:source} tag is parsed successfully.
    let template = cache
        .get_or_compile("<color red unterminated {*the:source:subj} [source:attack].")
        .expect("Failed to compile template");

    let output =
        render_msg!("char_2", &template, "source" => &goblin).expect("Failed to render template");
    assert_eq!(output, "<Color red unterminated the goblin attacks.");
}

#[test]
#[cfg(feature = "msp")]
fn test_unterminated_msp_fallback() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // Unterminated MSP sequence (missing closing ')')
    // Falls back to literal text. The {the:source} tag is parsed successfully.
    let template = cache
        .get_or_compile("!!SOUND(roar.wav unterminated {*the:source:subj} [source:attack].")
        .expect("Failed to compile template");

    let output =
        render_msg!("char_2", &template, "source" => &goblin).expect("Failed to render template");
    assert_eq!(output, "!!SOUND(roar.wav unterminated the goblin attacks.");
}

#[test]
fn test_escaped_tags() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    // Verifies that escaped braces, brackets, and backslashes are cleanly bypassed
    let template = cache
        .get_or_compile(r"some \{escaped\} and \[tags\]. \\{*The:source:subj}")
        .expect("Failed to compile template");

    let output =
        render_msg!("char_2", &template, "source" => &goblin).expect("Failed to render template");
    assert_eq!(output, r"Some {escaped} and [tags]. \The goblin");
}

#[test]
fn test_quoted_text_capitalization() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let player = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // Scenario 1: Quote at the start of the sentence
    // The typography engine correctly skips the `"` and capitalizes the first letter.
    let template1 = cache
        .get_or_compile("\"{*the:source:subj} [source:be] a fool!\"")
        .unwrap();
    let output1 = render_msg!("char_2", &template1, "source" => &goblin).unwrap();
    assert_eq!(output1, "\"The goblin is a fool!\"");

    // Scenario 2: Quote in the middle of a sentence with a proper noun
    // Proper nouns are returned already capitalized by `display_name_for`.
    let template2 = cache
        .get_or_compile("{*the:source:subj} [source:say], \"{*A:target:subj} [target:be] a fool!\"")
        .unwrap();
    let output2 =
        render_msg!("char_2", &template2, "source" => &goblin, "target" => &player).unwrap();
    assert_eq!(output2, "The goblin says, \"Aldran is a fool!\"");

    // Scenario 3: Quote in the middle of a sentence with a common noun (Capitalized Article)
    // By using {The:source}, we force the engine to capitalize the article regardless of the segmenter.
    let template3 = cache
        .get_or_compile("{*A:target:subj} [target:say], \"{*The:source:subj} [source:be] a fool!\"")
        .unwrap();
    let output3 =
        render_msg!("char_2", &template3, "source" => &goblin, "target" => &player).unwrap();
    assert_eq!(output3, "Aldran says, \"The goblin is a fool!\"");

    // Scenario 4: Indefinite article capitalization
    let template4 = cache
        .get_or_compile(
            "{*A:target:subj} [target:yell], \"{*A:source:subj} [source:be] approaching!\"",
        )
        .unwrap();
    let output4 =
        render_msg!("char_2", &template4, "source" => &goblin, "target" => &player).unwrap();
    assert_eq!(output4, "Aldran yells, \"A goblin is approaching!\"");
}

#[test]
fn test_mid_sentence_capitalization_overrides() {
    let disguised = MockEntity {
        id: "char_1".to_string(),
        name: "Aldran".to_string(), // will be masked as "tall man" to strangers
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let cache = TemplateCache::new(100);

    // 1. Force capitalizing a disguised common noun directly
    // The segmenter will capitalize "you", and `{Source}` overrides the disguise's lowercase.
    let template1 = cache
        .get_or_compile("you point at {*the:Source:obj}.")
        .unwrap();
    let out1 = render_msg!("stranger_1", &template1, "source" => &disguised).unwrap();
    assert_eq!(out1, "You point at the Tall man.");

    // 2. Force capitalizing a pronoun mid-sentence
    let template2 = cache
        .get_or_compile("you watch as {a:source:Subj} [source:fall].")
        .unwrap();
    let ctx2 = RenderContext::new("stranger_1")
        .with_entity("source", &disguised)
        .with_last_mentioned("source"); // Seed the context so it prints the pronoun!
    let out2 = PerspectiveEngine::render(&template2, &ctx2).unwrap();
    assert_eq!(out2, "You watch as He falls.");

    // 3. Verbs already support this organically
    // We use `{the:source}` to provide the article, and a sentence structure
    // where a conjugated verb is grammatically correct mid-sentence.
    let template3 = cache
        .get_or_compile("they say {*the:source:subj} [source:Smile] often.")
        .unwrap();
    let out3 = render_msg!("stranger_1", &template3, "source" => &disguised).unwrap();
    assert_eq!(out3, "They say the tall man Smiles often.");
}

#[test]
#[cfg(feature = "ansi")]
fn test_ansi_colored_possessive_suffixes() {
    let colored_boss = MockEntity {
        id: "mob_1".to_string(),
        name: "\x1b[31mboss\x1b[0m".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let colored_wolves = MockEntity {
        id: "mob_2".to_string(),
        name: "\x1b[32mwolves\x1b[0m".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };
    let colored_goblin = MockEntity {
        id: "mob_3".to_string(),
        name: "\x1b[33mgoblin\x1b[0m".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("You take {*the:target's:poss} gold.")
        .expect("Failed to compile template");

    // Singular common noun ending in 's' with ANSI code at the end -> expects 's
    let out_boss = render_msg!("char_2", &template, "target" => &colored_boss)
        .expect("Failed to render template");
    assert_eq!(out_boss, "You take the \x1b[31mboss\x1b[0m's gold.");

    // Plural common noun ending in 's' with ANSI code at the end -> expects '
    let out_wolves = render_msg!("char_2", &template, "target" => &colored_wolves)
        .expect("Failed to render template");
    assert_eq!(out_wolves, "You take the \x1b[32mwolves\x1b[0m' gold.");

    // Regular singular common noun with ANSI code at the end -> expects 's
    let out_goblin = render_msg!("char_2", &template, "target" => &colored_goblin)
        .expect("Failed to render template");
    assert_eq!(out_goblin, "You take the \x1b[33mgoblin\x1b[0m's gold.");
}

#[test]
fn test_trailing_whitespace_possessive_suffixes() {
    let wolves_spaced = MockEntity {
        id: "mob_1".to_string(),
        name: "wolves ".to_string(),
        gender: Gender::Plural,
        is_plural: true,
        is_proper_noun: false,
    };
    let boss_spaced = MockEntity {
        id: "mob_2".to_string(),
        name: "boss   ".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("You take {*the:target's:poss} gold.")
        .expect("Failed to compile template");

    // Plural ending in 's' followed by space -> expects '
    let out_wolves = render_msg!("char_2", &template, "target" => &wolves_spaced)
        .expect("Failed to render template");
    assert_eq!(out_wolves, "You take the wolves ' gold.");

    // Singular ending in 's' followed by multiple spaces -> expects 's
    let out_boss = render_msg!("char_2", &template, "target" => &boss_spaced)
        .expect("Failed to render template");
    assert_eq!(out_boss, "You take the boss   's gold.");
}

#[test]
fn test_manual_sentence_boundaries() {
    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("viewer");

    // 1. [SB] forces a sentence boundary
    let t_sb = cache
        .get_or_compile("wait, [SB]what?")
        .expect("Failed to compile template");
    let out_sb = PerspectiveEngine::render(&t_sb, &ctx).expect("Failed to render template");
    assert_eq!(out_sb, "Wait, What?");

    // 2. [NO_SB] suppresses a sentence boundary
    let t_no_sb = cache
        .get_or_compile("apples vs.[NO_SB] oranges.")
        .expect("Failed to compile template");
    let out_no_sb = PerspectiveEngine::render(&t_no_sb, &ctx).expect("Failed to render template");
    assert_eq!(out_no_sb, "Apples vs. oranges.");

    // 3. Ensuring tags don't output stray whitespace and chain well
    let t_combined = cache
        .get_or_compile("one.[NO_SB] two[SB] three.")
        .expect("Failed to compile template");
    let out_combined =
        PerspectiveEngine::render(&t_combined, &ctx).expect("Failed to render template");
    assert_eq!(out_combined, "One. two Three.");
}

#[test]
fn test_all_caps_mode() {
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

    // Test 1: Full uppercase tag for an object pronoun (fallback to noun)
    let t1 = cache
        .get_or_compile("{*A:source:subj} [source:hit] {THE:TARGET:OBJ}!")
        .expect("Failed to compile template");
    let ctx1 = RenderContext::new("char_2")
        .with_entity("source", &player)
        .with_entity("target", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx1).expect("Failed to render template"),
        "Aldran hits THE GOBLIN!"
    );

    // Test 2: Full uppercase tag for a verb
    let t2 = cache
        .get_or_compile("{*The:target:subj} [TARGET:ATTACK]!")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx1).expect("Failed to render template"),
        "The goblin ATTACKS!"
    );

    // Test 3: Possessives
    let t3 = cache
        .get_or_compile("{SOURCE'S} sword [source:glow].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t3, &ctx1).expect("Failed to render template"),
        "ALDRAN'S sword glows."
    );

    // Test 4: Pronouns
    let ctx2 = ctx1.with_last_mentioned("target");
    let t4 = cache
        .get_or_compile("{TARGET:SUBJ} [target:fall].")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t4, &ctx2).expect("Failed to render template"),
        "IT falls."
    );
}

#[test]
#[cfg(feature = "ansi")]
fn test_all_caps_skips_ansi() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "\x1b[31mgoblin\x1b[0m".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let t1 = cache
        .get_or_compile("{*THE:SOURCE:SUBJ} [SOURCE:ATTACK]!")
        .expect("Failed to compile template");
    let ctx = RenderContext::new("char_2").with_entity("source", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "THE \x1b[31mGOBLIN\x1b[0m ATTACKS!"
    );
}

#[test]
#[cfg(feature = "mxp")]
fn test_all_caps_skips_mxp() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: r#"<SEND HREF="look at goblin">goblin</SEND>"#.to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let t1 = cache
        .get_or_compile("{*THE:SOURCE:SUBJ} [SOURCE:ATTACK]!")
        .expect("Failed to compile template");
    let ctx = RenderContext::new("char_2").with_entity("source", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        r#"THE <SEND HREF="look at goblin">GOBLIN</SEND> ATTACKS!"#
    );
}

#[test]
#[cfg(feature = "msp")]
fn test_all_caps_skips_msp() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "!!SOUND(roar.wav)goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);

    let t1 = cache
        .get_or_compile("{*THE:SOURCE:SUBJ} [SOURCE:ATTACK]!")
        .expect("Failed to compile template");
    let ctx = RenderContext::new("char_2").with_entity("source", &goblin);

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "THE !!SOUND(roar.wav)GOBLIN ATTACKS!"
    );
}

#[test]
fn test_distributed_group_article_suppression_after_possessive() {
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
    let wolf = MockEntity {
        id: "mob_2".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let party = GroupEntity::new(vec![&goblin, &wolf]);
    let cache = TemplateCache::new(100);

    let template = cache
        .get_or_compile("{*A:source's} {the:party:obj}.")
        .expect("Failed to compile template");

    let output = render_msg!("char_2", &template, "source" => &aldran, "party" => &party)
        .expect("Failed to render template");

    // The possessive "Aldran's" should suppress the article for BOTH the goblin and the wolf.
    assert_eq!(output, "Aldran's goblin and wolf.");
}

#[test]
fn test_reflexive_group_capitalization() {
    let bob = MockEntity {
        id: "char_2".to_string(),
        name: "Bob".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };
    let charlie = MockEntity {
        id: "char_3".to_string(),
        name: "Charlie".to_string(),
        gender: Gender::Male,
        is_plural: false,
        is_proper_noun: true,
    };

    let party = GroupEntity::new(vec![&charlie, &bob]);
    let cache = TemplateCache::new(100);

    let template = cache
        .get_or_compile("{Bob:subj} [bob:defend] {The:Party:obj}.")
        .expect("Failed to compile template");
    let ctx = RenderContext::new("char_1")
        .with_entity("bob", &bob)
        .with_entity("party", &party);

    // Because Charlie is the `first_visible_item` in the party vector, he absorbs the
    // capitalization flags from `{The:Party:obj}`. Bob is the second item, so his
    // reflexive pronoun "himself" remains lowercase. If the vector order was reversed,
    // the output would be "Bob defends Himself and Charlie."
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx).expect("Failed to render template"),
        "Bob defends Charlie and himself."
    );
}

#[test]
fn test_reflexive_first_member_common_noun() {
    let wolf = MockEntity {
        id: "mob_1".to_string(),
        name: "wolf".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };
    let goblin = MockEntity {
        id: "mob_2".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let party = GroupEntity::new(vec![&wolf, &goblin]);
    let cache = TemplateCache::new(100);

    // 1. The wolf is the active subject, and it is the FIRST member of the party.
    // It gets reflexively replaced ("Itself") and absorbs the capitalization from {The:party:obj}.
    // The goblin is the second member, so its article remains lowercase ("the goblin").
    let template = cache
        .get_or_compile("{*The:wolf:subj} [wolf:defend] {The:party:obj}.")
        .expect("Failed to compile template");

    let ctx1 = RenderContext::new("char_1")
        .with_entity("wolf", &wolf)
        .with_entity("party", &party);

    assert_eq!(
        PerspectiveEngine::render(&template, &ctx1).expect("Failed to render template"),
        "The wolf defends Itself and the goblin."
    );

    // 2. Reverse the group order so the goblin is first.
    let party_reversed = GroupEntity::new(vec![&goblin, &wolf]);
    let ctx2 = RenderContext::new("char_1")
        .with_entity("wolf", &wolf)
        .with_entity("party", &party_reversed);

    // Now the goblin is the first member, so its article absorbs the capitalization ("The goblin").
    // The wolf is the second member, so its reflexive pronoun is lowercase ("itself").
    assert_eq!(
        PerspectiveEngine::render(&template, &ctx2).expect("Failed to render template"),
        "The wolf defends The goblin and itself."
    );
}

#[test]
fn test_force_article_distributes_to_group_members() {
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

    let party = GroupEntity::new(vec![&aldran, &bob]);
    let cache = TemplateCache::new(100);

    // Without +the, proper nouns naturally suppress the article.
    let template_normal = cache
        .get_or_compile("{*The:party:subj} [party:arrive].")
        .expect("Failed to compile template");
    let output_normal = render_msg!("char_3", &template_normal, "party" => &party).unwrap();
    assert_eq!(output_normal, "Aldran and Bob arrive.");

    // With +the, the article is forced on ALL proper nouns in the group list.
    let template_forced = cache
        .get_or_compile("{*+The:party:subj} [party:arrive].")
        .expect("Failed to compile template");
    let output_forced = render_msg!("char_3", &template_forced, "party" => &party).unwrap();
    assert_eq!(output_forced, "The Aldran and the Bob arrive.");
}

#[test]
#[cfg(feature = "ansi")]
fn test_is_after_possessive_skips_ansi() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_1").with_entity("target", &goblin);

    // Possessive pronoun followed by ANSI tag before the noun tag
    let template_1 = cache
        .get_or_compile("It is your \x1b[31m{the:target:obj}\x1b[0m!")
        .expect("Failed to compile template");

    // Article "the" should be suppressed by "your", despite the ANSI tag in between
    assert_eq!(
        PerspectiveEngine::render(&template_1, &ctx).expect("Failed to render template"),
        "It is your \x1b[31mgoblin\x1b[0m!"
    );

    ctx.clear_anaphora();

    // Possessive suffix followed by ANSI tag before the noun tag
    let template_2 = cache
        .get_or_compile("Aldran's \x1b[32m{the:target:obj}\x1b[0m.")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&template_2, &ctx).expect("Failed to render template"),
        "Aldran's \x1b[32mgoblin\x1b[0m."
    );
}

#[test]
#[cfg(feature = "mxp")]
fn test_is_after_possessive_skips_mxp() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_1").with_entity("target", &goblin);

    let template_1 = cache
        .get_or_compile("It is your <SEND href=\"look\">{the:target:obj}</SEND>!")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&template_1, &ctx).expect("Failed to render template"),
        "It is your <SEND href=\"look\">goblin</SEND>!"
    );
}

#[test]
#[cfg(feature = "msp")]
fn test_is_after_possessive_skips_msp() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_1").with_entity("target", &goblin);

    let template_1 = cache
        .get_or_compile("It is your !!SOUND(roar.wav){the:target:obj}!")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&template_1, &ctx).expect("Failed to render template"),
        "It is your !!SOUND(roar.wav)goblin!"
    );
}

#[test]
#[cfg(all(feature = "ansi", feature = "mxp"))]
fn test_is_after_possessive_skips_mixed_tags() {
    let goblin = MockEntity {
        id: "mob_1".to_string(),
        name: "goblin".to_string(),
        gender: Gender::Neutral,
        is_plural: false,
        is_proper_noun: false,
    };

    let cache = TemplateCache::new(100);
    let ctx = RenderContext::new("char_1").with_entity("target", &goblin);

    // Combines both ANSI sequence and MXP element between possessive and entity
    let template_1 = cache
        .get_or_compile("It is your \x1b[31m<SEND href=\"look\">{the:target:obj}</SEND>\x1b[0m!")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&template_1, &ctx).expect("Failed to render template"),
        "It is your \x1b[31m<SEND href=\"look\">goblin</SEND>\x1b[0m!"
    );
}
