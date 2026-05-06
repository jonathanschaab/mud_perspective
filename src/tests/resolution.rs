use super::common::{ConfigurableMockEntity, MockEntity};
use crate::cache::TemplateCache;
use crate::engine::PerspectiveEngine;
use crate::models::{Gender, GroupEntity, RenderContext, TemplateEntity};
use std::borrow::Cow;

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
        .get_or_compile("{*A:w1:subj}, {*a:orc:subj}, and {*a:w2:subj} arrive.")
        .expect("Failed to compile template");
    PerspectiveEngine::render(&t, &ctx).expect("Failed to render template");

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
            .expect("Failed to resolve deep entity")
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
            .expect("Failed to resolve deep entity")
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
            .expect("Failed to resolve deep entity")
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
    let t = cache
        .get_or_compile("{*A:r1:subj} and {*A:r2:subj} are here.")
        .expect("Failed to compile template");
    PerspectiveEngine::render(&t, &ctx).expect("Failed to render template"); // Seeds ordinals

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
        .get_or_compile("{*A:w_normal:subj}, {*a:dw1:subj}, and {*a:dw2:subj} arrive.")
        .expect("Failed to compile template");
    PerspectiveEngine::render(&t, &ctx).expect("Failed to render template");

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
    let t = cache
        .get_or_compile("{*A:o1:subj} and {*a:o2:subj} arrive.")
        .expect("Failed to compile template");
    PerspectiveEngine::render(&t, &ctx).expect("Failed to render template");

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
    let t = cache
        .get_or_compile("{*A:o1:subj} and {*a:o2:subj} arrive.")
        .expect("Failed to compile template");
    PerspectiveEngine::render(&t, &ctx).expect("Failed to render template");

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
        .get_or_compile("{*A:g1:subj} and {*a:g2:subj} ambush {*a:player:obj}!")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_intro, &ctx).expect("Failed to render template"),
        "A goblin and another goblin ambush you!"
    );

    // 2. Simulate Player Input: "attack the first goblin"
    let targets = ctx.resolve_target("the first goblin");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].key, "g1");

    // 3. Render the player's combat action
    let t_attack = cache
        .get_or_compile("{*A:player:subj} [player:slash] {*the:g1:obj} with {a:player:poss} {*:player.weapon:obj}.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_attack, &ctx).expect("Failed to render template"),
        "You slash the first goblin with your glowing sword."
    );

    // 4. Enemy 2 retaliates
    let t_retaliate = cache
        .get_or_compile(
            "{*The:g2:subj} [g2:swing] {a:g2:poss} {*:g2.weapon:obj} at {a:player:obj}!",
        )
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t_retaliate, &ctx).expect("Failed to render template"),
        "The second goblin swings its wooden club at you!"
    );

    // 5. Simulate Player Input: "disarm brute's weapon" (Uses alias + sub-element path)
    let targets_alias = ctx.resolve_target_strict("brute's weapon");
    assert_eq!(targets_alias.len(), 1);
    assert_eq!(targets_alias[0].key, "g2");
    assert_eq!(targets_alias[0].path.as_deref(), Some("weapon"));

    let nested_item = targets_alias[0]
        .resolve_deep_entity()
        .expect("Failed to resolve deep entity");
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
        .get_or_compile("{*A:w1:subj}, {*a:w2:subj}, and {*a:o1:subj} arrive.")
        .expect("Failed to compile template");
    PerspectiveEngine::render(&t, &ctx).expect("Failed to render template");

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
        .get_or_compile("You hear {*the:wolves':poss} howls.")
        .expect("Failed to compile template");
    let t2 = cache
        .get_or_compile("You take {*the:boss':poss} gold.")
        .expect("Failed to compile template");

    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "You hear the wolves' howls."
    );
    assert_eq!(
        PerspectiveEngine::render(&t2, &ctx).expect("Failed to render template"),
        "You take the boss's gold."
    );
}
