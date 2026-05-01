#[cfg(test)]
mod tests {
    use crate::cache::TemplateCache;
    use crate::engine::{PerspectiveEngine, Template};
    use crate::models::{Gender, GroupEntity, RenderContext, TemplateEntity};
    use std::borrow::Cow;

    /// A mock entity to represent game objects and characters in our tests.
    struct MockEntity {
        id: String,
        name: String,
        gender: Gender,
        is_plural: bool,
        is_proper_noun: bool,
    }

    impl TemplateEntity for MockEntity {
        fn contains_viewer(&self, viewer_id: &str) -> bool {
            self.id == viewer_id
        }

        fn gender(&self) -> Gender {
            self.gender
        }

        fn is_plural(&self) -> bool {
            self.is_plural
        }

        fn display_name_for<'a>(&'a self, viewer_id: &str) -> Cow<'a, str> {
            if self.contains_viewer(viewer_id) {
                return Cow::Borrowed("you");
            }

            // Simulate an epistemological visibility check:
            // If the viewer is a stranger, hide Aldran's real name.
            if viewer_id == "stranger_1" && self.name == "Aldran" {
                Cow::Borrowed("tall man")
            } else if viewer_id == "stranger_1" && self.name == "the Avengers" {
                Cow::Borrowed("masked heroes")
            } else {
                Cow::Borrowed(&self.name)
            }
        }

        fn is_proper_noun_for(&self, viewer_id: &str) -> bool {
            // If the stranger sees the masked "tall man", it is no longer a proper noun
            if viewer_id == "stranger_1" && (self.name == "Aldran" || self.name == "the Avengers") {
                false
            } else {
                self.is_proper_noun
            }
        }
    }

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
            Template::compile("{source} [source:be] looking around for {source:poss} sword.")
                .unwrap();

        // 1. Actor Stance (Aldran is the viewer)
        let ctx_actor = RenderContext::new("char_1").with_entity("source", &aldran);
        let actor_output = PerspectiveEngine::render(&template, &ctx_actor).unwrap();
        assert_eq!(actor_output, "You are looking around for your sword.");

        // 2. Director Stance (A third-party observer)
        let ctx_director = RenderContext::new("char_2").with_entity("source", &aldran);
        let director_output = PerspectiveEngine::render(&template, &ctx_director).unwrap();
        assert_eq!(director_output, "Aldran is looking around for his sword.");
    }

    #[test]
    fn test_epistemological_masking_and_articles() {
        let aldran = MockEntity {
            id: "char_1".to_string(),
            name: "Aldran".to_string(), // Will be masked as "tall man" to strangers
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: true, // is_proper_noun_for returns false for strangers
        };

        let template = Template::compile("{a:source} [source:approach].").unwrap();

        let ctx_stranger = RenderContext::new("stranger_1").with_entity("source", &aldran);
        let stranger_output = PerspectiveEngine::render(&template, &ctx_stranger).unwrap();

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
        let template_the = cache.get_or_compile("{the:source} is here.").unwrap();
        let output_the = render_msg!("char_2", &template_the, "source" => &goblin).unwrap();
        assert_eq!(output_the, "The goblin is here.");

        // --- SCENARIO 2: `{a:key}` suppressed for a proper noun ---
        let template_a_proper = cache.get_or_compile("{a:source} is here.").unwrap();
        let output_a_proper =
            render_msg!("char_2", &template_a_proper, "source" => &aldran).unwrap();
        assert_eq!(output_a_proper, "Aldran is here.");

        // --- SCENARIO 3: `{the:key}` suppressed for a proper noun ---
        let template_the_proper = cache.get_or_compile("{the:source} is here.").unwrap();
        let output_the_proper =
            render_msg!("char_2", &template_the_proper, "source" => &aldran).unwrap();
        assert_eq!(output_the_proper, "Aldran is here.");

        // --- SCENARIO 4: `{a:key}` suppressed for the viewer ---
        let template_a_viewer = cache
            .get_or_compile("{a:source} [source:be] here.")
            .unwrap();
        let output_a_viewer =
            render_msg!("char_1", &template_a_viewer, "source" => &aldran).unwrap();
        assert_eq!(output_a_viewer, "You are here.");

        // --- SCENARIO 5: `{the:key}` suppressed for the viewer ---
        let template_the_viewer = cache
            .get_or_compile("{the:source} [source:be] here.")
            .unwrap();
        let output_the_viewer =
            render_msg!("char_1", &template_the_viewer, "source" => &aldran).unwrap();
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
        let template_a_plural = cache.get_or_compile("{a:source} assemble!").unwrap();
        let output_a_plural =
            render_msg!("char_2", &template_a_plural, "source" => &avengers).unwrap();
        assert_eq!(output_a_plural, "The Avengers assemble!");

        // Definite article "the" is suppressed, leaving the base name "the Avengers"
        let template_the_plural = cache.get_or_compile("{The:source} assemble!").unwrap();
        let output_the_plural =
            render_msg!("char_2", &template_the_plural, "source" => &avengers).unwrap();
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
        let template_a_common_plural = cache.get_or_compile("{a:source} howl.").unwrap();
        let output_a_common_plural =
            render_msg!("char_2", &template_a_common_plural, "source" => &wolves).unwrap();
        assert_eq!(output_a_common_plural, "Some wolves howl.");

        // Definite article "the" should NOT be suppressed for plural common nouns
        let template_the_common_plural = cache.get_or_compile("{The:source} howl.").unwrap();
        let output_the_common_plural =
            render_msg!("char_2", &template_the_common_plural, "source" => &wolves).unwrap();
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

        let template_a = cache.get_or_compile("{a:source} [source:arrive].").unwrap();
        let template_the = cache
            .get_or_compile("{the:source} [source:arrive].")
            .unwrap();

        // 1. Friend's perspective (knows they are The Avengers)
        let out_friend_a = render_msg!("char_2", &template_a, "source" => &avengers).unwrap();
        let out_friend_the = render_msg!("char_2", &template_the, "source" => &avengers).unwrap();

        // Suppresses articles natively because they are recognized as a proper noun
        assert_eq!(out_friend_a, "The Avengers arrive.");
        assert_eq!(out_friend_the, "The Avengers arrive.");

        // 2. Stranger's perspective (sees "masked heroes")
        let out_stranger_a = render_msg!("stranger_1", &template_a, "source" => &avengers).unwrap();
        let out_stranger_the =
            render_msg!("stranger_1", &template_the, "source" => &avengers).unwrap();

        // Evaluates as a common plural noun, meaning `{a:source}` maps to "Some", and `{the:source}` maps to "The"
        assert_eq!(out_stranger_a, "Some masked heroes arrive.");
        assert_eq!(out_stranger_the, "The masked heroes arrive.");
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
            Template::compile("the {target} watches as the {source} [source:attack]!").unwrap();

        let ctx = RenderContext::new("char_2")
            .with_entity("source", &wolves)
            .with_entity("target", &player);

        let output = PerspectiveEngine::render(&template, &ctx).unwrap();

        // Because wolves are plural, the verb "attack" should NOT become "attacks",
        // even though it's evaluating in the third person.
        assert_eq!(output, "The Aldran watches as the pack of wolves attack!");
    }

    #[test]
    fn test_template_caching() {
        let player = MockEntity {
            id: "char_1".to_string(),
            name: "Aldran".to_string(),
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: true,
        };

        // Initialize the cache with a capacity of 1000 templates
        let cache = TemplateCache::new(1000);
        let raw_text = "The {source} [source:attack]!";

        // First call - CACHE MISS. The engine compiles the AST.
        let template_1 = cache.get_or_compile(raw_text).unwrap();

        // Second call - CACHE HIT. The engine instantly returns the pre-compiled AST.
        let template_2 = cache.get_or_compile(raw_text).unwrap();

        let ctx = RenderContext::new("char_1").with_entity("source", &player);

        // Both pointers work perfectly with your existing renderer!
        let output_1 = PerspectiveEngine::render(&template_1, &ctx).unwrap();
        let output_2 = PerspectiveEngine::render(&template_2, &ctx).unwrap();

        assert_eq!(output_1, "The you attack!");
        assert_eq!(output_2, "The you attack!");
    }

    #[test]
    fn test_macro_ergonomics() {
        let player = MockEntity {
            id: "char_1".to_string(),
            name: "Aldran".to_string(),
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: true,
        };

        let wolves = MockEntity {
            id: "mob_1".to_string(),
            name: "pack of wolves".to_string(),
            gender: Gender::Plural,
            is_plural: true,
            is_proper_noun: false,
        };

        let cache = TemplateCache::new(100);
        let template = cache
            .get_or_compile("{the:target} [target:watch] as the {source} [source:attack]!")
            .unwrap();

        // BEFORE: The verbose, manual context building
        let manual_ctx = RenderContext::new("char_1")
            .with_entity("source", &wolves)
            .with_entity("target", &player);
        let manual_output = PerspectiveEngine::render(&template, &manual_ctx).unwrap();

        // AFTER: The clean, single-line macro approach
        let macro_output = render_msg!("char_1", &template,
            "source" => &wolves,
            "target" => &player,
        )
        .unwrap();

        // Both should yield the exact same grammatically correct string
        assert_eq!(manual_output, "You watch as the pack of wolves attack!");
        assert_eq!(macro_output, "You watch as the pack of wolves attack!");
    }

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
            .get_or_compile("{source} [source:open] the door.")
            .unwrap();

        // Viewer is IN the party -> Expects "you" injection and uninflected verb
        let member_action = render_msg!("char_1", &template_action, "source" => &party).unwrap();
        assert_eq!(member_action, "You and Bob open the door.");

        // Viewer is OUTSIDE the party -> Expects 3rd-person names, but still an uninflected verb
        let observer_action = render_msg!("char_3", &template_action, "source" => &party).unwrap();
        assert_eq!(observer_action, "Aldran and Bob open the door.");

        // Oxford comma test for 3+ members
        let observer_big = render_msg!("mob_1", &template_action, "source" => &big_party).unwrap();
        assert_eq!(observer_big, "Aldran, Bob, and Charlie open the door.");

        // --- SCENARIO 2: Group Pronouns ---
        let template_pronoun = cache
            .get_or_compile("{the:source} [source:attack] {target:obj}!")
            .unwrap();

        // The group is the target, viewer is IN the group -> Expects 2nd-person "you"
        let member_pronoun =
            render_msg!("char_1", &template_pronoun, "source" => &enemy, "target" => &party)
                .unwrap();
        assert_eq!(member_pronoun, "The Goblin attacks you!");

        // The group is the target, viewer is OUTSIDE the group -> Expects 3rd-person plural "them"
        let observer_pronoun =
            render_msg!("char_3", &template_pronoun, "source" => &enemy, "target" => &party)
                .unwrap();
        assert_eq!(observer_pronoun, "The Goblin attacks them!");

        // --- SCENARIO 3: Article Suppression ---
        let template_article = cache
            .get_or_compile("{the:source} [source:be] ready.")
            .unwrap();

        // Viewer IN party -> suppresses article (starts with "You")
        let member_article = render_msg!("char_1", &template_article, "source" => &party).unwrap();
        assert_eq!(member_article, "You and Bob are ready.");

        // Viewer OUTSIDE party -> suppresses article because the Group is treated as a proper noun
        let observer_article =
            render_msg!("char_3", &template_article, "source" => &party).unwrap();
        assert_eq!(observer_article, "Aldran and Bob are ready.");

        // --- SCENARIO 4: Mixed Recognition (Internal Articles) ---
        let mixed_party = GroupEntity {
            members: vec![&player, &enemy],
        };
        let template_mixed = cache
            .get_or_compile("{source} [source:prepare] for battle.")
            .unwrap();

        let observer_mixed =
            render_msg!("char_3", &template_mixed, "source" => &mixed_party).unwrap();
        // "Aldran" is a proper noun (no article), "Goblin" is a common noun (gets "the").
        assert_eq!(observer_mixed, "Aldran and the Goblin prepare for battle.");
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
            .get_or_compile("{source} [source:must] flee from {the:target}!")
            .unwrap();

        // Actor Stance (Player is the one fleeing)
        let actor_must =
            render_msg!("char_1", &template_must, "source" => &player, "target" => &goblin)
                .unwrap();
        assert_eq!(actor_must, "You must flee from the Goblin!");

        // Director Stance (A bystander is watching the Player flee)
        // The engine should output "must", NOT "musts"
        let director_must =
            render_msg!("char_3", &template_must, "source" => &player, "target" => &goblin)
                .unwrap();
        assert_eq!(director_must, "Aldran must flee from the Goblin!");

        // --- TEST 2: Multiple modal verbs ("can" and "will") in a complex sentence ---
        let template_can = cache
            .get_or_compile(
                "if {source} [source:can] catch {the:target}, {source:subj} [source:will] win.",
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
                "{the:source} [source:should] be careful, or {the:target} [target:might] attack.",
            )
            .unwrap();

        let observer_should =
            render_msg!("char_3", &template_should, "source" => &player, "target" => &wolves)
                .unwrap();
        assert_eq!(
            observer_should,
            "Aldran should be careful, or the pack of wolves might attack."
        );
    }

    #[test]
    fn test_unclosed_tags_return_errors() {
        let entity_err = Template::compile("The {source approaches.").unwrap_err();
        assert_eq!(entity_err, "Unclosed entity tag starting at index 4");

        let verb_err = Template::compile("The goblin [attack").unwrap_err();
        assert_eq!(verb_err, "Unclosed verb tag starting at index 11");
    }

    #[test]
    fn test_malformed_tags_return_errors() {
        let entity_err = Template::compile("The {a:b:c} approaches.").unwrap_err();
        assert_eq!(entity_err, "Malformed entity tag: {a:b:c}");

        let verb_err = Template::compile("The goblin [a:b:c]").unwrap_err();
        assert_eq!(verb_err, "Malformed verb tag: [a:b:c]");
    }

    #[test]
    fn test_empty_tag_parts_return_errors() {
        let err1 = Template::compile("The {a:} approaches.").unwrap_err();
        assert_eq!(err1, "Entity tag has an article but an empty key: {a:}");

        let err2 = Template::compile("The {the:} approaches.").unwrap_err();
        assert_eq!(err2, "Entity tag has an article but an empty key: {the:}");

        let err3 = Template::compile("The goblin hits {:poss} shield.").unwrap_err();
        assert_eq!(err3, "Pronoun tag has an empty key or type: {:poss}");

        let err4 = Template::compile("The goblin hits {source:}.").unwrap_err();
        assert_eq!(err4, "Pronoun tag has an empty key or type: {source:}");

        let err5 = Template::compile("A {} appears.").unwrap_err();
        assert_eq!(err5, "Entity tag has an empty key: {}");

        let err6 = Template::compile("The goblin [:attack].").unwrap_err();
        assert_eq!(err6, "Verb tag has an empty subject key: [:attack]");
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
        let template_be = cache.get_or_compile("{source} [source:Be] here.").unwrap();
        let director_be = render_msg!("char_3", &template_be, "source" => &player).unwrap();
        assert_eq!(director_be, "Aldran Is here.");

        // --- TEST 2: "Have" -> "Has" ---
        let template_have = cache
            .get_or_compile("{source} [source:Have] a sword.")
            .unwrap();
        let director_have = render_msg!("char_3", &template_have, "source" => &player).unwrap();
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

        let template_y = cache.get_or_compile("{source} [source:y].").unwrap();
        let output_y = render_msg!("char_3", &template_y, "source" => &player).unwrap();
        assert_eq!(output_y, "Aldran ys.");

        let template_empty = cache.get_or_compile("{source} [source:].").unwrap();
        let output_empty = render_msg!("char_3", &template_empty, "source" => &player).unwrap();
        assert_eq!(output_empty, "Aldran s.");
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
            .get_or_compile("{source} [source:defend] {source:reflex}!")
            .unwrap();

        // Plural Viewer (Actor Stance) -> tests the "yourselves" logic
        let plural_actor = render_msg!("char_1", &template, "source" => &party).unwrap();
        assert_eq!(plural_actor, "You and Bob defend yourselves!");
    }

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
            .get_or_compile("\x1b[31mthe {source} [source:attack].")
            .unwrap();
        let output_ansi = render_msg!("char_2", &template_ansi, "source" => &goblin).unwrap();
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
            .get_or_compile("<COLOR red>a {source} [source:approach].")
            .unwrap();
        let output_mxp = render_msg!("char_2", &template_mxp, "source" => &goblin).unwrap();
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
            .get_or_compile("a <COLOR red.blue>fierce {source} [source:approach].")
            .unwrap();
        let output_mxp = render_msg!("char_2", &template_mxp, "source" => &goblin).unwrap();
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
            .get_or_compile("!!SOUND(roar.wav){the:source} [source:roar].")
            .unwrap();
        let output_msp = render_msg!("char_2", &template_msp, "source" => &goblin).unwrap();
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
                "\x1b[1;32m<SEND href=\"look\">{the:source} [source:wave].\x1b[0m <COLOR blue>it [source:smile].",
            )
            .unwrap();

        let output_mixed = render_msg!("char_2", &template_mixed, "source" => &goblin).unwrap();
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
            .get_or_compile("<SEND HREF=\"[look]\">{the:source} triggers a !!SOUND({roar})!")
            .unwrap();

        let output = render_msg!("char_2", &template, "source" => &goblin).unwrap();
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
            .get_or_compile("\x1b]8;;https://example.com/?q={123}&v=[456]\x07{the:source} [source:attack].\x1b]8;;\x07")
            .unwrap();

        let output = render_msg!("char_2", &template, "source" => &goblin).unwrap();
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
            .get_or_compile("\x1b]8;;unterminated {the:source} [source:attack].")
            .unwrap();

        let output = render_msg!("char_2", &template, "source" => &goblin).unwrap();
        // The 'u' in 'unterminated' is capitalized by the post-processor because
        // the sequence was treated as literal text.
        assert_eq!(output, "\x1b]8;;Unterminated the goblin attacks.");

        // 2. Unterminated CSI sequence
        // Falls back to literal text, but the `[` immediately triggers the verb tag parser.
        // Since there's no `]`, it safely fails with a syntax error instead of skipping the string.
        let err = Template::compile("\x1b[31").unwrap_err();
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
            .get_or_compile("<color red unterminated {the:source} [source:attack].")
            .unwrap();

        let output = render_msg!("char_2", &template, "source" => &goblin).unwrap();
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
            .get_or_compile("!!SOUND(roar.wav unterminated {the:source} [source:attack].")
            .unwrap();

        let output = render_msg!("char_2", &template, "source" => &goblin).unwrap();
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
            .get_or_compile(r"some \{escaped\} and \[tags\]. \\{The:source}")
            .unwrap();

        let output = render_msg!("char_2", &template, "source" => &goblin).unwrap();
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
            .get_or_compile("\"{the:source} [source:be] a fool!\"")
            .unwrap();
        let output1 = render_msg!("char_2", &template1, "source" => &goblin).unwrap();
        assert_eq!(output1, "\"The goblin is a fool!\"");

        // Scenario 2: Quote in the middle of a sentence with a proper noun
        // Proper nouns are returned already capitalized by `display_name_for`.
        let template2 = cache
            .get_or_compile("{the:source} [source:say], \"{target} [target:be] a fool!\"")
            .unwrap();
        let output2 =
            render_msg!("char_2", &template2, "source" => &goblin, "target" => &player).unwrap();
        assert_eq!(output2, "The goblin says, \"Aldran is a fool!\"");

        // Scenario 3: Quote in the middle of a sentence with a common noun (Capitalized Article)
        // By using {The:source}, we force the engine to capitalize the article regardless of the segmenter.
        let template3 = cache
            .get_or_compile("{target} [target:say], \"{The:source} [source:be] a fool!\"")
            .unwrap();
        let output3 =
            render_msg!("char_2", &template3, "source" => &goblin, "target" => &player).unwrap();
        assert_eq!(output3, "Aldran says, \"The goblin is a fool!\"");

        // Scenario 4: Indefinite article capitalization
        let template4 = cache
            .get_or_compile("{target} [target:yell], \"{A:source} [source:be] approaching!\"")
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
        let template1 = cache.get_or_compile("you point at {the:Source}.").unwrap();
        let out1 = render_msg!("stranger_1", &template1, "source" => &disguised).unwrap();
        assert_eq!(out1, "You point at the Tall man.");

        // 2. Force capitalizing a pronoun mid-sentence
        let template2 = cache
            .get_or_compile("you watch as {source:Subj} falls.")
            .unwrap();
        let out2 = render_msg!("stranger_1", &template2, "source" => &disguised).unwrap();
        assert_eq!(out2, "You watch as He falls.");

        // 3. Verbs already support this organically
        // We use `{the:source}` to provide the article, and a sentence structure
        // where a conjugated verb is grammatically correct mid-sentence.
        let template3 = cache
            .get_or_compile("they say {the:source} [source:Smile] often.")
            .unwrap();
        let out3 = render_msg!("stranger_1", &template3, "source" => &disguised).unwrap();
        assert_eq!(out3, "They say the tall man Smiles often.");
    }
}
