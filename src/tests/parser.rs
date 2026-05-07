use super::common::MockEntity;
use crate::cache::TemplateCache;
use crate::engine::{PerspectiveEngine, Template};
use crate::models::{Gender, RenderContext, TemplateEntity};
use crate::parser::Token;
use std::borrow::Cow;

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
    let raw_text = "The {*a:source:subj} [source:attack]!";

    // First call - CACHE MISS. The engine compiles the AST.
    let template_1 = cache
        .get_or_compile(raw_text)
        .expect("Failed to compile template");

    // Second call - CACHE HIT. The engine returns the pre-compiled AST.
    let template_2 = cache
        .get_or_compile(raw_text)
        .expect("Failed to compile template");

    let ctx = RenderContext::new("char_1").with_entity("source", &player);

    // Both pointers work with your existing renderer!
    let output_1 = PerspectiveEngine::render(&template_1, &ctx).expect("Failed to render template");
    let output_2 = PerspectiveEngine::render(&template_2, &ctx).expect("Failed to render template");

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
        .get_or_compile("{*the:target:subj} [target:watch] as {*the:source:subj} [source:attack]!")
        .expect("Failed to compile template");

    // BEFORE: The verbose, manual context building
    let manual_ctx = RenderContext::new("char_1")
        .with_entity("source", &wolves)
        .with_entity("target", &player);
    let manual_output =
        PerspectiveEngine::render(&template, &manual_ctx).expect("Failed to render template");

    // AFTER: The clean, single-line macro approach
    let macro_output = render_msg!("char_1", &template,
        "source" => &wolves,
        "target" => &player,
    )
    .expect("Failed to render template");

    // Both should yield the exact same grammatically correct string
    assert_eq!(manual_output, "You watch as the pack of wolves attack!");
    assert_eq!(macro_output, "You watch as the pack of wolves attack!");
}

#[test]
fn test_unclosed_tags_return_errors() {
    let entity_err =
        Template::compile("The {source approaches.").expect_err("Expected compilation to fail");
    assert_eq!(entity_err, "Unclosed entity tag starting at index 4");

    let verb_err =
        Template::compile("The goblin [attack").expect_err("Expected compilation to fail");
    assert_eq!(verb_err, "Unclosed verb tag starting at index 11");
}

#[test]
fn test_malformed_tags_return_errors() {
    let entity_err =
        Template::compile("The {x:y:z} approaches.").expect_err("Expected compilation to fail");
    assert_eq!(entity_err, "Malformed entity tag: {x:y:z}");

    let entity_err4 =
        Template::compile("The {x:y:z:a} approaches.").expect_err("Expected compilation to fail");
    assert_eq!(entity_err4, "Malformed entity tag: {x:y:z:a}");

    let verb_err =
        Template::compile("The goblin [a:b:c]").expect_err("Expected compilation to fail");
    assert_eq!(verb_err, "Malformed verb tag: [a:b:c]");
}

