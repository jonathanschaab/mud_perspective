#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use crate::cache::TemplateCache;
    use crate::engine::{PerspectiveEngine, Template};
    use crate::models::{Gender, GroupEntity, RenderContext, TemplateEntity};
    use serial_test::serial;
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

        fn long_display_name_for<'a>(&'a self, _: &str) -> Option<Cow<'a, str>> {
            if self.id == "mob_2_long" || self.id == "mob_3_long_collide" {
                Some(Cow::Borrowed("large wolf"))
            } else if self.id == "mob_1_scrawny" {
                Some(Cow::Borrowed("scrawny wolf"))
            } else if self.id == "char_jim" {
                Some(Cow::Borrowed("large wolf"))
            } else {
                None
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

        let err7 = Template::compile("You [source:be|].").unwrap_err();
        assert_eq!(
            err7,
            "Verb tag has an empty verb or forced conjugation segment: [source:be|]"
        );

        let err8 = Template::compile("You [source:|be].").unwrap_err();
        assert_eq!(
            err8,
            "Verb tag has an empty verb or forced conjugation segment: [source:|be]"
        );

        let err9 = Template::compile("You [source:be|am||is].").unwrap_err();
        assert_eq!(
            err9,
            "Verb tag has an empty forced present conjugation segment: [source:be|am||is]"
        );

        let err10 = Template::compile("You [source:be|am|are|is|were].").unwrap_err();
        assert_eq!(
            err10,
            "Verb tag has too many forced present conjugation segments: [source:be|am|are|is|were]"
        );

        let err11 = Template::compile("You [source:be|am|are|is;].").unwrap_err();
        assert_eq!(
            err11,
            "Verb tag has an empty forced past conjugation segment: [source:be|am|are|is;]"
        );

        let err12 = Template::compile("You [source:be|;was||was].").unwrap_err();
        assert_eq!(
            err12,
            "Verb tag has an empty forced past conjugation segment: [source:be|;was||was]"
        );

        let err13 = Template::compile("You [source:be|am|are|is;was|were|was|were].").unwrap_err();
        assert_eq!(
            err13,
            "Verb tag has too many forced past conjugation segments: [source:be|am|are|is;was|were|was|were]"
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
            .get_or_compile("{the:source} [source:freak out|freak out|freaks out].")
            .unwrap();
        assert_eq!(
            render_msg!("char_1", &template_2, "source" => &player).unwrap(),
            "You freak out."
        );
        assert_eq!(
            render_msg!("char_2", &template_2, "source" => &player).unwrap(),
            "Aldran freaks out."
        );
        assert_eq!(
            render_msg!("char_2", &template_2, "source" => &wolves).unwrap(),
            "The wolves freak out."
        );

        // 2. Three-part syntax (1st Singular | 2nd/Plural | 3rd Singular)
        let template_3 = cache
            .get_or_compile("{the:source} [source:be|was|were|was] here.")
            .unwrap();
        let ctx_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &player);
        let ctx_second = RenderContext::new("char_1").with_entity("source", &player);
        let ctx_third = RenderContext::new("char_2").with_entity("source", &player);
        let ctx_plural = RenderContext::new("char_2").with_entity("source", &wolves);

        assert_eq!(
            PerspectiveEngine::render(&template_3, &ctx_first).unwrap(),
            "I was here."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_3, &ctx_second).unwrap(),
            "You were here."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_3, &ctx_third).unwrap(),
            "Aldran was here."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_3, &ctx_plural).unwrap(),
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
            .get_or_compile("{This:source} [source:be] angry.")
            .unwrap();

        let ctx_singular = RenderContext::new("char_2")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &goblin);
        let out_singular = PerspectiveEngine::render(&template, &ctx_singular).unwrap();
        assert_eq!(out_singular, "This goblin was angry.");

        let ctx_plural = RenderContext::new("char_2")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &wolves);
        let out_plural = PerspectiveEngine::render(&template, &ctx_plural).unwrap();
        assert_eq!(out_plural, "These wolves were angry.");

        // 2. Automatically suppresses the demonstrative for the viewer just like an article
        let ctx_viewer = RenderContext::new("mob_2")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &wolves);
        let out_viewer = PerspectiveEngine::render(&template, &ctx_viewer).unwrap();
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
            .get_or_compile("{target:Subj} [target:look] around.")
            .unwrap();
        let out1 = PerspectiveEngine::render(&t1, &ctx).unwrap();
        assert_eq!(out1, "A goblin looks around.");

        // 2. Second time using a pronoun tag: The context REMEMBERS the goblin and uses "It"!
        let t2 = cache
            .get_or_compile("{target:Subj} [target:attack]!")
            .unwrap();
        let out2 = PerspectiveEngine::render(&t2, &ctx).unwrap();
        assert_eq!(out2, "It attacks!");

        // 3. Clearing the context resets the memory, expanding it to the full name again.
        ctx.clear_anaphora();
        let out3 = PerspectiveEngine::render(&t2, &ctx).unwrap();
        assert_eq!(out3, "A goblin attacks!");

        // 4. Interruption by another entity prevents confusing pronouns
        ctx.clear_anaphora();
        let t4 = cache
            .get_or_compile(
                "{The:target} enters. {The:other} blinks. {target:Subj} [target:scream].",
            )
            .unwrap();
        let out4 = PerspectiveEngine::render(&t4, &ctx).unwrap();
        // Because the slime (Neutral) was just introduced, the pronoun for the target (goblin, also Neutral)
        // is now ambiguous. The engine must safely expand it back to "The goblin" to prevent confusion.
        assert_eq!(
            out4,
            "The goblin enters. The slime blinks. The goblin screams."
        );

        // 5. Reflexive pronouns explicitly bypass Anaphora resolution.
        // Possessive pronouns fallback intelligently to possessive nouns!
        ctx.clear_anaphora();
        let t5 = cache
            .get_or_compile("{Other:poss} sword falls, and {other:subj} cuts {other:reflex}.")
            .unwrap();

        let out5 = PerspectiveEngine::render(&t5, &ctx).unwrap();
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
            .get_or_compile("Bob [bob:attack] {aldran}. {aldran:Subj} [aldran:fall].")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx1).unwrap(),
            "Bob attacks Aldran. Aldran falls."
        );

        // 2. Unambiguous tracking via verb tag
        // Jill is introduced via a verb tag. Because she is Female, Aldran (Male) can safely use "He".
        let ctx2 = RenderContext::new("viewer")
            .with_entity("jill", &jill)
            .with_entity("aldran", &aldran);
        let t2 = cache
            .get_or_compile("Jill [jill:attack] {aldran}. {aldran:Subj} [aldran:fall].")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx2).unwrap(),
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
            .get_or_compile("{target:Subj} [target:hiss].")
            .unwrap();
        let output = PerspectiveEngine::render(&template, &ctx).unwrap();
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
        let t1 = cache.get_or_compile("{the:target} enters.").unwrap();
        let t2 = cache
            .get_or_compile("{target:Subj} [target:look] around.")
            .unwrap();

        // Render the first template in context 1
        let ctx1 = RenderContext::new("char_2").with_entity("target", &goblin);
        let _ = PerspectiveEngine::render(&t1, &ctx1).unwrap();

        // Extract the full narrative state from context 1 and inject it into a brand new context 2
        let state = ctx1.extract_anaphora();
        let ctx2 = RenderContext::new("char_2")
            .with_entity("target", &goblin)
            .with_anaphora(state);

        let out2 = PerspectiveEngine::render(&t2, &ctx2).unwrap();
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
            .get_or_compile("{bob} is standing next to {aldran}.")
            .unwrap();
        let t2 = cache
            .get_or_compile("{aldran:Subj} [aldran:wave].")
            .unwrap();

        // Render the first sentence
        let ctx1 = RenderContext::new("viewer")
            .with_entity("aldran", &aldran)
            .with_entity("bob", &bob);
        let out1 = PerspectiveEngine::render(&t1, &ctx1).unwrap();
        assert_eq!(out1, "Bob is standing next to Aldran.");

        // Carry the state over to context 2
        let state = ctx1.extract_anaphora();
        let ctx2 = RenderContext::new("viewer")
            .with_entity("aldran", &aldran)
            .with_entity("bob", &bob)
            .with_anaphora(state);

        // Because the state includes Bob in the recent_entities memory, the engine knows
        // that Aldran and Bob are both male and prevents the ambiguous "He waves."
        let out2 = PerspectiveEngine::render(&t2, &ctx2).unwrap();
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
                .get_or_compile("{aldran} [aldran:wave] at {bob}.")
                .unwrap(),
            &ctx,
        )
        .unwrap();

        // 2. Introduce the goblin (Memory: Bob, Goblin). Aldran is evicted!
        let _ = PerspectiveEngine::render(
            &cache
                .get_or_compile("{the:goblin} [goblin:approach].")
                .unwrap(),
            &ctx,
        )
        .unwrap();

        // 3. Request a pronoun for Bob. He is still in memory, so he is safely remembered as the subject.
        let out_bob = PerspectiveEngine::render(
            &cache.get_or_compile("{bob:Subj} [bob:smile].").unwrap(),
            &ctx,
        )
        .unwrap();
        assert_eq!(out_bob, "He smiles.");

        // 4. Request a pronoun for Aldran. Because he was evicted, the engine forgot he was Male,
        // and must fall back to his name!
        let out_aldran = PerspectiveEngine::render(
            &cache
                .get_or_compile("{aldran:Subj} [aldran:sigh].")
                .unwrap(),
            &ctx,
        )
        .unwrap();
        assert_eq!(out_aldran, "Aldran sighs.");
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

        let _ = PerspectiveEngine::render(&cache.get_or_compile("{tom} arrives.").unwrap(), &ctx)
            .unwrap();
        let _ = PerspectiveEngine::render(&cache.get_or_compile("{jim} arrives.").unwrap(), &ctx)
            .unwrap();
        let _ = PerspectiveEngine::render(&cache.get_or_compile("{dan} arrives.").unwrap(), &ctx)
            .unwrap();

        // Dan was the last mentioned. Bob is NOT the last mentioned, but is pinned in memory.
        // Memory has Dan (Male) and Bob (Male). Both are male. Bob's pronoun "He" is correctly recognized as ambiguous!
        let out_bob = PerspectiveEngine::render(
            &cache.get_or_compile("{bob:Subj} [bob:smile].").unwrap(),
            &ctx,
        )
        .unwrap();
        assert_eq!(out_bob, "Bob smiles.");

        // Now we explicitly forget Dan. The only male in memory is Bob.
        ctx.forget_anaphora("dan");

        // Now "He" for Bob should be unambiguous!
        let out_bob2 = PerspectiveEngine::render(
            &cache.get_or_compile("{bob:Subj} [bob:wave].").unwrap(),
            &ctx,
        )
        .unwrap();
        assert_eq!(out_bob2, "He waves.");

        // Now unpin Bob and let him naturally evict.
        ctx.unpin_anaphora("bob");

        // Add Tom, Jim, Dan to push Bob out of the LRU cache
        let _ = PerspectiveEngine::render(&cache.get_or_compile("{tom} arrives.").unwrap(), &ctx)
            .unwrap();
        let _ = PerspectiveEngine::render(&cache.get_or_compile("{jim} arrives.").unwrap(), &ctx)
            .unwrap();
        let _ = PerspectiveEngine::render(&cache.get_or_compile("{dan} arrives.").unwrap(), &ctx)
            .unwrap();

        // Now memory should be [Jim, Dan]. Bob is gone.
        // Requesting Bob's pronoun will just treat him as a newly introduced entity and print his name.
        let out_bob3 = PerspectiveEngine::render(
            &cache.get_or_compile("{bob:Subj} [bob:nod].").unwrap(),
            &ctx,
        )
        .unwrap();
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
        let _ = PerspectiveEngine::render(&cache.get_or_compile("{dan} arrives.").unwrap(), &ctx)
            .unwrap();

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
    fn test_deeply_nested_properties() {
        // A simple recursive struct to easily test arbitrary nesting depths
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
            fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
                if property_name == "child" {
                    self.child.as_deref().map(|c| c as &dyn TemplateEntity)
                } else {
                    None
                }
            }
        }

        let deeply_nested = Node {
            name: "root".to_string(),
            child: Some(Box::new(Node {
                name: "branch".to_string(),
                child: Some(Box::new(Node {
                    name: "twig".to_string(),
                    child: Some(Box::new(Node {
                        name: "leaf".to_string(),
                        child: None,
                    })),
                })),
            })),
        };

        let cache = TemplateCache::new(100);
        let ctx = RenderContext::new("viewer").with_entity("tree", &deeply_nested);

        // 1. Success at 3 levels deep (tree -> child -> child -> child)
        let t1 = cache
            .get_or_compile("You look at {the:tree.child.child.child}.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx).unwrap(),
            "You look at the leaf."
        );

        // 2. Graceful error tracking at 4 levels deep
        let t_err = cache
            .get_or_compile("You look at {the:tree.child.child.child.bug}.")
            .unwrap();
        let err_output = PerspectiveEngine::render(&t_err, &ctx).unwrap_err();
        assert_eq!(
            err_output,
            "Missing property 'bug' on entity 'tree.child.child.child'"
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
            .get_or_compile("{source} [source:walk] forward.")
            .unwrap();

        // Second Person (Default)
        let out_second = render_msg!("char_1", &template, "source" => &player).unwrap();
        assert_eq!(out_second, "You walk forward.");

        // First Person
        let ctx_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &player);
        let out_first = PerspectiveEngine::render(&template, &ctx_first).unwrap();
        assert_eq!(out_first, "I walk forward.");

        // Third Person
        let ctx_third = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::ThirdPerson)
            .with_entity("source", &player);
        let out_third = PerspectiveEngine::render(&template, &ctx_third).unwrap();
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
            .get_or_compile("{source} [source:be] looking for {source:poss} sword.")
            .unwrap();
        let ctx_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &player);
        assert_eq!(
            PerspectiveEngine::render(&template_be, &ctx_first).unwrap(),
            "I am looking for my sword."
        );

        let template_past = cache
            .get_or_compile("Before, {source} [source:be] looking.")
            .unwrap();
        let ctx_past = ctx_first.with_tense(crate::models::Tense::Past);
        assert_eq!(
            PerspectiveEngine::render(&template_past, &ctx_past).unwrap(),
            "Before, I was looking."
        );
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
            .get_or_compile("{source} [source:open] the door.")
            .unwrap();

        // First Person
        let ctx_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_first).unwrap(),
            "Bob and I open the door."
        );

        // Third Person
        let ctx_third = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::ThirdPerson)
            .with_entity("source", &party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_third).unwrap(),
            "Aldran and Bob open the door."
        );
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
                "{source} [source:hit] {the:target}. {target:Subj} [target:hit] {source:obj} back.",
            )
            .unwrap();

        // First Person
        let ctx_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &player)
            .with_entity("target", &goblin);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_first).unwrap(),
            "I hit the goblin. It hits me back."
        );

        // Third Person
        let ctx_third = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::ThirdPerson)
            .with_entity("source", &player)
            .with_entity("target", &goblin);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_third).unwrap(),
            "Aldran hits the goblin. It hits him back."
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
            .get_or_compile("{+source} [+source:draw] {+source:poss} sword.")
            .unwrap();

        let ctx_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &player);

        // The '+' prefix should safely override the First Person 'I/my' back to 'Aldran/his'
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_first).unwrap(),
            "Aldran draws his sword."
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

        let party = GroupEntity {
            members: vec![&player, &ally],
        };

        let cache = TemplateCache::new(100);
        let template = cache
            .get_or_compile("{source:Subj} [source:defend] {source:reflex}. {The:target} [target:strike] {source:obj}. It is {source:poss} fight, the victory is {source:abs_poss}!")
            .unwrap();

        // 1. First Person Singular
        let ctx_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &player)
            .with_entity("target", &goblin);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_first).unwrap(),
            "I defend myself. The goblin strikes me. It is my fight, the victory is mine!"
        );

        // 2. First Person Plural
        let ctx_first_plural = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &party)
            .with_entity("target", &goblin);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_first_plural).unwrap(),
            "We defend ourselves. The goblin strikes us. It is our fight, the victory is ours!"
        );

        // 3. Second Person Singular
        let ctx_second = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::SecondPerson)
            .with_entity("source", &player)
            .with_entity("target", &goblin);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_second).unwrap(),
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
            PerspectiveEngine::render(&template, &ctx_third).unwrap(),
            "He defends himself. The goblin strikes him. It is his fight, the victory is his!"
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
        let template = cache.get_or_compile("They take {source's} gold.").unwrap();

        let ctx_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &player);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_first).unwrap(),
            "They take my gold."
        );

        let ctx_second = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::SecondPerson)
            .with_entity("source", &player);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_second).unwrap(),
            "They take your gold."
        );

        let ctx_third = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::ThirdPerson)
            .with_entity("source", &player);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_third).unwrap(),
            "They take Aldran's gold."
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
            .get_or_compile("{source} [source:attack] {the:target} with {source:poss} claws!")
            .unwrap();

        let ctx = RenderContext::new("mob_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &wolves)
            .with_entity("target", &goblin);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx).unwrap(),
            "We attack the goblin with our claws!"
        );

        // Group with plural viewer
        let party = GroupEntity {
            members: vec![&wolves, &goblin],
        };

        let group_template = cache
            .get_or_compile("{the:source} [source:attack]!")
            .unwrap();
        let group_ctx = RenderContext::new("mob_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &party);

        assert_eq!(
            PerspectiveEngine::render(&group_template, &group_ctx).unwrap(),
            "You, the goblin, and I attack!"
        );

        // Objective pronouns
        let obj_template = cache
            .get_or_compile("{the:target} [target:ambush] {source:obj}!")
            .unwrap();

        // 1. Solo plural viewer
        let obj_ctx_solo = RenderContext::new("mob_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &wolves)
            .with_entity("target", &goblin);

        assert_eq!(
            PerspectiveEngine::render(&obj_template, &obj_ctx_solo).unwrap(),
            "The goblin ambushes us!"
        );

        // 2. Mixed group containing plural viewer
        let obj_ctx_group = RenderContext::new("mob_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &party)
            .with_entity("target", &goblin);

        // A pronoun referring to a group that includes a 1st-person viewer correctly collapses to "us"
        assert_eq!(
            PerspectiveEngine::render(&obj_template, &obj_ctx_group).unwrap(),
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
            .get_or_compile("You take {the:source's} gold.")
            .unwrap();

        // 1. Second Person Mixed -> "your and the goblin's"
        let ctx_second = RenderContext::new("char_1").with_entity("source", &mixed_party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_second).unwrap(),
            "You take your and the goblin's gold."
        );

        // 2. First Person Mixed -> "the goblin's and my"
        let ctx_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &mixed_party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_first).unwrap(),
            "You take the goblin's and my gold."
        );

        // 3. Second Person Solo -> "your"
        let ctx_solo_second = RenderContext::new("char_1").with_entity("source", &solo_party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_solo_second).unwrap(),
            "You take your gold."
        );

        // 4. First Person Solo -> "my"
        let ctx_solo_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &solo_party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_solo_first).unwrap(),
            "You take my gold."
        );

        // 5. Third Person Mixed -> "Aldran and the goblin's"
        let ctx_third = RenderContext::new("char_2").with_entity("source", &mixed_party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_third).unwrap(),
            "You take Aldran and the goblin's gold."
        );

        // 6. First Person Big Mixed -> "the goblin's, the slime's, and my"
        let ctx_first_big = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &big_mixed_party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_first_big).unwrap(),
            "You take the goblin's, the slime's, and my gold."
        );

        // 7. Second Person Big Mixed -> "your, the goblin's, and the slime's"
        let ctx_second_big = RenderContext::new("char_1").with_entity("source", &big_mixed_party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_second_big).unwrap(),
            "You take your, the goblin's, and the slime's gold."
        );

        // 8. First Person Plural Solo -> "our"
        let ctx_first_plural_solo = RenderContext::new("mob_3")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &solo_wolves_party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_first_plural_solo).unwrap(),
            "You take our gold."
        );

        // 9. First Person Plural Mixed -> "your, the goblin's, and my"
        let ctx_first_plural_mixed = RenderContext::new("mob_3")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &mixed_wolves_party);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_first_plural_mixed).unwrap(),
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
            .get_or_compile("{the:goblin} [goblin:ambush] {party}. {party:Subj} [party:retaliate]!")
            .unwrap();
        let ctx1 = RenderContext::new("char_3")
            .with_entity("party", &party)
            .with_entity("goblin", &goblin);

        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx1).unwrap(),
            "The goblin ambushes Aldran and Bob. They retaliate!"
        );

        // 2. Ambiguous Group Pronoun (Monsters and Party are both Plural)
        let t2 = cache
            .get_or_compile(
                "{the:monsters} [monsters:ambush] {party}. {party:Subj} [party:retaliate]!",
            )
            .unwrap();
        let ctx2 = RenderContext::new("char_3")
            .with_entity("party", &party)
            .with_entity("monsters", &monsters);

        // The anaphora ambiguity check should safely catch the collision and fall back to the group's name.
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx2).unwrap(),
            "The goblin and the slime ambush Aldran and Bob. Aldran and Bob retaliate!"
        );

        // 3. Ambiguous Group Pronoun with Viewer Included (Actor Stance)
        // Because "you" (or "we") is unambiguous regardless of other entities, it securely bypasses ambiguity checks!
        let ctx3 = RenderContext::new("char_1")
            .with_entity("party", &party)
            .with_entity("monsters", &monsters);

        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx3).unwrap(),
            "The goblin and the slime ambush you and Bob. You retaliate!"
        );

        // 4. First Person Stance with Viewer
        let ctx4 = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("party", &party)
            .with_entity("monsters", &monsters);

        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx4).unwrap(),
            "The goblin and the slime ambush Bob and I. We retaliate!"
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
            .get_or_compile("{the:target} [target:nod]. {target:Subj} [target:smile].")
            .unwrap();
        let ctx1 = RenderContext::new("viewer").with_entity("target", &nested_solo);
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx1).unwrap(),
            "Aldran nods. He smiles."
        );

        // 2. Ambiguity with Nested Solo (Male) and Bob (Male)
        let t2 = cache
            .get_or_compile("{bob} [bob:look] at {target}. {target:Subj} [target:smile].")
            .unwrap();
        let ctx2 = RenderContext::new("viewer")
            .with_entity("bob", &bob)
            .with_entity("target", &nested_solo);
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx2).unwrap(),
            "Bob looks at Aldran. Aldran smiles."
        );

        // 3. Nested Plural -> Acts as Plural
        let t3 = cache
            .get_or_compile("{the:party} [party:nod]. {party:Subj} [party:smile].")
            .unwrap();
        let ctx3 = RenderContext::new("viewer").with_entity("party", &nested_plural);
        assert_eq!(
            PerspectiveEngine::render(&t3, &ctx3).unwrap(),
            "Aldran and Bob nod. They smile."
        );

        // 4. Ambiguity with Nested Plural (Plural) and Wolves (Plural)
        let t4 = cache
            .get_or_compile("{the:wolves} [wolves:look] at {party}. {party:Subj} [party:smile].")
            .unwrap();
        let ctx4 = RenderContext::new("viewer")
            .with_entity("wolves", &wolves)
            .with_entity("party", &nested_plural);
        assert_eq!(
            PerspectiveEngine::render(&t4, &ctx4).unwrap(),
            "The wolves look at Aldran and Bob. Aldran and Bob smile."
        );
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
        let template = cache.get_or_compile("{source:Subj} [source:nod].").unwrap();

        let ctx_injected = RenderContext::new("viewer")
            .with_entity("source", &player)
            .with_anaphora(empty_state);

        // Because the state is completely empty, it should safely fall back to the full name instead of using a pronoun.
        let out = PerspectiveEngine::render(&template, &ctx_injected).unwrap();
        assert_eq!(out, "Aldran nods.");
    }

    #[test]
    fn test_empty_template_string() {
        let cache = TemplateCache::new(100);
        let template = cache.get_or_compile("").unwrap();

        let ctx = RenderContext::new("viewer");
        let output = PerspectiveEngine::render(&template, &ctx).unwrap();

        assert_eq!(output, "");
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
        let _ = PerspectiveEngine::render(&cache.get_or_compile("{tom} arrives.").unwrap(), &ctx)
            .unwrap();

        // If `with_last_mentioned` accidentally cleared Bob's flags, he would have been evicted as the oldest.
        // Because his IS_PINNED flag was preserved, Tom (the newest but unpinned) is instantly evicted instead!
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
        let _ = PerspectiveEngine::render(&cache.get_or_compile("{tom} arrives.").unwrap(), &ctx2)
            .unwrap();

        // Because the state extraction/injection preserved Bob's IS_PINNED flag, Tom is evicted instead.
        assert_eq!(ctx2.recent_entities.borrow().len(), 1);
        assert_eq!(ctx2.recent_entities.borrow()[0].key, "bob");
    }

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
        let template_fly = cache.get_or_compile("{source} [source:fly].").unwrap();
        assert_eq!(
            render_msg!("char_1", &template_fly, "source" => &player).unwrap(),
            "You fly."
        );
        assert_eq!(
            render_msg!("char_2", &template_fly, "source" => &player).unwrap(),
            "Aldran flies."
        );

        // 2. "run" -> "ran" (Dynamic past tense shift)
        let template_run = cache.get_or_compile("{source} [source:run].").unwrap();
        let ctx_actor_past = RenderContext::new("char_1")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &player);
        assert_eq!(
            PerspectiveEngine::render(&template_run, &ctx_actor_past).unwrap(),
            "You ran."
        );
        let ctx_director_past = RenderContext::new("char_2")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &player);
        assert_eq!(
            PerspectiveEngine::render(&template_run, &ctx_director_past).unwrap(),
            "Aldran ran."
        );

        // 3. "catch" -> "catches"
        let template_catch = cache.get_or_compile("{source} [source:catch] it.").unwrap();
        assert_eq!(
            render_msg!("char_2", &template_catch, "source" => &player).unwrap(),
            "Aldran catches it."
        );

        // 4. Fallback rule: consonant + y -> ies ("try" -> "tries")
        let template_try = cache.get_or_compile("{source} [source:try].").unwrap();
        assert_eq!(
            render_msg!("char_2", &template_try, "source" => &player).unwrap(),
            "Aldran tries."
        );

        // 5. Fallback rule: ends with x -> es ("box" -> "boxes")
        let template_box = cache.get_or_compile("{source} [source:box].").unwrap();
        assert_eq!(
            render_msg!("char_2", &template_box, "source" => &player).unwrap(),
            "Aldran boxes."
        );

        // 6. Modal verbs natively injected via build.rs
        // This ensures colliding verbs (e.g. "cans" or "wills") don't overwrite modal behaviors.
        let modals = [
            "can", "could", "will", "would", "shall", "should", "may", "might", "must", "ought",
        ];
        for modal in modals {
            let template_str = format!("{{source}} [source:{modal}].");
            let template_modal = cache.get_or_compile(&template_str).unwrap();
            assert_eq!(
                render_msg!("char_1", &template_modal, "source" => &player).unwrap(),
                format!("You {modal}.")
            );
            assert_eq!(
                render_msg!("char_2", &template_modal, "source" => &player).unwrap(),
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
        let template_be = cache.get_or_compile("{source} [source:be] ready.").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&template_be, &ctx_first).unwrap(),
            "I am ready."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_be, &ctx_second).unwrap(),
            "You are ready."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_be, &ctx_third).unwrap(),
            "Aldran is ready."
        );

        // 2. "was" (Handled dynamically in past tense by stance and perspective overrides)
        let ctx_first_past = ctx_first.with_tense(crate::models::Tense::Past);
        let ctx_second_past = ctx_second.with_tense(crate::models::Tense::Past);
        let ctx_third_past = ctx_third.with_tense(crate::models::Tense::Past);

        assert_eq!(
            PerspectiveEngine::render(&template_be, &ctx_first_past).unwrap(),
            "I was ready."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_be, &ctx_second_past).unwrap(),
            "You were ready."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_be, &ctx_third_past).unwrap(),
            "Aldran was ready."
        );

        // 3. Ensure first person leaves irregular and algorithmically modified verbs uninflected
        let ctx_first = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_entity("source", &player);

        let template_fly = cache.get_or_compile("{source} [source:fly].").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&template_fly, &ctx_first).unwrap(),
            "I fly."
        );

        let template_catch = cache.get_or_compile("{source} [source:catch] it.").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&template_catch, &ctx_first).unwrap(),
            "I catch it."
        );

        let template_try = cache.get_or_compile("{source} [source:try].").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&template_try, &ctx_first).unwrap(),
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
            .get_or_compile("{source} [source:have] a sword.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&template_have, &ctx_first).unwrap(),
            "I have a sword."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_have, &ctx_second).unwrap(),
            "You have a sword."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_have, &ctx_third).unwrap(),
            "Aldran has a sword."
        );

        let ctx_first_past = ctx_first.with_tense(crate::models::Tense::Past);
        let ctx_second_past = ctx_second.with_tense(crate::models::Tense::Past);
        let ctx_third_past = ctx_third.with_tense(crate::models::Tense::Past);

        assert_eq!(
            PerspectiveEngine::render(&template_have, &ctx_first_past).unwrap(),
            "I had a sword."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_have, &ctx_second_past).unwrap(),
            "You had a sword."
        );
        assert_eq!(
            PerspectiveEngine::render(&template_have, &ctx_third_past).unwrap(),
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
        crate::grammar::add_irregular_verb("yeet", "yeetses", "yeeted").unwrap();

        let template = cache.get_or_compile("{source} [source:yeet].").unwrap();
        assert_eq!(
            render_msg!("char_2", &template, "source" => &player).unwrap(),
            "Aldran yeetses."
        );

        // Attempting to add an existing PHF verb should fail
        assert!(crate::grammar::add_irregular_verb("arise", "arises not", "arose not").is_err());

        // Forcing an existing PHF verb should succeed and override
        crate::grammar::force_add_irregular_verb("arise", "arizez", "arouze");

        let template_arise = cache.get_or_compile("{source} [source:arise].").unwrap();
        assert_eq!(
            render_msg!("char_2", &template_arise, "source" => &player).unwrap(),
            "Aldran arizez."
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

        // Ergonomically register multiple custom verbs at once
        crate::register_custom_verbs! {
            "bloop" => ("bloopses", "bloopeded"),
            "blarg" => ("blargs", "blarged"),
        };

        let ctx_pres = RenderContext::new("char_2").with_entity("source", &player);
        let ctx_past = RenderContext::new("char_2")
            .with_entity("source", &player)
            .with_tense(crate::models::Tense::Past);

        let t1 = cache.get_or_compile("{source} [source:bloop].").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx_pres).unwrap(),
            "Aldran bloopses."
        );
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx_past).unwrap(),
            "Aldran bloopeded."
        );

        let t2 = cache.get_or_compile("{source} [source:blarg].").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx_pres).unwrap(),
            "Aldran blargs."
        );
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx_past).unwrap(),
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
            .get_or_compile("{source} [source:be|be] looking tired.")
            .unwrap();
        assert_eq!(
            render_msg!("char_1", &template_pirate, "source" => &player).unwrap(),
            "You be looking tired."
        );
        assert_eq!(
            render_msg!("char_2", &template_pirate, "source" => &player).unwrap(),
            "Aldran be looking tired."
        );

        // 2. Force capitalization correctly
        let template_cap = cache.get_or_compile("[source:Look|gaze] at me!").unwrap();
        assert_eq!(
            render_msg!("char_2", &template_cap, "source" => &player).unwrap(),
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
            .get_or_compile("{source} [source:pick up] the sword.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_second).unwrap(),
            "You pick up the sword."
        );
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_third).unwrap(),
            "Aldran picks up the sword."
        );

        let template_cap = cache.get_or_compile("[source:Give up]!").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&template_cap, &ctx_third).unwrap(),
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
            .get_or_compile("{source} [source:look around].")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx).unwrap(),
            "Aldran looks around."
        );

        // 2. Phrasal verb explicitly in PHF ("pinch run" -> "pinch runs")
        let t2 = cache
            .get_or_compile("{source} [source:pinch run].")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx).unwrap(),
            "Aldran pinch runs."
        );

        // 3. Hyphenated verb treated as single word ("cross-pollinate" -> "cross-pollinates")
        let t3 = cache
            .get_or_compile("{source} [source:cross-pollinate].")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t3, &ctx).unwrap(),
            "Aldran cross-pollinates."
        );

        // 4. Runtime dictionary multi-word override ("make do" -> "makes do")
        crate::grammar::add_irregular_verb("make do", "makes do", "made do").unwrap();
        let t4 = cache.get_or_compile("{source} [source:make do].").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t4, &ctx).unwrap(),
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
            .get_or_compile("{source} [source:hit] {the:target} and [source:laugh].")
            .unwrap();

        let ctx_present = RenderContext::new("char_1")
            .with_entity("source", &player)
            .with_entity("target", &goblin);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_present).unwrap(),
            "You hit the goblin and laugh."
        );

        let ctx_past = RenderContext::new("char_1")
            .with_entity("source", &player)
            .with_entity("target", &goblin)
            .with_tense(crate::models::Tense::Past);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_past).unwrap(),
            "You hit the goblin and laughed."
        );

        let ctx_past_director = RenderContext::new("char_2")
            .with_entity("source", &player)
            .with_entity("target", &goblin)
            .with_tense(crate::models::Tense::Past);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_past_director).unwrap(),
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
            .get_or_compile("{the:source} [source:be] here, and [source:try] to escape.")
            .unwrap();

        let ctx_solo_actor = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &player);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_solo_actor).unwrap(),
            "I was here, and tried to escape."
        );

        let ctx_solo_director = RenderContext::new("char_2")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &goblin);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_solo_director).unwrap(),
            "The goblin was here, and tried to escape."
        );

        let ctx_party_actor = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &party);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_party_actor).unwrap(),
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
            let template_str = format!("{{source}} [source:{verb}].");
            let template = cache.get_or_compile(&template_str).unwrap();
            let ctx = RenderContext::new("char_2")
                .with_tense(crate::models::Tense::Past)
                .with_entity("source", &player);

            assert_eq!(
                PerspectiveEngine::render(&template, &ctx).unwrap(),
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

        let template = cache.get_or_compile("{source} [source:catch up].").unwrap();
        let ctx = RenderContext::new("char_2")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &player);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx).unwrap(),
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
            .get_or_compile("{source} [source:freak out|freak out|freaks out].")
            .unwrap();

        assert_eq!(
            PerspectiveEngine::render(&t_pres_only, &ctx_director_present).unwrap(),
            "Aldran freaks out."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_pres_only, &ctx_director_past).unwrap(),
            "Aldran freaked out."
        );

        // 2. Both present and past overrides
        let t_both = cache
            .get_or_compile("{source} [source:be|am|are|is;was|were|was] here.")
            .unwrap();

        let ctx_actor_first_present = RenderContext::new("char_1")
            .with_entity("source", &player)
            .with_stance(crate::models::ActorStance::FirstPerson);
        let ctx_actor_first_past = RenderContext::new("char_1")
            .with_entity("source", &player)
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_tense(crate::models::Tense::Past);

        assert_eq!(
            PerspectiveEngine::render(&t_both, &ctx_actor_first_present).unwrap(),
            "I am here."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_both, &ctx_actor_first_past).unwrap(),
            "I was here."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_both, &ctx_director_present).unwrap(),
            "Aldran is here."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_both, &ctx_director_past).unwrap(),
            "Aldran was here."
        );

        // 3. Past-only override (falls back to native algorithmic conjugation for present)
        let t_past_only = cache
            .get_or_compile("{source} [source:bloop|;blorped].")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_past_only, &ctx_director_present).unwrap(),
            "Aldran bloops."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_past_only, &ctx_director_past).unwrap(),
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
            .get_or_compile("{source} [source:draw] {source:poss} sword to defend {source:reflex}. The victory [source:be] {source:abs_poss}!")
            .unwrap();

        let ctx_present = RenderContext::new("char_2").with_entity("source", &player);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_present).unwrap(),
            "Aldran draws his sword to defend himself. The victory is his!"
        );

        let ctx_past = RenderContext::new("char_2")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &player);

        // Pronouns should not be affected by tense, but all verbs ("draw" -> "drew", "be" -> "was") should shift.
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_past).unwrap(),
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
            .get_or_compile("{source} [source:have] no choice, {source:subj} [source:be] trapped.")
            .unwrap();

        let ctx_present = RenderContext::new("char_2").with_entity("source", &player);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_present).unwrap(),
            "Aldran has no choice, he is trapped."
        );

        let ctx_past = RenderContext::new("char_2")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &player);
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_past).unwrap(),
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
            .get_or_compile("{the:source} [source:strike] {target}. {target:Subj} [target:fall].")
            .unwrap();

        let ctx_past = RenderContext::new("char_3")
            .with_tense(crate::models::Tense::Past)
            .with_entity("source", &goblin)
            .with_entity("target", &player);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_past).unwrap(),
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
            .get_or_compile("{source} [source:lie(lay)] down.")
            .unwrap();
        // 2. lie -> lied (e.g. deceiving)
        let t_lied = cache
            .get_or_compile("{source} [source:lie(lied)] to me.")
            .unwrap();

        let ctx_pres = RenderContext::new("char_2").with_entity("source", &player);
        let ctx_past = RenderContext::new("char_2")
            .with_entity("source", &player)
            .with_tense(crate::models::Tense::Past);

        // In the present tense, both flawlessly evaluate to "lies"
        assert_eq!(
            PerspectiveEngine::render(&t_lay, &ctx_pres).unwrap(),
            "Aldran lies down."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_lied, &ctx_pres).unwrap(),
            "Aldran lies to me."
        );

        // In the past tense, they diverge to their intended meanings!
        assert_eq!(
            PerspectiveEngine::render(&t_lay, &ctx_past).unwrap(),
            "Aldran lay down."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_lied, &ctx_past).unwrap(),
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
        let t_walk = cache.get_or_compile("[source:Walk] away.").unwrap();
        // 2. Irregular Verb
        let t_run = cache.get_or_compile("[source:Run] away.").unwrap();
        // 3. Phrasal Verb
        let t_pick = cache.get_or_compile("[source:Pick up] the sword.").unwrap();
        // 4. "To Be"
        let t_be = cache.get_or_compile("[source:Be] ready.").unwrap();

        let ctx = RenderContext::new("char_2")
            .with_entity("source", &player)
            .with_tense(crate::models::Tense::Past);

        // All of these should retain their first-letter capitalization despite dynamic shifting!
        assert_eq!(
            PerspectiveEngine::render(&t_walk, &ctx).unwrap(),
            "Walked away."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_run, &ctx).unwrap(),
            "Ran away."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_pick, &ctx).unwrap(),
            "Picked up the sword."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_be, &ctx).unwrap(),
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
        let t_can = cache.get_or_compile("{source} [source:can] win.").unwrap();
        let t_will = cache.get_or_compile("{source} [source:will] win.").unwrap();
        let t_shall = cache
            .get_or_compile("{source} [source:shall] win.")
            .unwrap();
        let t_may = cache.get_or_compile("{source} [source:may] win.").unwrap();

        assert_eq!(
            PerspectiveEngine::render(&t_can, &ctx).unwrap(),
            "Aldran could win."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_will, &ctx).unwrap(),
            "Aldran would win."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_shall, &ctx).unwrap(),
            "Aldran should win."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_may, &ctx).unwrap(),
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
        let t_walk = cache.get_or_compile("{source} [source:walk].").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_walk, &ctx).unwrap(),
            "Aldran will walk."
        );

        // 2. Irregular verb
        let t_be = cache.get_or_compile("{source} [source:be] ready.").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_be, &ctx).unwrap(),
            "Aldran will be ready."
        );

        // 3. Phrasal verb
        let t_pick = cache
            .get_or_compile("{source} [source:pick up] the sword.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_pick, &ctx).unwrap(),
            "Aldran will pick up the sword."
        );

        // 4. Modal verbs (should remain unchanged, preventing "will can")
        let t_can = cache.get_or_compile("{source} [source:can] win.").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_can, &ctx).unwrap(),
            "Aldran can win."
        );

        // 5. Capitalization preservation
        let t_cap = cache.get_or_compile("[source:Attack]!").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_cap, &ctx).unwrap(),
            "Will attack!"
        );

        // 6. Forced conjugation ignored natively
        let t_forced = cache
            .get_or_compile("{source} [source:freak out|freak out|freaks out]!")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_forced, &ctx).unwrap(),
            "Aldran will freak out!"
        );

        // 7. Phrasal modal and quasi-modal edge cases
        let t_have_to = cache
            .get_or_compile("{source} [source:have to] win.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_have_to, &ctx).unwrap(),
            "Aldran will have to win."
        );

        let t_ought_to = cache
            .get_or_compile("{source} [source:ought to] win.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_ought_to, &ctx).unwrap(),
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
            .get_or_compile("{source} [source:draw] {source:poss} sword to defend {source:reflex}. The victory [source:be] {source:abs_poss}!")
            .unwrap();

        let ctx_future = RenderContext::new("char_2")
            .with_tense(crate::models::Tense::Future)
            .with_entity("source", &player);

        // Pronouns should not be affected by tense, but all verbs ("draw" -> "will draw", "be" -> "will be") should shift.
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_future).unwrap(),
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
            .get_or_compile("{the:source} [source:strike] {target}. {target:Subj} [target:fall].")
            .unwrap();

        let ctx_future = RenderContext::new("char_3")
            .with_tense(crate::models::Tense::Future)
            .with_entity("source", &goblin)
            .with_entity("target", &player);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_future).unwrap(),
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
            .get_or_compile("{the:source} [source:be] here, and [source:try] to escape.")
            .unwrap();

        let ctx_party_actor = RenderContext::new("char_1")
            .with_stance(crate::models::ActorStance::FirstPerson)
            .with_tense(crate::models::Tense::Future)
            .with_entity("source", &party);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_party_actor).unwrap(),
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
            .get_or_compile("{+source} [+source:win] the battle.")
            .unwrap();

        let ctx_future = RenderContext::new("char_1") // Player is the viewer
            .with_tense(crate::models::Tense::Future)
            .with_entity("source", &player);

        // Even though the viewer is the actor, the `+` syntax forces third person logic
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_future).unwrap(),
            "Aldran will win the battle."
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

        let t_simple = cache.get_or_compile("{source} [source:walk].").unwrap();
        let t_continuous = cache
            .get_or_compile("{source} [source:be] walking.")
            .unwrap();
        let t_perfect = cache
            .get_or_compile("{source} [source:have] walked.")
            .unwrap();
        let t_perfect_continuous = cache
            .get_or_compile("{source} [source:have] been walking.")
            .unwrap();

        let ctx_pres = RenderContext::new("char_2").with_entity("source", &player);
        let ctx_past = RenderContext::new("char_2")
            .with_entity("source", &player)
            .with_tense(crate::models::Tense::Past);
        let ctx_future = RenderContext::new("char_2")
            .with_entity("source", &player)
            .with_tense(crate::models::Tense::Future);

        // 1. Simple Tenses
        assert_eq!(
            PerspectiveEngine::render(&t_simple, &ctx_pres).unwrap(),
            "Aldran walks."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_simple, &ctx_past).unwrap(),
            "Aldran walked."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_simple, &ctx_future).unwrap(),
            "Aldran will walk."
        );

        // 2. Continuous Tenses
        assert_eq!(
            PerspectiveEngine::render(&t_continuous, &ctx_pres).unwrap(),
            "Aldran is walking."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_continuous, &ctx_past).unwrap(),
            "Aldran was walking."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_continuous, &ctx_future).unwrap(),
            "Aldran will be walking."
        );

        // 3. Perfect Tenses
        assert_eq!(
            PerspectiveEngine::render(&t_perfect, &ctx_pres).unwrap(),
            "Aldran has walked."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_perfect, &ctx_past).unwrap(),
            "Aldran had walked."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_perfect, &ctx_future).unwrap(),
            "Aldran will have walked."
        );

        // 4. Perfect Continuous Tenses
        assert_eq!(
            PerspectiveEngine::render(&t_perfect_continuous, &ctx_pres).unwrap(),
            "Aldran has been walking."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_perfect_continuous, &ctx_past).unwrap(),
            "Aldran had been walking."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_perfect_continuous, &ctx_future).unwrap(),
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
            .get_or_compile("{source} [source:do(aux)] not run.")
            .unwrap();

        assert_eq!(
            PerspectiveEngine::render(&t_neg, &ctx_pres).unwrap(),
            "Aldran does not run."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_neg, &ctx_past).unwrap(),
            "Aldran did not run."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_neg, &ctx_future).unwrap(),
            "Aldran will not run."
        );

        // 2. Question Sentence (Capitalized)
        let t_question = cache
            .get_or_compile("[source:Do(aux)] {source:subj} run?")
            .unwrap();

        assert_eq!(
            PerspectiveEngine::render(&t_question, &ctx_pres).unwrap(),
            "Does he run?"
        );
        assert_eq!(
            PerspectiveEngine::render(&t_question, &ctx_past).unwrap(),
            "Did he run?"
        );
        assert_eq!(
            PerspectiveEngine::render(&t_question, &ctx_future).unwrap(),
            "Will he run?"
        );

        // 3. Main Verb (Unannotated "do")
        let t_main = cache
            .get_or_compile("{source} [source:do] the laundry.")
            .unwrap();

        assert_eq!(
            PerspectiveEngine::render(&t_main, &ctx_pres).unwrap(),
            "Aldran does the laundry."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_main, &ctx_past).unwrap(),
            "Aldran did the laundry."
        );
        assert_eq!(
            PerspectiveEngine::render(&t_main, &ctx_future).unwrap(),
            "Aldran will do the laundry."
        );
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
            .get_or_compile("{A:source} [source:walk] in. {A:source} [source:howl].")
            .unwrap();

        let ctx = RenderContext::new("char_1").with_entity("source", &wolf1);

        // First mention uses "A", second uses "The"
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx).unwrap(),
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
                "{A:source} [source:walk] in. {A:other} [other:walk] in. {A:source} [source:howl].",
            )
            .unwrap();

        let ctx = RenderContext::new("char_1")
            .with_entity("source", &wolf1)
            .with_entity("other", &wolf2);

        // First is "A", second is "Another", third is "The first"
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx).unwrap(),
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
            .get_or_compile("{A:source} [source:approach]. {A:source} [source:howl].")
            .unwrap();

        let ctx = RenderContext::new("char_1").with_entity("source", &wolves);

        // First is "Some wolves", second is "The wolves"
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx).unwrap(),
            "Some wolves approach. The wolves howl."
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
            .get_or_compile("{A:source} [source:walk] in. {!A:source} [source:howl].")
            .unwrap();
        let ctx1 = RenderContext::new("char_1").with_entity("source", &wolf1);
        assert_eq!(
            PerspectiveEngine::render(&t_article, &ctx1).unwrap(),
            "A wolf walks in. A wolf howls." // The ! prefix successfully suppressed "The"
        );

        // 2. Suppress pronoun fallback (Ambiguity between wolf1 and wolf2)
        let t_pronoun = cache
            .get_or_compile("{A:source} [source:walk] in. {A:other} [other:walk] in. {source:!Subj} [source:howl].")
            .unwrap();
        let ctx2 = RenderContext::new("char_1")
            .with_entity("source", &wolf1)
            .with_entity("other", &wolf2);

        // Because of `!`, the engine forces "It howls." instead of falling back to "The wolf howls."
        assert_eq!(
            PerspectiveEngine::render(&t_pronoun, &ctx2).unwrap(),
            "A wolf walks in. Another wolf walks in. It howls."
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
            .get_or_compile("{A:source} [source:arrive]. {source:Subj} [source:wait]. {A:source} [source:attack]!")
            .unwrap();

        let ctx = RenderContext::new("char_1").with_entity("source", &goblin);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx).unwrap(),
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
            .get_or_compile("{A:source's} sword [source:fall]. {A:source's} shield [source:break].")
            .unwrap();

        let ctx = RenderContext::new("char_1").with_entity("source", &goblin);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx).unwrap(),
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
            .get_or_compile("{A:source} [source:walk]. {A:source} [source:run].")
            .unwrap();

        let ctx = RenderContext::new("char_1").with_entity("source", &player);

        // The 'A' and 'The' upgrades are both cleanly suppressed because the entity is the viewer
        assert_eq!(
            PerspectiveEngine::render(&template, &ctx).unwrap(),
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
            .get_or_compile("{A:source} [source:draw] {a:weapon}. {A:weapon} [weapon:be] sharp.")
            .unwrap();

        let ctx = RenderContext::new("char_1")
            .with_entity("source", &goblin)
            .with_entity("weapon", &rusty_sword);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx).unwrap(),
            "A goblin draws a rusty sword. The rusty sword is sharp."
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
            .get_or_compile("{source:Subj} [source:howl].")
            .unwrap();
        let ctx1 = RenderContext::new("char_1").with_entity("source", &wolf1);
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx1).unwrap(),
            "A wolf howls."
        );

        // Scenario 2: Pronoun Collision, but Unique Name -> "The"
        // The slime makes the pronoun "It" ambiguous. It falls back to "A", which sees it's unique and upgrades to "The".
        let t2 = cache
            .get_or_compile("{a:source} and {a:slime} arrive. {source:Subj} [source:howl].")
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
            .get_or_compile("{a:source} and {a:other} arrive. {source:Subj} [source:howl].")
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
    fn test_manual_sentence_boundaries() {
        let cache = TemplateCache::new(100);
        let ctx = RenderContext::new("viewer");

        // 1. [SB] forces a sentence boundary
        let t_sb = cache.get_or_compile("wait, [SB]what?").unwrap();
        let out_sb = PerspectiveEngine::render(&t_sb, &ctx).unwrap();
        assert_eq!(out_sb, "Wait, What?");

        // 2. [NO_SB] suppresses a sentence boundary
        let t_no_sb = cache.get_or_compile("apples vs.[NO_SB] oranges.").unwrap();
        let out_no_sb = PerspectiveEngine::render(&t_no_sb, &ctx).unwrap();
        assert_eq!(out_no_sb, "Apples vs. oranges.");

        // 3. Ensuring tags don't output stray whitespace and chain well
        let t_combined = cache.get_or_compile("one.[NO_SB] two[SB] three.").unwrap();
        let out_combined = PerspectiveEngine::render(&t_combined, &ctx).unwrap();
        assert_eq!(out_combined, "One. two Three.");
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
            .get_or_compile("{A:source} and {a:other} arrive.")
            .unwrap();
        let ctx1 = RenderContext::new("char_1")
            .with_entity("source", &wolf1)
            .with_entity("other", &wolf2);

        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx1).unwrap(),
            "A wolf and a large wolf arrive."
        );

        // Clear the anaphora memory so the second template evaluates as a fresh encounter!
        ctx1.clear_anaphora();

        // 2. Pronoun Fallback Upgrades
        let t2 = cache
            .get_or_compile("{A:source} and {a:other} arrive. {other:Subj} [other:howl].")
            .unwrap();

        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx1).unwrap(),
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
            .get_or_compile("{A:source} and {a:other} arrive.")
            .unwrap();
        let ctx1 = RenderContext::new("char_1")
            .with_entity("source", &wolf1)
            .with_entity("other", &wolf2);

        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx1).unwrap(),
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
            .get_or_compile("{A:w1} enters. {A:w2} enters. {A:w3} enters.")
            .unwrap();
        let ctx1 = RenderContext::new("char_1")
            .with_entity("w1", &wolf_scrawny)
            .with_entity("w2", &wolf_large1)
            .with_entity("w3", &wolf_large2);

        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx1).unwrap(),
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
            .get_or_compile("{A:jim} enters. {A:w1} enters. {A:w2} enters. {A:w3} enters.")
            .unwrap();
        let ctx1 = RenderContext::new("char_1")
            .with_entity("jim", &jim)
            .with_entity("w1", &wolf_scrawny)
            .with_entity("w2", &wolf_large1)
            .with_entity("w3", &wolf_large2);

        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx1).unwrap(),
            "Jim enters. A wolf enters. A large wolf enters. Another large wolf enters."
        );
    }

    struct ConfigurableMockEntity {
        id: String,
        name: String,
        long_name: Option<String>,
        gender: Gender,
    }

    impl TemplateEntity for ConfigurableMockEntity {
        fn contains_viewer(&self, viewer_id: &str) -> bool {
            self.id == viewer_id
        }
        fn gender(&self) -> Gender {
            self.gender
        }
        fn is_plural(&self) -> bool {
            self.gender == Gender::Plural
        }
        fn is_proper_noun_for(&self, _: &str) -> bool {
            false
        }
        fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
            Cow::Borrowed(&self.name)
        }
        fn long_display_name_for<'a>(&'a self, _: &str) -> Option<Cow<'a, str>> {
            self.long_name.as_deref().map(Cow::Borrowed)
        }
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
            .get_or_compile("{A:w1} and {a:w2} arrive. {w1:Subj} [w1:bark]. {w2:Subj} [w2:growl].")
            .unwrap();
        let ctx = RenderContext::new("viewer")
            .with_entity("w1", &w1)
            .with_entity("w2", &w2);

        // Because w2 vacates the "wolf" namespace to become "large wolf", w1 correctly
        // realizes it is unique, resolving its pronoun fallback to "The wolf" rather than "A wolf"!
        assert_eq!(
            PerspectiveEngine::render(&t, &ctx).unwrap(),
            "A wolf and a large wolf arrive. The wolf barks. The large wolf growls."
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
            .get_or_compile("{A:w1}, {a:d1}, and {a:w2} arrive.")
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
        let t = cache.get_or_compile("{A:w2} and {a:w3} arrive.").unwrap();
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
        let t = cache.get_or_compile("{A:w1} and {a:w2} arrive.").unwrap();
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
            .get_or_compile("{A:w1} and {a:w2} arrive. {w1:Subj} [w1:growl]. {w2:Subj} [w2:bark].")
            .unwrap();
        let ctx = RenderContext::new("viewer")
            .with_entity("w1", &w1)
            .with_entity("w2", &w2);

        assert_eq!(
            PerspectiveEngine::render(&t, &ctx).unwrap(),
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
            .get_or_compile("{A:w2} and {a:w1} arrive. {w2:Subj} [w2:bark]. {w1:Subj} [w1:growl].")
            .unwrap();
        let ctx = RenderContext::new("viewer")
            .with_entity("w1", &w1)
            .with_entity("w2", &w2);

        assert_eq!(
            PerspectiveEngine::render(&t, &ctx).unwrap(),
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
            .get_or_compile("{A:w1} and {a:w2} arrive. {w1:Subj} [w1:growl]. {w2:Subj} [w2:bark].")
            .unwrap();

        // Without lookahead (left-to-right causal pop-in)
        let ctx_default = RenderContext::new("viewer")
            .with_entity("w1", &w1)
            .with_entity("w2", &w2);
        assert_eq!(
            PerspectiveEngine::render(&t, &ctx_default).unwrap(),
            "A wolf and another wolf arrive. The large wolf growls. The wolf barks."
        );

        // With lookahead: w1 realizes w2 is coming and will cause a collision.
        // It immediately preempts the ambiguity and uses its long name on the very first mention!
        let ctx_lookahead = RenderContext::new("viewer")
            .with_entity("w1", &w1)
            .with_entity("w2", &w2)
            .with_lookahead(true);
        assert_eq!(
            PerspectiveEngine::render(&t, &ctx_lookahead).unwrap(),
            "A large wolf and a wolf arrive. The large wolf growls. The wolf barks."
        );
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
            .get_or_compile("{A:w1} enters. {A:w2} enters. {A:w3} enters. {A:w4} enters.")
            .unwrap();
        let ctx = RenderContext::new("viewer")
            .with_entity("w1", &w1)
            .with_entity("w2", &w2)
            .with_entity("w3", &w3)
            .with_entity("w4", &w4);

        assert_eq!(
            PerspectiveEngine::render(&t, &ctx).unwrap(),
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
            .get_or_compile("{A:w1} walks in. {A:w2} walks in. {The:w1} howls. {The:w2} grins.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx).unwrap(),
            "A wolf walks in. Another wolf walks in. The first wolf howls. The second wolf grins."
        );

        // Forget w2. Now only w1 is in the scene. The engine gracefully drops the ordinals for the lone entity!
        ctx.forget_anaphora("w2");

        let t2 = cache.get_or_compile("{The:w1} sighs.").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx).unwrap(),
            "The wolf sighs."
        );

        // Now add w2 back. W1 gets re-assigned to "1" and W2 gets "2".
        // We also test the pronoun fallback ordinal synergy!
        let t3 = cache
            .get_or_compile(
                "{A:w2} returns. {w1:Subj} [w1:growl] at {the:w2}. {w2:Subj} [w2:flee].",
            )
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t3, &ctx).unwrap(),
            "Another wolf returns. The first wolf growls at the second wolf. The second wolf flees."
        );
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
            .get_or_compile("{One of the:orcs} [-orcs:bellow], and {-orcs:subj} [-orcs:charge]!")
            .unwrap();
        let ctx1 = RenderContext::new("viewer").with_entity("orcs", &orcs);

        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx1).unwrap(),
            "One of the orcs bellows, and it charges!"
        );

        // 2. Singular Override Pronoun Ambiguity Fallback
        // The `-` prefix on the pronoun forces `is_plural = false` and `effective_gender = Neutral`.
        // The goblin is Neutral. This causes an ambiguity!
        // The engine should fallback gracefully to "One of the orcs" instead of "Some orcs".
        let t2 = cache
            .get_or_compile("{One of the:orcs} and {a:goblin} arrive. {-orcs:Subj} [-orcs:bellow].")
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
            .get_or_compile("{-orcs:Subj} [-orcs:charge].")
            .unwrap();

        // 1. Director Stance (Present, Past, Future)
        let ctx_director_pres = RenderContext::new("viewer").with_entity("orcs", &orcs);
        let ctx_director_past = ctx_director_pres
            .clone()
            .with_tense(crate::models::Tense::Past);
        let ctx_director_fut = ctx_director_pres
            .clone()
            .with_tense(crate::models::Tense::Future);

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_director_pres).unwrap(),
            "One of the orcs charges."
        );

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_director_past).unwrap(),
            "One of the orcs charged."
        );

        assert_eq!(
            PerspectiveEngine::render(&template, &ctx_director_fut).unwrap(),
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
        let t_no_override = cache.get_or_compile("{orcs:Subj} [orcs:charge].").unwrap();
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
            .get_or_compile("{A:goblin} snarls. {-orcs:Subj} [-orcs:draw] {-orcs:poss} blade!")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t, &ctx).unwrap(),
            "A goblin snarls. One of the orcs draws its blade!"
        );

        ctx.clear_anaphora();

        // If the orc WASN'T the active subject, the ambiguity would trigger the fallback.
        // But the builder can stack the `!` and `-` modifiers to force the pronoun anyway!
        let t2 = cache
            .get_or_compile("{A:goblin} snarls at {-orcs:obj} and steals {!-orcs:poss} blade!")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx).unwrap(),
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
        let t = cache.get_or_compile("{-orcs:Subj} [-orcs:be|am|are|is] here. {-orcs:Subj} [-orcs:have|have|have|has] arrived!").unwrap();

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
            .get_or_compile("{+!-source:subj} [+source:nod].")
            .unwrap();
        let t2 = cache
            .get_or_compile("{-!+source:subj} [+source:nod].")
            .unwrap();
        let t3 = cache
            .get_or_compile("{!+-source:subj} [+source:nod].")
            .unwrap();

        assert_eq!(PerspectiveEngine::render(&t1, &ctx).unwrap(), "He nods.");
        assert_eq!(PerspectiveEngine::render(&t2, &ctx).unwrap(), "He nods.");
        assert_eq!(PerspectiveEngine::render(&t3, &ctx).unwrap(), "He nods.");
    }

    #[test]
    fn test_nested_properties_returning_proper_nouns() {
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
            } // The weapon is a proper noun!
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
            fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
                if property_name == "weapon" {
                    Some(&self.weapon)
                } else {
                    None
                }
            }
        }

        let arthur = King {
            name: "Arthur".to_string(),
            weapon: Excalibur {
                name: "Excalibur".to_string(),
            },
        };

        let cache = TemplateCache::new(100);
        let ctx = RenderContext::new("viewer").with_entity("source", &arthur);

        // Prove that the dot-notation path bubbles up the proper noun flag dynamically.
        // Neither 'A' nor 'a' should be rendered in the output.
        let t = cache
            .get_or_compile("{A:source} draws {a:source.weapon}.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t, &ctx).unwrap(),
            "Arthur draws Excalibur."
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
        let t = cache.get_or_compile("{A:w1} howls.").unwrap();

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
        let t1 = cache.get_or_compile("{A:source} [source:howl].").unwrap();
        let t2 = cache
            .get_or_compile("{Some:source} [source:howl].")
            .unwrap();
        let t3 = cache
            .get_or_compile("{One of the:source} [source:howl].")
            .unwrap();

        assert_eq!(PerspectiveEngine::render(&t1, &ctx).unwrap(), "We howl.");
        assert_eq!(PerspectiveEngine::render(&t2, &ctx).unwrap(), "We howl.");
        assert_eq!(PerspectiveEngine::render(&t3, &ctx).unwrap(), "We howl.");

        // But if the singular override is attached, it should accurately treat the pack as an individual "I"!
        let t4 = cache.get_or_compile("{-source} [-source:howl].").unwrap();
        assert_eq!(PerspectiveEngine::render(&t4, &ctx).unwrap(), "I howl.");
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
            .get_or_compile("{A:w1} arrive. {A:w2} arrive. {A:w3} arrive.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx).unwrap(),
            "Some wolves arrive. A second set of wolves arrive. A third set of wolves arrive."
        );

        // 2. Plural Demonstratives (This first set, That second set)
        let t2 = cache
            .get_or_compile("{This:w1} howl. {That:w2} howl.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx).unwrap(),
            "This first set of wolves howl. That second set of wolves howl."
        );

        // 3. "One of the" and "Some" explicitly preserving ordinals
        let t3 = cache
            .get_or_compile("{One of the:w1} howls. {Some:w2} howl.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t3, &ctx).unwrap(),
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
            .get_or_compile("{A:p1} arrive. {A:p2} arrive. {A:p3} arrive.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx).unwrap(),
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
            .get_or_compile("{!A:w1}, {!another:w2}, and {!a:w3} arrive.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx).unwrap(),
            "A wolf, another wolf, and a wolf arrive."
        );

        let t2 = cache.get_or_compile("{!The:w1} howls.").unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx).unwrap(),
            "The wolf howls."
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
            .get_or_compile("{The:boss.minions} [boss.minions:attack]!")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx).unwrap(),
            "The goblin and the slime attack!"
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
        let t_intro = cache.get_or_compile("{The:orcs} are here.").unwrap();
        let _ = PerspectiveEngine::render(&t_intro, &ctx).unwrap();

        // Without override (Standard Plural):
        let t1 = cache
            .get_or_compile("{orcs:Subj} [orcs:hurt] {orcs:reflex}.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx).unwrap(),
            "They hurt themselves."
        );

        // With override: Shifts from Plural -> Neutral (It/itself)
        let t2 = cache
            .get_or_compile("{-orcs:Subj} [-orcs:hurt] {-orcs:reflex}.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx).unwrap(),
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
                "{avengers} [avengers:assemble] and [avengers:defend] {avengers:reflex}.",
            )
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx).unwrap(),
            "The Avengers assemble and defend themselves."
        );

        // The singular override cleanly intercepts the verb and pronoun logic, even for proper nouns
        let t2 = cache
            .get_or_compile(
                "{-avengers} [-avengers:assemble] and [-avengers:defend] {-avengers:reflex}.",
            )
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx).unwrap(),
            "The Avengers assembles and defends itself."
        );
    }

    #[test]
    fn test_resolve_target_pronouns() {
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
        let wolves = MockEntity {
            id: "mob_2".to_string(),
            name: "wolves".to_string(),
            gender: Gender::Plural,
            is_plural: true,
            is_proper_noun: false,
        };

        let ctx = RenderContext::new("char_1")
            .with_entity("player", &player)
            .with_entity("goblin", &goblin)
            .with_entity("wolves", &wolves)
            .with_last_mentioned("player")
            .with_last_mentioned("goblin")
            .with_last_mentioned("wolves");

        // Singular male (but viewer)
        let matches = ctx.resolve_target("me");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "player");

        let matches = ctx.resolve_target("you");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "player");

        // Neutral singular
        let matches = ctx.resolve_target("it");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "goblin");

        // Plural
        let matches = ctx.resolve_target("them");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "wolves");
    }

    #[test]
    fn test_resolve_target_names_and_ordinals() {
        let wolf1 = MockEntity {
            id: "w1".to_string(),
            name: "wolf".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
        };
        let wolf2 = MockEntity {
            id: "w2".to_string(),
            name: "wolf".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
        };
        let orc = MockEntity {
            id: "mob_1".to_string(),
            name: "orc".to_string(),
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: false,
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("w1", &wolf1)
            .with_entity("w2", &wolf2)
            .with_entity("orc", &orc);

        // Pre-populate ordinals
        let cache = TemplateCache::new(100);
        let t = cache
            .get_or_compile("{A:w1}, {a:orc}, and {a:w2} arrive.")
            .unwrap();
        PerspectiveEngine::render(&t, &ctx).unwrap();

        // Exact match via name
        let matches = ctx.resolve_target("the orc");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "orc");

        // Ordinals
        let matches = ctx.resolve_target("the first wolf");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "w1");

        let matches = ctx.resolve_target("the 2nd wolf");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "w2");

        // Ambiguous match
        let matches = ctx.resolve_target("a wolf");
        assert_eq!(matches.len(), 2);
        let mut keys: Vec<_> = matches.iter().map(|m| m.key.clone()).collect();
        keys.sort();
        assert_eq!(keys, vec!["w1", "w2"]);
    }

    #[test]
    fn test_resolve_target_sub_elements() {
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
                    "sword" => Some(&self.weapon),
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

        let ctx = RenderContext::new("char_2").with_entity("aldran", &player);

        // Possessive literal name
        let matches = ctx.resolve_target("Aldran's sword");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "aldran");
        assert_eq!(matches[0].path.as_deref(), Some("sword"));
        assert_eq!(matches[0].path_uncertain, false);
        assert_eq!(
            matches[0]
                .resolve_deep_entity()
                .unwrap()
                .display_name_for("viewer"),
            "rusty sword"
        );

        // Possessive literal name, missing path
        let matches2 = ctx.resolve_target("Aldran's shield");
        assert_eq!(matches2.len(), 1);
        assert_eq!(matches2[0].key, "aldran");
        assert_eq!(matches2[0].path.as_deref(), Some("shield"));
        assert_eq!(matches2[0].path_uncertain, true); // Path uncertain
        assert!(matches2[0].resolve_deep_entity().is_none());

        // Possessive pronoun (needs seeded recent_entities)
        let ctx = ctx.with_last_mentioned("aldran");
        let matches3 = ctx.resolve_target("his sword");
        assert_eq!(matches3.len(), 1);
        assert_eq!(matches3[0].key, "aldran");
        assert_eq!(matches3[0].path.as_deref(), Some("sword"));
        assert_eq!(matches3[0].path_uncertain, false);
    }

    #[test]
    fn test_resolve_target_nested_sub_elements() {
        struct Hilt {
            name: String,
        }
        impl TemplateEntity for Hilt {
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

        struct Weapon {
            name: String,
            hilt: Hilt,
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
            fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
                match property_name {
                    "hilt" => Some(&self.hilt),
                    _ => None,
                }
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
                    "sword" => Some(&self.weapon),
                    _ => None,
                }
            }
        }

        let player = Actor {
            name: "Aldran".to_string(),
            weapon: Weapon {
                name: "rusty sword".to_string(),
                hilt: Hilt {
                    name: "leather hilt".to_string(),
                },
            },
        };

        let ctx = RenderContext::new("char_2").with_entity("aldran", &player);

        // Chained possessives should iteratively build and resolve against a dot-notation path
        let matches = ctx.resolve_target("Aldran's sword's hilt");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "aldran");
        assert_eq!(matches[0].path.as_deref(), Some("sword.hilt"));
        assert_eq!(matches[0].path_uncertain, false);
        assert_eq!(
            matches[0]
                .resolve_deep_entity()
                .unwrap()
                .display_name_for("viewer"),
            "leather hilt"
        );

        // Chained possessive pronouns also work
        let ctx2 = ctx.with_last_mentioned("aldran");
        let matches2 = ctx2.resolve_target("his sword's hilt");
        assert_eq!(matches2.len(), 1);
        assert_eq!(matches2[0].key, "aldran");
        assert_eq!(matches2[0].path.as_deref(), Some("sword.hilt"));
        assert_eq!(matches2[0].path_uncertain, false);
        assert_eq!(
            matches2[0]
                .resolve_deep_entity()
                .unwrap()
                .display_name_for("viewer"),
            "leather hilt"
        );

        // Missing deep path results in uncertain match
        let matches3 = ctx2.resolve_target("his sword's gem");
        assert_eq!(matches3.len(), 1);
        assert_eq!(matches3[0].key, "aldran");
        assert_eq!(matches3[0].path.as_deref(), Some("sword.gem"));
        assert_eq!(matches3[0].path_uncertain, true);
        assert!(matches3[0].resolve_deep_entity().is_none());
    }

    #[test]
    fn test_resolve_target_strip_articles() {
        let orcs = MockEntity {
            id: "mob_1".to_string(),
            name: "orcs".to_string(),
            gender: Gender::Plural,
            is_plural: true,
            is_proper_noun: false,
        };

        let ctx = RenderContext::new("viewer").with_entity("orcs", &orcs);

        let matches = ctx.resolve_target("one of the orcs");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "orcs");

        let matches = ctx.resolve_target("some orcs");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "orcs");
    }

    #[test]
    fn test_resolve_target_entity_names_with_articles() {
        let avengers = MockEntity {
            id: "char_1".to_string(),
            name: "the Avengers".to_string(),
            gender: Gender::Plural,
            is_plural: true,
            is_proper_noun: true,
        };

        let shiny_key = MockEntity {
            id: "item_1".to_string(),
            name: "a shiny key".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("avengers", &avengers)
            .with_entity("key", &shiny_key);

        // Player inputs without articles:
        let m1 = ctx.resolve_target("avengers");
        assert_eq!(m1.len(), 1);
        assert_eq!(m1[0].key, "avengers");

        let m2 = ctx.resolve_target("the avengers");
        assert_eq!(m2.len(), 1);
        assert_eq!(m2[0].key, "avengers");

        let m3 = ctx.resolve_target("shiny key");
        assert_eq!(m3.len(), 1);
        assert_eq!(m3[0].key, "key");

        let m4 = ctx.resolve_target("the shiny key");
        assert_eq!(m4.len(), 1);
        assert_eq!(m4[0].key, "key");
    }

    #[test]
    fn test_resolve_target_ordinals_with_embedded_articles() {
        let ring1 = MockEntity {
            id: "r1".to_string(),
            name: "a ring".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
        };
        let ring2 = MockEntity {
            id: "r2".to_string(),
            name: "a ring".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("r1", &ring1)
            .with_entity("r2", &ring2);

        let cache = TemplateCache::new(100);
        let t = cache.get_or_compile("{r1} and {r2} are here.").unwrap();
        PerspectiveEngine::render(&t, &ctx).unwrap(); // Seeds ordinals

        let m1 = ctx.resolve_target("first ring");
        assert_eq!(m1.len(), 1);
        assert_eq!(m1[0].key, "r1");
    }

    #[test]
    fn test_resolve_target_long_descriptions() {
        let wolf_normal = ConfigurableMockEntity {
            id: "w1".into(),
            name: "wolf".into(),
            long_name: None,
            gender: Gender::Neutral,
        };
        let wolf_dire = ConfigurableMockEntity {
            id: "w2".into(),
            name: "wolf".into(),
            long_name: Some("dire wolf".into()),
            gender: Gender::Neutral,
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("w1", &wolf_normal)
            .with_entity("w2", &wolf_dire);

        // Match by short name (ambiguous)
        let matches_short = ctx.resolve_target("wolf");
        assert_eq!(matches_short.len(), 2);

        // Match by long name (unambiguous)
        let matches_long = ctx.resolve_target("dire wolf");
        assert_eq!(matches_long.len(), 1);
        assert_eq!(matches_long[0].key, "w2");

        // Match by long name with an article
        let matches_long_art = ctx.resolve_target("the dire wolf");
        assert_eq!(matches_long_art.len(), 1);
        assert_eq!(matches_long_art[0].key, "w2");
    }

    #[test]
    fn test_resolve_target_ordinals_with_long_descriptions() {
        let w_normal = ConfigurableMockEntity {
            id: "w_normal".into(),
            name: "wolf".into(),
            long_name: None,
            gender: Gender::Neutral,
        };
        let dw1 = ConfigurableMockEntity {
            id: "dw1".into(),
            name: "wolf".into(),
            long_name: Some("dire wolf".into()),
            gender: Gender::Neutral,
        };
        let dw2 = ConfigurableMockEntity {
            id: "dw2".into(),
            name: "wolf".into(),
            long_name: Some("dire wolf".into()),
            gender: Gender::Neutral,
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("w_normal", &w_normal)
            .with_entity("dw1", &dw1)
            .with_entity("dw2", &dw2);

        let cache = TemplateCache::new(100);
        // Render a template to seed the ordinal state. Because their short names ("wolf") collide
        // with w_normal, their long names ("dire wolf") have fewer collisions, so the engine will
        // their long name and assign ordinals.
        let t = cache
            .get_or_compile("{A:w_normal}, {a:dw1}, and {a:dw2} arrive.")
            .unwrap();
        PerspectiveEngine::render(&t, &ctx).unwrap();

        let m1 = ctx.resolve_target("the first dire wolf");
        assert_eq!(m1.len(), 1);
        assert_eq!(m1[0].key, "dw1");

        let m2 = ctx.resolve_target("the 2nd dire wolf");
        assert_eq!(m2.len(), 1);
        assert_eq!(m2[0].key, "dw2");

        // Ambiguous match without the ordinal
        let m3 = ctx.resolve_target("the dire wolf");
        assert_eq!(m3.len(), 2);
    }

    #[test]
    fn test_resolve_target_group_entities() {
        let p1 = MockEntity {
            id: "char_1".into(),
            name: "Aldran".into(),
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: true,
        };
        let p2 = MockEntity {
            id: "char_2".into(),
            name: "Bob".into(),
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: true,
        };

        let party = GroupEntity::new(vec![&p1, &p2]);

        let ctx = RenderContext::new("viewer")
            .with_entity("party", &party)
            .with_last_mentioned("party");

        // Match by plural pronoun
        let m1 = ctx.resolve_target("them");
        assert_eq!(m1.len(), 1);
        assert_eq!(m1[0].key, "party");

        // Match by literal generated group name
        let m2 = ctx.resolve_target("Aldran and Bob");
        assert_eq!(m2.len(), 1);
        assert_eq!(m2[0].key, "party");
    }

    #[test]
    fn test_resolve_target_aliases() {
        struct AliasActor {
            name: &'static str,
            alias_list: Vec<&'static str>,
        }

        impl TemplateEntity for AliasActor {
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
            fn aliases(&self) -> Option<&[&str]> {
                Some(&self.alias_list)
            }
        }

        let boss = AliasActor {
            name: "Lord Aldran",
            alias_list: vec!["Aldran", "the dark lord", "boss"],
        };

        let ctx = RenderContext::new("viewer").with_entity("boss", &boss);

        assert_eq!(ctx.resolve_target("lord aldran").len(), 1);

        assert_eq!(ctx.resolve_target("aldran").len(), 1);
        assert_eq!(ctx.resolve_target("dark lord").len(), 1); // Also verifies article stripping works
        assert_eq!(ctx.resolve_target("boss").len(), 1);
        assert_eq!(ctx.resolve_target("the boss").len(), 1);

        assert_eq!(ctx.resolve_target("king").len(), 0);

        // Verify that partial substring matches do not falsely trigger alias resolution
        assert_eq!(ctx.resolve_target("dark").len(), 0);
        assert_eq!(ctx.resolve_target("lord").len(), 0); // "lord" is in "dark lord" and "Lord Aldran", but not an exact match
        assert_eq!(ctx.resolve_target("the dark").len(), 0);
    }

    #[test]
    fn test_group_entity_aliases() {
        struct AliasActor {
            name: &'static str,
            alias_list: Vec<&'static str>,
        }

        impl TemplateEntity for AliasActor {
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
            fn aliases(&self) -> Option<&[&str]> {
                Some(&self.alias_list)
            }
        }

        let boss = AliasActor {
            name: "Lord Aldran",
            alias_list: vec!["Aldran", "the dark lord", "boss"],
        };

        let group = GroupEntity::new(vec![&boss]);
        let ctx = RenderContext::new("viewer").with_entity("group", &group);

        assert_eq!(ctx.resolve_target("dark lord").len(), 1);
        assert_eq!(ctx.resolve_target("dark lord")[0].key, "group");
    }

    #[test]
    fn test_resolve_target_ambiguous_multiple_matches() {
        struct GuardActor {
            name: &'static str,
            gender: Gender,
            alias_list: Vec<&'static str>,
        }

        impl TemplateEntity for GuardActor {
            fn contains_viewer(&self, _: &str) -> bool {
                false
            }
            fn gender(&self) -> Gender {
                self.gender
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
            fn aliases(&self) -> Option<&[&str]> {
                Some(&self.alias_list)
            }
        }

        let guard1 = GuardActor {
            name: "tall guard",
            gender: Gender::Male,
            alias_list: vec!["guard", "watchman"],
        };
        let guard2 = GuardActor {
            name: "short guard",
            gender: Gender::Male,
            alias_list: vec!["guard", "patrol"],
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("g1", &guard1)
            .with_entity("g2", &guard2)
            // Seed anaphora memory so pronouns evaluate against both
            .with_last_mentioned("g1")
            .with_last_mentioned("g2");

        // 1. Ambiguous Pronoun (Both are Male)
        let matches_pro = ctx.resolve_target("him");
        assert_eq!(matches_pro.len(), 2);

        // 2. Ambiguous Alias ("guard" is shared)
        let matches_alias = ctx.resolve_target("the guard");
        assert_eq!(matches_alias.len(), 2);

        // Unambiguous Alias (for sanity)
        let matches_unambig = ctx.resolve_target("the watchman");
        assert_eq!(matches_unambig.len(), 1);
        assert_eq!(matches_unambig[0].key, "g1");
    }

    #[test]
    fn test_resolve_target_ambiguous_sub_elements() {
        struct Item {
            name: &'static str,
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
                false
            }
            fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
                Cow::Borrowed(self.name)
            }
        }

        struct Mob {
            name: &'static str,
            sword: Option<Item>,
        }
        impl TemplateEntity for Mob {
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
            fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
                Cow::Borrowed(self.name)
            }
            fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
                if property_name == "sword" {
                    self.sword.as_ref().map(|s| s as &dyn TemplateEntity)
                } else {
                    None
                }
            }
        }

        let guard1 = Mob {
            name: "guard",
            sword: Some(Item { name: "iron sword" }),
        };
        let guard2 = Mob {
            name: "guard",
            sword: Some(Item {
                name: "steel sword",
            }),
        };

        let thief1 = Mob {
            name: "thief",
            sword: None,
        };
        let thief2 = Mob {
            name: "thief",
            sword: None,
        };

        let orc1 = Mob {
            name: "orc",
            sword: Some(Item {
                name: "rusty sword",
            }),
        };
        let orc2 = Mob {
            name: "orc",
            sword: None,
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("g1", &guard1)
            .with_entity("g2", &guard2)
            .with_entity("t1", &thief1)
            .with_entity("t2", &thief2)
            .with_entity("o1", &orc1)
            .with_entity("o2", &orc2);

        // 1. Both have the sub-element
        let mut m_guard = ctx.resolve_target("the guard's sword");
        assert_eq!(m_guard.len(), 2);
        m_guard.sort_by_key(|m| m.key.clone());
        assert_eq!(m_guard[0].key, "g1");
        assert_eq!(m_guard[0].path_uncertain, false);
        assert_eq!(m_guard[1].key, "g2");
        assert_eq!(m_guard[1].path_uncertain, false);

        // 2. Neither has the sub-element
        let mut m_thief = ctx.resolve_target("the thief's sword");
        assert_eq!(m_thief.len(), 2);
        m_thief.sort_by_key(|m| m.key.clone());
        assert_eq!(m_thief[0].key, "t1");
        assert_eq!(m_thief[0].path_uncertain, true);
        assert_eq!(m_thief[1].key, "t2");
        assert_eq!(m_thief[1].path_uncertain, true);

        // 3. One has it, the other doesn't
        let mut m_orc = ctx.resolve_target("the orc's sword");
        assert_eq!(m_orc.len(), 2);
        m_orc.sort_by_key(|m| m.key.clone());
        assert_eq!(m_orc[0].key, "o1");
        assert_eq!(m_orc[0].path_uncertain, false); // o1 has it!
        assert_eq!(m_orc[1].key, "o2");
        assert_eq!(m_orc[1].path_uncertain, true); // o2 does not!
    }

    #[test]
    fn test_resolve_target_ordinals_with_sub_elements() {
        struct Item {
            name: &'static str,
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
                false
            }
            fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
                Cow::Borrowed(self.name)
            }
        }

        struct Mob {
            name: &'static str,
            sword: Option<Item>,
        }
        impl TemplateEntity for Mob {
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
            fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
                Cow::Borrowed(self.name)
            }
            fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
                if property_name == "sword" {
                    self.sword.as_ref().map(|s| s as &dyn TemplateEntity)
                } else {
                    None
                }
            }
        }

        let orc1 = Mob {
            name: "orc",
            sword: Some(Item {
                name: "rusty sword",
            }),
        };
        let orc2 = Mob {
            name: "orc",
            sword: None,
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("o1", &orc1)
            .with_entity("o2", &orc2);

        // Seed the ordinals by rendering a template
        let cache = TemplateCache::new(100);
        let t = cache.get_or_compile("{A:o1} and {a:o2} arrive.").unwrap();
        PerspectiveEngine::render(&t, &ctx).unwrap();

        let m1 = ctx.resolve_target("the first orc's sword");
        assert_eq!(m1.len(), 1);
        assert_eq!(m1[0].key, "o1");
        assert_eq!(m1[0].path_uncertain, false); // o1 has the sword!

        let m2 = ctx.resolve_target("the second orc's sword");
        assert_eq!(m2.len(), 1);
        assert_eq!(m2[0].key, "o2");
        assert_eq!(m2[0].path_uncertain, true); // o2 does not have the sword!
    }

    #[test]
    fn test_resolve_target_strict() {
        struct Item {
            name: &'static str,
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
                false
            }
            fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
                Cow::Borrowed(self.name)
            }
        }

        struct Mob {
            name: &'static str,
            sword: Option<Item>,
        }
        impl TemplateEntity for Mob {
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
            fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
                Cow::Borrowed(self.name)
            }
            fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
                if property_name == "sword" {
                    self.sword.as_ref().map(|s| s as &dyn TemplateEntity)
                } else {
                    None
                }
            }
        }

        let orc1 = Mob {
            name: "orc",
            sword: Some(Item {
                name: "rusty sword",
            }),
        };
        let orc2 = Mob {
            name: "orc",
            sword: None,
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("o1", &orc1)
            .with_entity("o2", &orc2);

        // Standard resolve_target returns both, including the uncertain one.
        assert_eq!(ctx.resolve_target("the orc's sword").len(), 2);

        // Strict resolve naturally filters out o2 because its `get_property` returned None!
        let m_strict = ctx.resolve_target_strict("the orc's sword");
        assert_eq!(m_strict.len(), 1);
        assert_eq!(m_strict[0].key, "o1");
        assert_eq!(m_strict[0].path_uncertain, false);
    }

    #[test]
    fn test_resolve_target_shortcut_ordinals() {
        let orc1 = MockEntity {
            id: "o1".to_string(),
            name: "orc".to_string(),
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: false,
        };
        let orc2 = MockEntity {
            id: "o2".to_string(),
            name: "orc".to_string(),
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: false,
        };
        let goblin = MockEntity {
            id: "g1".to_string(),
            name: "goblin".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("o1", &orc1)
            .with_entity("o2", &orc2)
            .with_entity("g1", &goblin);

        let cache = TemplateCache::new(100);
        let t = cache.get_or_compile("{A:o1} and {a:o2} arrive.").unwrap();
        PerspectiveEngine::render(&t, &ctx).unwrap();

        // 1. Postfix syntax for ambiguous items
        let m1 = ctx.resolve_target("orc 1");
        assert_eq!(m1.len(), 1);
        assert_eq!(m1[0].key, "o1");

        let m2 = ctx.resolve_target("orc 2");
        assert_eq!(m2.len(), 1);
        assert_eq!(m2[0].key, "o2");

        let m_first = ctx.resolve_target("first orc");
        assert_eq!(m_first.len(), 1);
        assert_eq!(m_first[0].key, "o1");

        let m_bad_post = ctx.resolve_target("orc1");
        assert_eq!(m_bad_post.len(), 0);

        let m_bad_pre = ctx.resolve_target("firstorc");
        assert_eq!(m_bad_pre.len(), 0);

        // 2. Postfix syntax for unambiguous items (defaults to 1)
        let m3 = ctx.resolve_target("goblin 1");
        assert_eq!(m3.len(), 1);
        assert_eq!(m3[0].key, "g1");

        let m4 = ctx.resolve_target("first goblin");
        assert_eq!(m4.len(), 1);
        assert_eq!(m4[0].key, "g1");

        let m5 = ctx.resolve_target("goblin 2");
        assert_eq!(m5.len(), 0);
    }

    #[test]
    fn test_e2e_combat_round() {
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
            is_proper_noun: bool,
            weapon: Option<Weapon>,
            aliases: &'static [&'static str],
        }
        impl TemplateEntity for Combatant {
            fn contains_viewer(&self, viewer_id: &str) -> bool {
                self.id == viewer_id
            }
            fn gender(&self) -> Gender {
                self.gender
            }
            fn is_plural(&self) -> bool {
                false
            }
            fn is_proper_noun_for(&self, _: &str) -> bool {
                self.is_proper_noun
            }
            fn display_name_for<'a>(&'a self, viewer_id: &str) -> Cow<'a, str> {
                if self.contains_viewer(viewer_id) {
                    Cow::Borrowed("you")
                } else {
                    Cow::Borrowed(self.name)
                }
            }
            fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
                if property_name == "weapon" {
                    self.weapon.as_ref().map(|w| w as &dyn TemplateEntity)
                } else {
                    None
                }
            }
            fn aliases(&self) -> Option<&[&str]> {
                Some(self.aliases)
            }
        }

        let player = Combatant {
            id: "char_1",
            name: "Aldran",
            gender: Gender::Male,
            is_proper_noun: true,
            weapon: Some(Weapon {
                name: "glowing sword",
            }),
            aliases: &[],
        };
        let goblin1 = Combatant {
            id: "mob_1",
            name: "goblin",
            gender: Gender::Neutral,
            is_proper_noun: false,
            weapon: Some(Weapon {
                name: "rusty dagger",
            }),
            aliases: &["scout"],
        };
        let goblin2 = Combatant {
            id: "mob_2",
            name: "goblin",
            gender: Gender::Neutral,
            is_proper_noun: false,
            weapon: Some(Weapon {
                name: "wooden club",
            }),
            aliases: &["brute"], // Same base name to trigger ordinals
        };

        let cache = TemplateCache::new(100);
        let ctx = RenderContext::new("char_1") // The player is the viewer
            .with_entity("player", &player)
            .with_entity("g1", &goblin1)
            .with_entity("g2", &goblin2);

        // 1. Render the encounter (Seeds Ordinals and Anaphora memory)
        let t_intro = cache
            .get_or_compile("{A:g1} and {a:g2} ambush {player}!")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_intro, &ctx).unwrap(),
            "A goblin and another goblin ambush you!"
        );

        // 2. Simulate Player Input: "attack the first goblin"
        let targets = ctx.resolve_target("the first goblin");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].key, "g1");

        // 3. Render the player's combat action
        let t_attack = cache
            .get_or_compile("{player} [player:slash] {the:g1} with {player:poss} {player.weapon}.")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_attack, &ctx).unwrap(),
            "You slash the first goblin with your glowing sword."
        );

        // 4. Enemy 2 retaliates
        let t_retaliate = cache
            .get_or_compile("{The:g2} [g2:swing] {g2:poss} {g2.weapon} at {player:obj}!")
            .unwrap();
        assert_eq!(
            PerspectiveEngine::render(&t_retaliate, &ctx).unwrap(),
            "The second goblin swings its wooden club at you!"
        );

        // 5. Simulate Player Input: "disarm brute's weapon" (Uses alias + sub-element path)
        let targets_alias = ctx.resolve_target_strict("brute's weapon");
        assert_eq!(targets_alias.len(), 1);
        assert_eq!(targets_alias[0].key, "g2");
        assert_eq!(targets_alias[0].path.as_deref(), Some("weapon"));

        let nested_item = targets_alias[0].resolve_deep_entity().unwrap();
        assert_eq!(nested_item.display_name_for("viewer"), "wooden club");

        // 6. Simulate Player Input: "attack it" (Ambiguous pronoun, both neutral goblins are in memory)
        let targets_pronoun = ctx.resolve_target("it");
        assert_eq!(targets_pronoun.len(), 2);
    }

    #[test]
    fn test_resolve_target_unicode_optimizations() {
        struct UnicodeMob {
            name: &'static str,
            aliases: &'static [&'static str],
        }
        impl TemplateEntity for UnicodeMob {
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
            fn aliases(&self) -> Option<&[&str]> {
                Some(self.aliases)
            }
        }

        let w1 = UnicodeMob {
            name: "Ängry Wölf",
            aliases: &["Grümpy Bëast"],
        };
        let w2 = UnicodeMob {
            name: "Ängry Wölf",
            aliases: &[],
        }; // Identical name for ordinals testing
        let o1 = UnicodeMob {
            name: "Mÿstïc Ørc",
            aliases: &[],
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("w1", &w1)
            .with_entity("w2", &w2)
            .with_entity("o1", &o1);

        // Seed ordinals
        let cache = TemplateCache::new(100);
        let t = cache
            .get_or_compile("{A:w1}, {a:w2}, and {a:o1} arrive.")
            .unwrap();
        PerspectiveEngine::render(&t, &ctx).unwrap();

        // 1. Case-insensitive exact match downgrading to ASCII
        let m1 = ctx.resolve_target("angry wolf");
        assert_eq!(m1.len(), 2); // Matches both due to ordinals grouping

        let m_orc = ctx.resolve_target("MYSTIC ORC"); // Fully uppercase ASCII string
        assert_eq!(m_orc.len(), 1);
        assert_eq!(m_orc[0].key, "o1");

        // 2. Case-insensitive article stripping with ASCII
        let m2 = ctx.resolve_target("The angry wolf");
        assert_eq!(m2.len(), 2);

        // 3. Aliases with ASCII and case difference
        let m3 = ctx.resolve_target("grumpy beast");
        assert_eq!(m3.len(), 1);
        assert_eq!(m3[0].key, "w1");

        // 4. Ordinals interacting directly with ASCII mapped to Unicode names
        let m4 = ctx.resolve_target("first angry wolf");
        assert_eq!(m4.len(), 1);
        assert_eq!(m4[0].key, "w1");

        let m5 = ctx.resolve_target("ANGRY WOLF 2"); // Postfix check with differing cases
        assert_eq!(m5.len(), 1);
        assert_eq!(m5[0].key, "w2");
    }

    #[test]
    fn test_resolve_target_strict_diacritics() {
        struct UnicodeMob {
            name: &'static str,
        }
        impl TemplateEntity for UnicodeMob {
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

        let w1 = UnicodeMob {
            name: "Ängry Wölf"
        };

        // Enable strict diacritic matching!
        let ctx = RenderContext::new("viewer")
            .with_entity("w1", &w1)
            .with_strict_diacritics(true);

        // 1. Exact match with correct diacritics still succeeds natively
        assert_eq!(ctx.resolve_target("ängry wölf").len(), 1);

        // 2. ASCII transliteration fails because strict mode is on!
        assert_eq!(ctx.resolve_target("angry wolf").len(), 0);
    }

    #[test]
    fn test_resolve_target_smart_quotes_and_spaces() {
        struct Item {
            name: &'static str,
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
                false
            }
            fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
                Cow::Borrowed(self.name)
            }
        }

        struct Actor {
            name: String,
            weapon: Item,
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
                    "sword" => Some(&self.weapon),
                    _ => None,
                }
            }
        }

        let player = Actor {
            name: "Aldran".to_string(),
            weapon: Item {
                name: "rusty sword",
            },
        };
        let robot = Actor {
            name: "Robot 5".to_string(),
            weapon: Item {
                name: "laser sword",
            },
        };

        let ctx = RenderContext::new("char_2")
            .with_entity("aldran", &player)
            .with_entity("robot", &robot);

        // 1. Smart quotes
        let m1 = ctx.resolve_target("Aldran’s sword");
        assert_eq!(m1[0].path.as_deref(), Some("sword"));

        // 2. Errant spaces before apostrophes should NOT match
        let m2 = ctx.resolve_target("Aldran 's sword");
        assert_eq!(m2.len(), 0);

        // 3. Multiple spaces after apostrophes
        let m3 = ctx.resolve_target("Aldran's   sword");
        assert_eq!(m3[0].path.as_deref(), Some("sword"));

        // 4. Names with numbers
        let m4 = ctx.resolve_target("Robot 5's sword");
        assert_eq!(m4[0].key, "robot");
    }

    #[test]
    fn test_resolve_target_possessive_ending_in_s() {
        struct Item {
            name: &'static str,
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
                false
            }
            fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
                Cow::Borrowed(self.name)
            }
        }

        struct Actor {
            name: String,
            weapon: Item,
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
                Cow::Borrowed(&self.name)
            }
            fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
                match property_name {
                    "sword" | "claws" => Some(&self.weapon),
                    _ => None,
                }
            }
        }

        let lucas = Actor {
            name: "Lucas".to_string(),
            weapon: Item {
                name: "rusty sword",
            },
        };
        let wolves = Actor {
            name: "wolves".to_string(),
            weapon: Item {
                name: "sharp claws",
            },
        };

        let ctx = RenderContext::new("viewer")
            .with_entity("lucas", &lucas)
            .with_entity("wolves", &wolves);

        // 1. Singular name ending in 's' using trailing apostrophe
        let m1 = ctx.resolve_target("Lucas' sword");
        assert_eq!(m1.len(), 1);
        assert_eq!(m1[0].key, "lucas");
        assert_eq!(m1[0].path.as_deref(), Some("sword"));

        // 2. Singular name ending in 's' using `'s`
        let m2 = ctx.resolve_target("Lucas's sword");
        assert_eq!(m2.len(), 1);
        assert_eq!(m2[0].key, "lucas");
        assert_eq!(m2[0].path.as_deref(), Some("sword"));

        // 3. Plural noun ending in 's' using trailing apostrophe
        let m3 = ctx.resolve_target("the wolves' claws");
        assert_eq!(m3.len(), 1);
        assert_eq!(m3[0].key, "wolves");
        assert_eq!(m3[0].path.as_deref(), Some("claws"));

        // 4. Incomplete plural possessive should NOT match the base entity
        let m4 = ctx.resolve_target("the wolves'");
        assert_eq!(m4.len(), 0);

        // 5. Ensure trailing sentence punctuation is still safely stripped
        let m5 = ctx.resolve_target("the wolves.");
        assert_eq!(m5.len(), 1);

        // 6. Incomplete singular possessives ending in 's' should NOT match the base entity
        let m6 = ctx.resolve_target("Lucas'");
        assert_eq!(m6.len(), 0);
        assert_eq!(ctx.resolve_target("Lucas's").len(), 0);
    }

    #[test]
    fn test_trailing_apostrophe_in_template_tag() {
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
        let ctx = RenderContext::new("viewer")
            .with_entity("wolves", &wolves)
            .with_entity("boss", &boss);

        // Builders might naturally write `{wolves'}` instead of `{wolves's}` to denote possession.
        // The parser safely accepts trailing apostrophes and evaluates them as possessives!
        let t1 = cache
            .get_or_compile("You hear {the:wolves'} howls.")
            .unwrap();
        let t2 = cache.get_or_compile("You take {the:boss'} gold.").unwrap();

        assert_eq!(
            PerspectiveEngine::render(&t1, &ctx).unwrap(),
            "You hear the wolves' howls."
        );
        assert_eq!(
            PerspectiveEngine::render(&t2, &ctx).unwrap(),
            "You take the boss's gold."
        );
    }
}
