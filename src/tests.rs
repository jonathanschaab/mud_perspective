#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
        assert_eq!(observer_pronoun, "The Goblin attacks Aldran and Bob!");

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
            .get_or_compile("{the:source} [source:prepare] for battle.")
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
        let ctx2 = RenderContext::new("stranger_1")
            .with_entity("source", &disguised)
            .with_last_mentioned("source"); // Seed the context so it prints the pronoun!
        let out2 = PerspectiveEngine::render(&template2, &ctx2).unwrap();
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
            .get_or_compile("{the:source} [source:prepare].")
            .unwrap();

        // 1. Director Stance (bystander sees everyone)
        // Expects empty group to be completely ignored.
        // Nested group is flattened so it prints as a single cohesive list.
        let out_director = render_msg!("stranger_1", &template, "source" => &top_group).unwrap();
        assert_eq!(
            out_director,
            "The tall man, Bob, Charlie, and Dave prepare."
        );

        // 2. Actor Stance (Player is the viewer)
        // Expects "You" to be pulled to the front of the flattened list cleanly.
        let out_actor = render_msg!("char_1", &template, "source" => &top_group).unwrap();
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
            .get_or_compile("{source} [source:open] the door.")
            .unwrap();
        let out_verb = render_msg!("char_2", &template_verb, "source" => &solo_group).unwrap();
        assert_eq!(out_verb, "Aldran opens the door.");

        // 2. Pronoun Resolution
        // Because Aldran is male, the pronoun must be "his" instead of "their"
        let template_pronoun = cache
            .get_or_compile("{source} [source:open] {source:poss} door.")
            .unwrap();
        let out_pronoun =
            render_msg!("char_2", &template_pronoun, "source" => &solo_group).unwrap();
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
            .get_or_compile("{the:source} [source:attack].")
            .unwrap();
        let out_art = render_msg!("char_2", &template_art, "source" => &goblin_group).unwrap();
        assert_eq!(out_art, "The goblin attacks.");
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
            .get_or_compile("{This:source} [source:was] angry.")
            .unwrap();

        let out_singular = render_msg!("char_2", &template, "source" => &goblin).unwrap();
        assert_eq!(out_singular, "This goblin was angry.");

        let out_plural = render_msg!("char_2", &template, "source" => &wolves).unwrap();
        assert_eq!(out_plural, "These wolves were angry.");

        // 2. Automatically suppresses the demonstrative for the viewer just like an article
        let out_viewer = render_msg!("mob_2", &template, "source" => &wolves).unwrap();
        assert_eq!(out_viewer, "You were angry.");

        // 3. Forcing an article for a proper noun using the `+` prefix
        let template_force = cache
            .get_or_compile("{+This:source} [source:be] angry.")
            .unwrap();
        let aldran = MockEntity {
            id: "char_1".to_string(),
            name: "Aldran".to_string(),
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: true,
        };
        let out_forced = render_msg!("char_2", &template_force, "source" => &aldran).unwrap();
        assert_eq!(out_forced, "This Aldran is angry.");
    }

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
            .get_or_compile("{+source} [+source:attack] {the:target} with {+source:poss} sword.")
            .unwrap();

        // The player is the viewer, so normally this would render "You attack the goblin with your sword."
        // Because of the `+` prefix on the keys, it forces 3rd person logic even for the viewer!
        let out_forced =
            render_msg!("char_1", &template_forced, "source" => &player, "target" => &goblin)
                .unwrap();
        assert_eq!(out_forced, "Aldran attacks the goblin with his sword.");

        // Can even force an article onto a forced-3rd-person proper noun (e.g. {+the:+source})
        let template_double_force = cache.get_or_compile("{+the:+source} is here.").unwrap();
        let out_double_force =
            render_msg!("char_1", &template_double_force, "source" => &player).unwrap();
        assert_eq!(out_double_force, "The Aldran is here.");
    }

    #[test]
    fn test_unbound_forced_director_verbs() {
        let cache = TemplateCache::new(100);
        let ctx = RenderContext::new("viewer");

        // [+smile] should output "smiles" (director stance, which is default anyway, but tests parser stripping)
        let out_director =
            PerspectiveEngine::render(&cache.get_or_compile("he [+smile].").unwrap(), &ctx)
                .unwrap();
        assert_eq!(out_director, "He smiles.");
    }

    #[test]
    fn test_anaphora_resolution() {
        let goblin = MockEntity {
            id: "mob_1".to_string(),
            name: "goblin".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
        };

        let gnome = MockEntity {
            id: "mob_2".to_string(),
            name: "gnome".to_string(),
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: false,
        };

        let cache = TemplateCache::new(100);
        let ctx = RenderContext::new("char_2")
            .with_entity("target", &goblin)
            .with_entity("other", &gnome);

        // 1. First time using a pronoun tag: Automatically expands to the full name!
        let t1 = cache
            .get_or_compile("{target:Subj} [target:look] around.")
            .unwrap();
        let out1 = PerspectiveEngine::render(&t1, &ctx).unwrap();
        assert_eq!(out1, "The goblin looks around.");

        // 2. Second time using a pronoun tag: The context REMEMBERS the goblin and uses "It"!
        let t2 = cache
            .get_or_compile("{target:Subj} [target:attack]!")
            .unwrap();
        let out2 = PerspectiveEngine::render(&t2, &ctx).unwrap();
        assert_eq!(out2, "It attacks!");

        // 3. Clearing the context resets the memory, expanding it to the full name again.
        ctx.clear_anaphora();
        let out3 = PerspectiveEngine::render(&t2, &ctx).unwrap();
        assert_eq!(out3, "The goblin attacks!");

        // 4. Interruption by another entity prevents confusing pronouns
        ctx.clear_anaphora();
        let t4 = cache
            .get_or_compile(
                "{The:target} enters. {The:other} blinks. {target:Subj} [target:scream].",
            )
            .unwrap();
        let out4 = PerspectiveEngine::render(&t4, &ctx).unwrap();
        // Because the gnome was the last entity mentioned, the pronoun for the target (goblin)
        // would be ambiguous. The engine must safely expand it back to "The goblin".
        assert_eq!(
            out4,
            "The goblin enters. The gnome blinks. The goblin screams."
        );

        // 5. Reflexive pronouns explicitly bypass Anaphora resolution.
        // Possessive pronouns fallback intelligently to possessive nouns!
        ctx.clear_anaphora();
        let t5 = cache
            .get_or_compile("{Other:poss} sword falls, and {other:subj} cuts {other:reflex}.")
            .unwrap();

        let out5 = PerspectiveEngine::render(&t5, &ctx).unwrap();
        assert_eq!(out5, "The gnome's sword falls, and he cuts himself.");
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
            .get_or_compile("{the:goblin} [goblin:hit] {aldran}. {aldran:Subj} [aldran:smile].")
            .unwrap();
        let out1 = PerspectiveEngine::render(&t1, &ctx).unwrap();
        assert_eq!(out1, "The goblin hits Aldran. He smiles.");

        // 2. Ambiguous object reference (Bob -> Male, Aldran -> Male)
        // Using "He" for Aldran is ambiguous, so the engine must fall back to "Aldran"
        ctx.clear_anaphora();
        let t2 = cache
            .get_or_compile("{bob} [bob:hit] {aldran}. {aldran:Subj} [aldran:smile].")
            .unwrap();
        let out2 = PerspectiveEngine::render(&t2, &ctx).unwrap();
        assert_eq!(out2, "Bob hits Aldran. Aldran smiles.");

        // 3. Ambiguous object reference with 3+ entities (Jill -> Female, Bob -> Male, Aldran -> Male)
        // Active subject is Jill. Target is Aldran. Jill is Female, Aldran is Male (unambiguous vs subject).
        // BUT Bob is Male. So "He" is ambiguous between Bob and Aldran.
        ctx.clear_anaphora();
        let t3 = cache
            .get_or_compile(
                "{jill} [jill:tell] {bob} about {aldran}. {aldran:Subj} [aldran:smile].",
            )
            .unwrap();
        let out3 = PerspectiveEngine::render(&t3, &ctx).unwrap();
        // Because Bob is in the `recent_entities` memory, "He" is correctly bypassed!
        assert_eq!(out3, "Jill tells Bob about Aldran. Aldran smiles.");
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
        // We expect "The giant spider", NOT "The Giant Spider" or "The Giant spider".
        let template = cache
            .get_or_compile("{target:Subj} [target:hiss].")
            .unwrap();
        let output = PerspectiveEngine::render(&template, &ctx).unwrap();
        assert_eq!(output, "The giant spider hisses.");
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
        let t1 = cache.get_or_compile("{the:target} enters.").unwrap();
        let t2 = cache
            .get_or_compile("{target:Subj} [target:look] around.")
            .unwrap();

        // Render the first template in context 1
        let ctx1 = RenderContext::new("char_2").with_entity("target", &goblin);
        let _ = PerspectiveEngine::render(&t1, &ctx1).unwrap();

        // Extract the subject from context 1 and inject it into a brand new context 2
        let subject = ctx1.last_mentioned().unwrap();
        let ctx2 = RenderContext::new("char_2")
            .with_entity("target", &goblin)
            .with_last_mentioned(&subject);

        let out2 = PerspectiveEngine::render(&t2, &ctx2).unwrap();
        // The engine natively uses "It" instead of "The goblin" because the context was seeded!
        assert_eq!(out2, "It looks around.");
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
                "{Source} [source:hit] {the:target}, then {source:subj} [source:step] back.",
            )
            .unwrap();

        // 1. Bystander (Director Stance)
        // Because "goblin" took focus, "Aldran" cannot safely use "he". It falls back to "Aldran".
        let out_director =
            render_msg!("char_3", &template, "source" => &player, "target" => &goblin).unwrap();
        assert_eq!(out_director, "Aldran hits the goblin, then he steps back.");

        // 2. Player (Actor Stance)
        // Even though "goblin" took focus, "you" is immune to anaphora ambiguity. It stays "you".
        let out_actor =
            render_msg!("char_1", &template, "source" => &player, "target" => &goblin).unwrap();
        assert_eq!(out_actor, "You hit the goblin, then you step back.");
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
            .get_or_compile("The victory is {source:abs_poss}!")
            .unwrap();

        let out_actor = render_msg!("char_1", &template, "source" => &player).unwrap();
        assert_eq!(out_actor, "The victory is yours!");

        // Seed the anaphora memory so it evaluates the pronoun instead of falling back to "Aldran's"
        let ctx_director = RenderContext::new("char_2")
            .with_entity("source", &player)
            .with_last_mentioned("source");
        let out_director = PerspectiveEngine::render(&template, &ctx_director).unwrap();
        assert_eq!(out_director, "The victory is his!");

        let ctx_plural = RenderContext::new("char_2")
            .with_entity("source", &wolves)
            .with_last_mentioned("source");
        let out_plural = PerspectiveEngine::render(&template, &ctx_plural).unwrap();
        assert_eq!(out_plural, "The victory is theirs!");
    }

    #[test]
    fn test_unbound_verbs() {
        let cache = TemplateCache::new(100);
        let ctx = RenderContext::new("viewer");

        // Without a subject, verbs safely default to 3rd-person singular conjugation
        let template = cache
            .get_or_compile("a shadow [loom] in the distance, and [approach].")
            .unwrap();
        let out = PerspectiveEngine::render(&template, &ctx).unwrap();
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
            .get_or_compile("You take {the:target's} gold.")
            .unwrap();

        // 1. Viewer
        let out_viewer = render_msg!("char_1", &template, "target" => &player).unwrap();
        assert_eq!(out_viewer, "You take your gold.");

        // 2. Singular Proper Noun
        let out_proper = render_msg!("char_2", &template, "target" => &player).unwrap();
        assert_eq!(out_proper, "You take Aldran's gold.");

        // 3. Plural common noun ending in 's'
        let out_plural = render_msg!("char_2", &template, "target" => &wolves).unwrap();
        assert_eq!(out_plural, "You take the wolves' gold.");

        // 4. Singular common noun ending in 's'
        let out_boss = render_msg!("char_2", &template, "target" => &boss).unwrap();
        assert_eq!(out_boss, "You take the boss's gold.");

        // 5. Group Entities with possessive suffixes
        // English attaches joint possessives to the final noun. The engine natively looks
        // at the end of the formatted list to determine if it should use 's or just '.
        let wolf_party = GroupEntity::new(vec![&player, &wolves]);
        let out_wolf_party = render_msg!("char_2", &template, "target" => &wolf_party).unwrap();
        assert_eq!(out_wolf_party, "You take Aldran and the wolves' gold.");

        // 6. Forced Director Stance with Possessive Suffixes
        let template_forced = cache.get_or_compile("You take {+target's} gold.").unwrap();
        // Even though the viewer is char_1 (the player), the + prefix overrides "your" to "Aldran's"
        let out_forced = render_msg!("char_1", &template_forced, "target" => &player).unwrap();
        assert_eq!(out_forced, "You take Aldran's gold.");
    }

    #[test]
    fn test_dot_notation_resolution() {
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

        struct Actor {
            name: String,
            weapon: Weapon,
        }
        impl TemplateEntity for Actor {
            fn contains_viewer(&self, viewer_id: &str) -> bool {
                viewer_id == "char_1"
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
            fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
                match property_name {
                    "weapon" => Some(&self.weapon),
                    _ => None,
                }
            }
        }

        let player = Actor {
            name: "Aldran".to_string(),
            weapon: Weapon {
                name: "rusty sword".to_string(),
            },
        };

        let cache = TemplateCache::new(100);
        let template = cache.get_or_compile("{Source} [source:draw] {a:source.weapon} and [source:swing] {source:poss} {source.weapon}!").unwrap();

        let out_director = render_msg!("char_2", &template, "source" => &player).unwrap();
        assert_eq!(
            out_director,
            "Aldran draws a rusty sword and swings his rusty sword!"
        );

        let out_actor = render_msg!("char_1", &template, "source" => &player).unwrap();
        assert_eq!(
            out_actor,
            "You draw a rusty sword and swing your rusty sword!"
        );

        // Verify graceful error handling if a builder requests a property that doesn't exist
        let err_template = cache.get_or_compile("{source.shield} breaks.").unwrap();
        let err_output = PerspectiveEngine::render(
            &err_template,
            &RenderContext::new("char_1").with_entity("source", &player),
        )
        .unwrap_err();
        assert_eq!(err_output, "Missing property 'shield' on entity 'source'");

        // Verify multi-level error handling tracks the traversed path accurately
        let err_template_multi = cache
            .get_or_compile("{source.weapon.edge} is sharp.")
            .unwrap();
        let err_output_multi = PerspectiveEngine::render(
            &err_template_multi,
            &RenderContext::new("char_1").with_entity("source", &player),
        )
        .unwrap_err();
        assert_eq!(
            err_output_multi,
            "Missing property 'edge' on entity 'source.weapon'"
        );

        // Verify malformed double-dot paths return a clear error at compile time
        let double_dot_err = cache
            .get_or_compile("{a:source..weapon} is drawn.")
            .unwrap_err();
        assert_eq!(
            double_dot_err,
            "Entity tag has an empty property segment: {a:source..weapon}"
        );
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
            .get_or_compile("You take {the:target's} gold.")
            .unwrap();

        // Singular common noun ending in 's' with ANSI code at the end -> expects 's
        let out_boss = render_msg!("char_2", &template, "target" => &colored_boss).unwrap();
        assert_eq!(out_boss, "You take the \x1b[31mboss\x1b[0m's gold.");

        // Plural common noun ending in 's' with ANSI code at the end -> expects '
        let out_wolves = render_msg!("char_2", &template, "target" => &colored_wolves).unwrap();
        assert_eq!(out_wolves, "You take the \x1b[32mwolves\x1b[0m' gold.");

        // Regular singular common noun with ANSI code at the end -> expects 's
        let out_goblin = render_msg!("char_2", &template, "target" => &colored_goblin).unwrap();
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
            .get_or_compile("You take {the:target's} gold.")
            .unwrap();

        // Plural ending in 's' followed by space -> expects '
        let out_wolves = render_msg!("char_2", &template, "target" => &wolves_spaced).unwrap();
        assert_eq!(out_wolves, "You take the wolves ' gold.");

        // Singular ending in 's' followed by multiple spaces -> expects 's
        let out_boss = render_msg!("char_2", &template, "target" => &boss_spaced).unwrap();
        assert_eq!(out_boss, "You take the boss   's gold.");
    }
}