#[test]
fn test_empty_tag_parts_return_errors() {
    let err1 = Template::compile("The {a:} approaches.").expect_err("Expected compilation to fail");
    assert_eq!(err1, "Entity tag has an article but an empty key: {a:}");

    let err2 =
        Template::compile("The {the:} approaches.").expect_err("Expected compilation to fail");
    assert_eq!(err2, "Entity tag has an article but an empty key: {the:}");

    let err3 = Template::compile("The goblin hits {:poss} shield.")
        .expect_err("Expected compilation to fail");
    assert_eq!(err3, "Pronoun tag has an empty key or type: {:poss}");

    let err4 =
        Template::compile("The goblin hits {source:}.").expect_err("Expected compilation to fail");
    assert_eq!(err4, "Pronoun tag has an empty key or type: {source:}");

    let err5 = Template::compile("A {} appears.").expect_err("Expected compilation to fail");
    assert_eq!(err5, "Entity tag has an empty key: {}");

    let err6 =
        Template::compile("The goblin [:attack].").expect_err("Expected compilation to fail");
    assert_eq!(err6, "Verb tag has an empty subject key: [:attack]");

    let err7 = Template::compile("You [source:be|].").expect_err("Expected compilation to fail");
    assert_eq!(
        err7,
        "Verb tag has an empty verb or forced conjugation segment: [source:be|]"
    );

    let err8 = Template::compile("You [source:|be].").expect_err("Expected compilation to fail");
    assert_eq!(
        err8,
        "Verb tag has an empty verb or forced conjugation segment: [source:|be]"
    );

    let err9 =
        Template::compile("You [source:be|am||is].").expect_err("Expected compilation to fail");
    assert_eq!(
        err9,
        "Verb tag has an empty forced present conjugation segment: [source:be|am||is]"
    );

    let err10 = Template::compile("You [source:be|am|are|is|were].")
        .expect_err("Expected compilation to fail");
    assert_eq!(
        err10,
        "Verb tag has too many forced present conjugation segments: [source:be|am|are|is|were]"
    );

    let err11 =
        Template::compile("You [source:be|am|are|is;].").expect_err("Expected compilation to fail");
    assert_eq!(
        err11,
        "Verb tag has an empty forced past conjugation segment: [source:be|am|are|is;]"
    );

    let err12 =
        Template::compile("You [source:be|;was||was].").expect_err("Expected compilation to fail");
    assert_eq!(
        err12,
        "Verb tag has an empty forced past conjugation segment: [source:be|;was||was]"
    );

    let err13 = Template::compile("You [source:be|am|are|is;was|were|was|were].")
        .expect_err("Expected compilation to fail");
    assert_eq!(
        err13,
        "Verb tag has too many forced past conjugation segments: [source:be|am|are|is;was|were|was|were]"
    );

    let err14 =
        Template::compile("The weather is {$ }.").expect_err("Expected compilation to fail");
    assert_eq!(err14, "Variable tag has an empty key: {$ }");

    let err15 = Template::compile("Text {# comment").expect_err("Expected compilation to fail");
    assert_eq!(err15, "Unclosed comment starting at index 5");

    let err16 = Template::compile("Text {% if $a").expect_err("Expected compilation to fail");
    assert_eq!(err16, "Unclosed control tag starting at index 5");

    let err17 = Template::compile("Text {% if $a %} B").expect_err("Expected compilation to fail");
    assert_eq!(err17, "Unclosed {% if %} tag at the end of the template");
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
    let template = cache.get_or_compile("{*A:Source:subj} [source:draw] {*a:source.weapon:obj} and [source:swing] {source's source.weapon:obj}!").expect("Failed to compile template");

    let out_director =
        render_msg!("char_2", &template, "source" => &player).expect("Failed to render template");
    assert_eq!(out_director, "Aldran draws a rusty sword and swings it!");

    let out_actor =
        render_msg!("char_1", &template, "source" => &player).expect("Failed to render template");
    assert_eq!(out_actor, "You draw a rusty sword and swing it!");

    // Verify graceful error handling if a builder requests a property that doesn't exist
    let err_template = cache
        .get_or_compile("{*A:source.shield:subj} breaks.")
        .expect("Failed to compile template");
    let err_output = PerspectiveEngine::render(
        &err_template,
        &RenderContext::new("char_1").with_entity("source", &player),
    )
    .expect_err("Expected an error");
    assert_eq!(err_output, "Missing property 'shield' on entity 'source'");

    // Verify multi-level error handling tracks the traversed path accurately
    let err_template_multi = cache
        .get_or_compile("{*A:source.weapon.edge:subj} is sharp.")
        .expect("Failed to compile template");
    let err_output_multi = PerspectiveEngine::render(
        &err_template_multi,
        &RenderContext::new("char_1").with_entity("source", &player),
    )
    .expect_err("Expected an error");
    assert_eq!(
        err_output_multi,
        "Missing property 'edge' on entity 'source.weapon'"
    );

    // Verify malformed double-dot paths return a clear error at compile time
    let double_dot_err = cache
        .get_or_compile("{*a:source..weapon:subj} is drawn.")
        .expect_err("Expected an error");
    assert_eq!(
        double_dot_err,
        "Entity tag has an empty property segment: {*a:source..weapon:subj}"
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
        .get_or_compile("You look at {*the:tree.child.child.child:obj}.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t1, &ctx).expect("Failed to render template"),
        "You look at the leaf."
    );

    // 2. Graceful error tracking at 4 levels deep
    let t_err = cache
        .get_or_compile("You look at {*the:tree.child.child.child.bug:obj}.")
        .expect("Failed to compile template");
    let err_output = PerspectiveEngine::render(&t_err, &ctx).expect_err("Expected an error");
    assert_eq!(
        err_output,
        "Missing property 'bug' on entity 'tree.child.child.child'"
    );
}

#[test]
fn test_empty_template_string() {
    let cache = TemplateCache::new(100);
    let template = cache
        .get_or_compile("")
        .expect("Failed to compile template");

    let ctx = RenderContext::new("viewer");
    let output = PerspectiveEngine::render(&template, &ctx).expect("Failed to render template");

    assert_eq!(output, "");
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
        .get_or_compile("{*A:source:subj} draws {*a:source.weapon:obj}.")
        .expect("Failed to compile template");
    assert_eq!(
        PerspectiveEngine::render(&t, &ctx).expect("Failed to render template"),
        "Arthur draws Excalibur."
    );
}

#[test]
fn test_inline_adjectives_ast_parsing() {
    // 1. 2-part tag without owner: {Adjectives:Key}
    let t1 = Template::compile("{glowing:sword}").expect("Failed to compile template");
    if let Token::EntityRef {
        key,
        article,
        p_type,
        owner_key,
        adjectives,
        ..
    } = &t1.tokens[0]
    {
        assert_eq!(key, &crate::parser::TagSegment::Literal("sword".into()));
        assert_eq!(article.as_ref(), None);
        assert_eq!(p_type.as_ref(), None);
        assert_eq!(owner_key.as_ref(), None);
        assert_eq!(
            adjectives.as_ref(),
            Some(&crate::parser::TagSegment::Literal("glowing".into()))
        );
    } else {
        panic!("Expected EntityRef token");
    }

    // 2. 3-part tag with Article: {Article:Adjectives:Key}
    let t2 = Template::compile("{The:rusty:shield}").expect("Failed to compile template");
    if let Token::EntityRef {
        key,
        article,
        p_type,
        owner_key,
        adjectives,
        ..
    } = &t2.tokens[0]
    {
        assert_eq!(key, &crate::parser::TagSegment::Literal("shield".into()));
        assert_eq!(
            article.as_ref(),
            Some(&crate::parser::TagSegment::Literal("The".into()))
        );
        assert_eq!(p_type.as_ref(), None);
        assert_eq!(owner_key.as_ref(), None);
        assert_eq!(
            adjectives.as_ref(),
            Some(&crate::parser::TagSegment::Literal("rusty".into()))
        );
    } else {
        panic!("Expected EntityRef token");
    }

    // 3. 3-part tag with Case: {Adjectives:Key:Case}
    let t3 = Template::compile("{big red:wolf:subj}").expect("Failed to compile template");
    if let Token::EntityRef {
        key,
        article,
        p_type,
        owner_key,
        adjectives,
        ..
    } = &t3.tokens[0]
    {
        assert_eq!(key, &crate::parser::TagSegment::Literal("wolf".into()));
        assert_eq!(article.as_ref(), None);
        assert_eq!(
            p_type.as_ref(),
            Some(&crate::parser::TagSegment::Literal("subj".into()))
        );
        assert_eq!(owner_key.as_ref(), None);
        assert_eq!(
            adjectives.as_ref(),
            Some(&crate::parser::TagSegment::Literal("big red".into()))
        );
    } else {
        panic!("Expected EntityRef token");
    }

    // 4. Full 4-part tag without owner: {Article:Adjectives:Key:Case}
    let t4 = Template::compile("{A:big red:wolf:subj}").expect("Failed to compile template");
    if let Token::EntityRef {
        key,
        article,
        p_type,
        owner_key,
        adjectives,
        ..
    } = &t4.tokens[0]
    {
        assert_eq!(key, &crate::parser::TagSegment::Literal("wolf".into()));
        assert_eq!(
            article.as_ref(),
            Some(&crate::parser::TagSegment::Literal("A".into()))
        );
        assert_eq!(
            p_type.as_ref(),
            Some(&crate::parser::TagSegment::Literal("subj".into()))
        );
        assert_eq!(owner_key.as_ref(), None);
        assert_eq!(
            adjectives.as_ref(),
            Some(&crate::parser::TagSegment::Literal("big red".into()))
        );
    } else {
        panic!("Expected EntityRef token");
    }

    // 5. Sanity check: Ensure owner parsing still works! {Owner's Adjectives:Target}
    let t5 = Template::compile("{Aldran's glowing:sword}").expect("Failed to compile template");
    if let Token::EntityRef {
        key,
        article,
        p_type,
        owner_key,
        adjectives,
        ..
    } = &t5.tokens[0]
    {
        assert_eq!(key, &crate::parser::TagSegment::Literal("sword".into()));
        assert_eq!(article.as_ref(), None);
        assert_eq!(p_type.as_ref(), None);
        assert_eq!(
            owner_key.as_ref(),
            Some(&crate::parser::TagSegment::Literal("aldran".into()))
        );
        assert_eq!(
            adjectives.as_ref(),
            Some(&crate::parser::TagSegment::Literal("glowing".into()))
        );
    } else {
        panic!("Expected EntityRef token");
    }
}

#[test]
fn test_dynamic_verb_ast_parsing() {
    let t1 = Template::compile("[source:$action]").expect("Failed to compile template");
    if let Token::VerbRef {
        subject_key,
        dynamic_key,
        ..
    } = &t1.tokens[0]
    {
        assert_eq!(
            subject_key.as_ref(),
            Some(&crate::parser::TagSegment::Literal("source".into()))
        );
        assert_eq!(dynamic_key.as_deref(), Some("action"));
    } else {
        panic!("Expected VerbRef token");
    }
}

#[test]
fn test_dynamic_variable_ast_parsing() {
    let t1 = Template::compile("{$weather}").expect("Failed to compile template");
    if let Token::VariableRef {
        key,
        fallback,
        flags,
    } = &t1.tokens[0]
    {
        assert_eq!(key, "weather");
        assert_eq!(fallback.as_ref(), None);
        assert!(!flags.is_capitalized());
        assert!(!flags.contains(crate::parser::TagFlags::ALL_CAPS));
    } else {
        panic!("Expected VariableRef token");
    }
}

#[test]
fn test_conditional_and_comment_parsing() {
    // 1. Comments should vanish cleanly from the AST
    let t_comment = Template::compile("A {# hidden #} B").expect("Failed to compile template");
    assert_eq!(t_comment.tokens.len(), 1); // "A " and " B" merge organically!
    if let Token::Literal(ref l) = t_comment.tokens[0] {
        assert_eq!(l, "A  B");
    }

    // 2. Conditionals AST structure
    let t_cond = Template::compile(
        "{% if $rain %} Raining {% elif $snow %} Snowing {% else %} Sunny {% endif %}",
    )
    .expect("Failed to compile template");

    assert_eq!(t_cond.tokens.len(), 1);
    if let Token::Conditional { branches, fallback } = &t_cond.tokens[0] {
        assert_eq!(branches.len(), 2);

        // First branch
        assert_eq!(
            branches[0].condition,
            crate::parser::Condition::Value(crate::parser::ConditionValue::Variable(
                "rain".to_string()
            ))
        );
        if let Token::Literal(ref l) = branches[0].body[0] {
            assert_eq!(l, " Raining ");
        }

        // Second branch
        assert_eq!(
            branches[1].condition,
            crate::parser::Condition::Value(crate::parser::ConditionValue::Variable(
                "snow".to_string()
            ))
        );
        if let Token::Literal(ref l) = branches[1].body[0] {
            assert_eq!(l, " Snowing ");
        }

        // Fallback (else)
        let fb = fallback.as_ref().expect("Expected else block");
        if let Token::Literal(ref l) = fb[0] {
            assert_eq!(l, " Sunny ");
        }
    } else {
        panic!("Expected Conditional token");
    }
}

#[test]
fn test_line_continuation() {
    // 1. Unix style (\n)
    let t1 = Template::compile("Hello \\\n    world!").unwrap();
    assert_eq!(t1.tokens.len(), 1);
    if let Token::Literal(ref l) = t1.tokens[0] {
        assert_eq!(l, "Hello world!");
    }

    // 2. Windows style with tabs (\r\n)
    let t2 = Template::compile("Hello \\\r\n\tworld!").unwrap();
    assert_eq!(t2.tokens.len(), 1);
    if let Token::Literal(ref l) = t2.tokens[0] {
        assert_eq!(l, "Hello world!");
    }

    // 3. Trailing backslash shouldn't panic
    let t3 = Template::compile("Trailing slash\\").unwrap();
    if let Token::Literal(ref l) = t3.tokens[0] {
        assert_eq!(l, "Trailing slash\\");
    }
}
