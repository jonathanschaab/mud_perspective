#[cfg(test)]
mod tests {
    use crate::models::{Gender, RenderContext, TemplateEntity};
    use crate::engine::{PerspectiveEngine, Template};
    use crate::cache::TemplateCache;
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
            // Simple clone for the test enum
            match self.gender {
                Gender::Male => Gender::Male,
                Gender::Female => Gender::Female,
                Gender::Neutral => Gender::Neutral,
                Gender::Plural => Gender::Plural,
            }
        }

        fn is_plural(&self) -> bool {
            self.is_plural
        }

        fn display_name_for<'a>(&'a self, viewer_id: &str) -> Cow<'a, str> {
            // Simulate an epistemological visibility check:
            // If the viewer is a stranger, hide Aldran's real name.
            if viewer_id == "stranger_1" && self.name == "Aldran" {
                Cow::Borrowed("tall man")
            } else {
                Cow::Borrowed(&self.name)
            }
        }

        fn is_proper_noun_for(&self, viewer_id: &str) -> bool {
            // If the stranger sees the masked "tall man", it is no longer a proper noun
            if viewer_id == "stranger_1" && self.name == "Aldran" {
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
        let template = Template::compile("{source} [source:be] looking around for {source:poss} sword.").unwrap();
        
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
            is_proper_noun: true, // Set to false so the stranger accurately gets "A tall man"
        };

        let template = Template::compile("{a:source} [source:approach].").unwrap();
        
        let ctx_stranger = RenderContext::new("stranger_1").with_entity("source", &aldran);
        let stranger_output = PerspectiveEngine::render(&template, &ctx_stranger).unwrap();
        
        // The engine should dynamically add the article "a", and capitalize the sentence
        assert_eq!(stranger_output, "A tall man approaches.");
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
        let template = Template::compile("the {target} watches as the {source} [source:attack]!").unwrap();
        
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
        let template = cache.get_or_compile("{the:target} [target:watch] as the {source} [source:attack]!").unwrap();

        // BEFORE: The verbose, manual context building
        let manual_ctx = RenderContext::new("char_1")
           .with_entity("source", &wolves)
           .with_entity("target", &player);
        let manual_output = PerspectiveEngine::render(&template, &manual_ctx).unwrap();

        // AFTER: The clean, single-line macro approach
        let macro_output = render_msg!("char_1", &template, 
            "source" => &wolves, 
            "target" => &player,
        ).unwrap();

        // Both should yield the exact same grammatically correct string
        assert_eq!(manual_output, "You watch as the pack of wolves attack!");
        assert_eq!(macro_output, "You watch as the pack of wolves attack!");
    }

    #[test]
    fn test_group_entity_perspectives() {
        let player = MockEntity { id: "char_1".to_string(), name: "Aldran".to_string(), gender: Gender::Male, is_plural: false, is_proper_noun: true };
        let ally = MockEntity { id: "char_2".to_string(), name: "Bob".to_string(), gender: Gender::Male, is_plural: false, is_proper_noun: true };
        let enemy = MockEntity { id: "mob_1".to_string(), name: "Goblin".to_string(), gender: Gender::Male, is_plural: false, is_proper_noun: false };
        let stranger = MockEntity { id: "char_3".to_string(), name: "Charlie".to_string(), gender: Gender::Male, is_plural: false, is_proper_noun: true };

        let party = GroupEntity { members: vec![&player, &ally] };
        let big_party = GroupEntity { members: vec![&player, &ally, &stranger] };

        let cache = TemplateCache::new(100);

        // --- SCENARIO 1: Verbs & Display Names ---
        let template_action = cache.get_or_compile("{source} [source:open] the door.").unwrap();

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
        let template_pronoun = cache.get_or_compile("{the:source} [source:attack] {target:obj}!").unwrap();

        // The group is the target, viewer is IN the group -> Expects 1st-person plural "us"
        let member_pronoun = render_msg!("char_1", &template_pronoun, "source" => &enemy, "target" => &party).unwrap();
        assert_eq!(member_pronoun, "The Goblin attacks us!");

        // The group is the target, viewer is OUTSIDE the group -> Expects 3rd-person plural "them"
        let observer_pronoun = render_msg!("char_3", &template_pronoun, "source" => &enemy, "target" => &party).unwrap();
        assert_eq!(observer_pronoun, "The Goblin attacks them!");
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
        let template_must = cache.get_or_compile("{source} [source:must] flee from {the:target}!").unwrap();

        // Actor Stance (Player is the one fleeing)
        let actor_must = render_msg!("char_1", &template_must, "source" => &player, "target" => &goblin).unwrap();
        assert_eq!(actor_must, "You must flee from the Goblin!");

        // Director Stance (A bystander is watching the Player flee)
        // The engine should output "must", NOT "musts"
        let director_must = render_msg!("char_3", &template_must, "source" => &player, "target" => &goblin).unwrap();
        assert_eq!(director_must, "Aldran must flee from the Goblin!");


        // --- TEST 2: Multiple modal verbs ("can" and "will") in a complex sentence ---
        let template_can = cache.get_or_compile("if {source} [source:can] catch {the:target}, {source:subj} [source:will] win.").unwrap();

        // Actor Stance
        let actor_can = render_msg!("char_1", &template_can, "source" => &player, "target" => &goblin).unwrap();
        assert_eq!(actor_can, "If you can catch the Goblin, you will win.");

        // Director Stance
        // The engine should output "can" and "will", NOT "cans" and "wills"
        let director_can = render_msg!("char_3", &template_can, "source" => &player, "target" => &goblin).unwrap();
        assert_eq!(director_can, "If Aldran can catch the Goblin, he will win.");
        
        
        // --- TEST 3: Modal verbs interacting with plural targets ---
        let wolves = MockEntity {
            id: "mob_2".to_string(),
            name: "pack of wolves".to_string(),
            gender: Gender::Plural,
            is_plural: true,
            is_proper_noun: false,
        };
        
        let template_should = cache.get_or_compile("{the:source} [source:should] be careful, or {the:target} [target:might] attack.").unwrap();
        
        let observer_should = render_msg!("char_3", &template_should, "source" => &player, "target" => &wolves).unwrap();
        assert_eq!(observer_should, "Aldran should be careful, or the pack of wolves might attack.");
    }

    #[test]
    fn test_unclosed_tags_return_errors() {
        let entity_err = Template::compile("The {source approaches.").unwrap_err();
        assert_eq!(entity_err, "Unclosed entity tag starting at index 4");

        let verb_err = Template::compile("The goblin [attack").unwrap_err();
        assert_eq!(verb_err, "Unclosed verb tag starting at index 11");
    }

    pub struct GroupEntity<'a> {
        pub members: Vec<&'a dyn TemplateEntity>,
    }

    impl<'a> TemplateEntity for GroupEntity<'a> {
        fn contains_viewer(&self, viewer_id: &str) -> bool {
            self.members.iter().any(|m| m.contains_viewer(viewer_id))
        }

        fn gender(&self) -> Gender {
            Gender::Plural // Forces 'they/them' for bystanders
        }

        fn is_plural(&self) -> bool {
            true // Forces base verbs like "attack"
        }

        fn display_name_for<'b>(&'b self, viewer_id: &str) -> Cow<'b, str> {
            let mut names: Vec<String> = self.members.iter()
               .filter(|m| !m.contains_viewer(viewer_id))
               .map(|m| m.display_name_for(viewer_id).into_owned())
               .collect();

            // If the viewer is in this group, they are always listed first as "you"
            if self.contains_viewer(viewer_id) {
                names.insert(0, "you".to_string());
            }

            let output = match names.len() {
                0 => String::new(),
                1 => names[0].clone(),
                2 => format!("{} and {}", names[0], names[1]),
                _ => {
                    let last = names.pop().unwrap();
                    format!("{}, and {}", names.join(", "), last) // Oxford comma for 3+ items
                }
            };

            Cow::Owned(output)
        }

        fn is_proper_noun_for(&self, _viewer_id: &str) -> bool {
            true 
        }
    }
}