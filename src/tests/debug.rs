#[test]
fn test_debug_standard_entities_permutations() {
    use crate::debug::test_template_with_standard_entities;
    use crate::engine::Template;

    let template = Template::compile("{*A:source:subj} [source:look] around.")
        .expect("Failed to compile template");
    let bindings = std::collections::HashMap::new();
    let results = test_template_with_standard_entities(&template, &bindings, false)
        .expect("Failed to generate permutations");

    // 7 viewers * 3 stances * 3 tenses * 7 actors = 441 permutations
    assert_eq!(results.len(), 441);

    // Spot check a few outputs to ensure correct context mappings and verb generation
    assert!(results.contains(
        &"[Viewer: viewer_1, FirstPerson, Present] {source: Aldran} -> I look around.".to_string()
    ));
    assert!(
        results.contains(
            &"[Viewer: bystander_1, ThirdPerson, Past] {source: Aldran} -> Aldran looked around."
                .to_string()
        )
    );
    assert!(results.contains(
        &"[Viewer: bystander_1, SecondPerson, Future] {source: wolves} -> Some wolves will look around.".to_string()
    ));
}

#[test]
fn test_debug_permutations_limit_error() {
    use crate::debug::{generate_template_permutations, standard_test_entities};
    use crate::engine::Template;
    use crate::models::{ActorStance, TemplateEntity, Tense};

    // 6 standard actors, 8 keys = 6^8 = 1,679,616 entity permutations
    // 3 stances * 1 tense * 1,679,616 = 5,038,848 total combinations > 100,000 threshold
    let template = Template::compile("{*a:a:subj} {*a:b:subj} {*a:c:subj} {*a:d:subj} {*a:e:subj} {*a:f:subj} {*a:g:subj} {*a:h:subj}").expect("Failed to compile template");
    let entities_data = standard_test_entities();
    let entities: Vec<&dyn TemplateEntity> = entities_data
        .iter()
        .map(|e| e as &dyn TemplateEntity)
        .collect();

    let stances = vec![
        ActorStance::FirstPerson,
        ActorStance::SecondPerson,
        ActorStance::ThirdPerson,
    ];
    let tenses = vec![Tense::Present];

    let mut subsets = std::collections::HashMap::new();
    subsets.insert("actors".to_string(), entities);
    let bindings = std::collections::HashMap::new();

    let result = generate_template_permutations(
        &template,
        &["viewer_1".to_string()],
        &stances,
        &tenses,
        &subsets,
        &bindings,
        false,
    );
    assert!(result.is_err());
    assert!(
        result
            .expect_err("Expected an error")
            .contains("Too many combinations")
    );
}

#[test]
fn test_debug_permutations_multiple_entities() {
    use crate::debug::{generate_template_permutations, standard_test_entities};
    use crate::engine::Template;
    use crate::models::{ActorStance, TemplateEntity, Tense};

    let template =
        Template::compile("{*A:source:subj} [source:give] {*the:target:obj} a high five.")
            .expect("Failed to compile template");
    let entities_data = standard_test_entities();

    // Use 2 entities to keep the permutation count small: The player and a goblin
    let entities: Vec<&dyn TemplateEntity> = vec![
        &entities_data[0] as &dyn TemplateEntity, // Aldran (viewer_1)
        &entities_data[2] as &dyn TemplateEntity, // goblin
    ];

    let stances = vec![ActorStance::SecondPerson];
    let tenses = vec![Tense::Present];

    let mut subsets = std::collections::HashMap::new();
    subsets.insert("actors".to_string(), entities);
    let bindings = std::collections::HashMap::new();

    let results = generate_template_permutations(
        &template,
        &["viewer_1".to_string()],
        &stances,
        &tenses,
        &subsets,
        &bindings,
        false,
    )
    .expect("Failed to generate permutations");

    // 1 stance * 1 tense * (2 entities ^ 2 keys) = 4 permutations
    assert_eq!(results.len(), 4);

    // Verify the distinct outputs for two different entities interacting
    assert!(results.contains(&"[Viewer: viewer_1, SecondPerson, Present] {source: Aldran, target: Aldran} -> You give yourself a high five.".to_string()));
    assert!(results.contains(&"[Viewer: viewer_1, SecondPerson, Present] {source: Aldran, target: goblin} -> You give the goblin a high five.".to_string()));
    assert!(results.contains(&"[Viewer: viewer_1, SecondPerson, Present] {source: goblin, target: Aldran} -> A goblin gives you a high five.".to_string()));
    assert!(results.contains(&"[Viewer: viewer_1, SecondPerson, Present] {source: goblin, target: goblin} -> A goblin gives itself a high five.".to_string()));
}

#[test]
fn test_debug_permutations_with_bindings() {
    use crate::debug::{generate_template_permutations, standard_test_entities};
    use crate::engine::Template;
    use crate::models::{ActorStance, TemplateEntity, Tense};
    use std::collections::HashMap;

    let template = Template::compile("{*A:source:subj} [source:pick up] {*the:item:obj}.")
        .expect("Failed to compile template");

    let entities_data = standard_test_entities();
    let mut subsets: HashMap<String, Vec<&dyn TemplateEntity>> = HashMap::new();

    for e in &entities_data {
        subsets
            .entry(e.subset.clone())
            .or_default()
            .push(e as &dyn TemplateEntity);
    }

    // Bind 'item' to 'objects'. 'source' defaults to 'actors'.
    let mut bindings = HashMap::new();
    bindings.insert("item".to_string(), "objects".to_string());

    let stances = vec![ActorStance::SecondPerson];
    let tenses = vec![Tense::Present];

    let results = generate_template_permutations(
        &template,
        &["viewer_1".to_string()],
        &stances,
        &tenses,
        &subsets,
        &bindings,
        false,
    )
    .expect("Failed to generate permutations");

    // 6 actors * 3 objects * 1 stance * 1 tense = 18 permutations
    assert_eq!(results.len(), 18);

    assert!(results.contains(&"[Viewer: viewer_1, SecondPerson, Present] {source: Aldran, item: rusty sword} -> You pick up the rusty sword.".to_string()));
    assert!(results.contains(&"[Viewer: viewer_1, SecondPerson, Present] {source: Elara, item: rusty sword} -> Elara picks up the rusty sword.".to_string()));
    assert!(results.contains(&"[Viewer: viewer_1, SecondPerson, Present] {source: goblin, item: rusty sword} -> A goblin picks up the rusty sword.".to_string()));
    assert!(results.contains(&"[Viewer: viewer_1, SecondPerson, Present] {source: wolves, item: rusty sword} -> Some wolves pick up the rusty sword.".to_string()));
    assert!(results.contains(&"[Viewer: viewer_1, SecondPerson, Present] {source: Iris, item: Excalibur} -> Iris picks up Excalibur.".to_string()));
    assert!(results.contains(&"[Viewer: viewer_1, SecondPerson, Present] {source: octopus, item: arbalest} -> An octopus picks up the arbalest.".to_string()));
}

#[test]
fn test_debug_permutations_subset_exclusion() {
    use crate::debug::{generate_template_permutations, standard_test_entities};
    use crate::engine::Template;
    use crate::models::{ActorStance, TemplateEntity, Tense};
    use std::collections::HashMap;

    let template = Template::compile("{*A:source:subj} [source:pick up] {*the:item:obj}.")
        .expect("Failed to compile template");

    let entities_data = standard_test_entities();
    let mut subsets: HashMap<String, Vec<&dyn TemplateEntity>> = HashMap::new();

    for e in &entities_data {
        subsets
            .entry(e.subset.clone())
            .or_default()
            .push(e as &dyn TemplateEntity);
    }

    let mut bindings = HashMap::new();
    bindings.insert("item".to_string(), "objects".to_string());
    bindings.insert("source".to_string(), "actors".to_string());

    let stances = vec![ActorStance::SecondPerson];
    let tenses = vec![Tense::Present];

    let results = generate_template_permutations(
        &template,
        &["viewer_1".to_string()],
        &stances,
        &tenses,
        &subsets,
        &bindings,
        false,
    )
    .expect("Failed to generate permutations");

    for result in results {
        // Ensure no actor ends up in the item position
        assert!(!result.contains("item: Aldran"));
        assert!(!result.contains("item: Elara"));
        assert!(!result.contains("item: goblin"));
        assert!(!result.contains("item: wolves"));
        assert!(!result.contains("item: Iris"));
        assert!(!result.contains("item: octopus"));

        // Ensure no object ends up in the source position
        assert!(!result.contains("source: rusty sword"));
        assert!(!result.contains("source: Excalibur"));
        assert!(!result.contains("source: arbalest"));
    }
}

#[test]
fn test_debug_permutations_objects_never_viewers() {
    use crate::debug::{generate_template_permutations, standard_test_entities};
    use crate::engine::Template;
    use crate::models::{ActorStance, TemplateEntity, Tense};
    use std::collections::HashMap;

    let template = Template::compile("{*A:source:subj} [source:look] at {*the:item:obj}.")
        .expect("Failed to compile template");

    let entities_data = standard_test_entities();
    let mut subsets: HashMap<String, Vec<&dyn TemplateEntity>> = HashMap::new();

    for e in &entities_data {
        subsets
            .entry(e.subset.clone())
            .or_default()
            .push(e as &dyn TemplateEntity);
    }

    let mut bindings = HashMap::new();
    bindings.insert("source".to_string(), "actors".to_string());
    bindings.insert("item".to_string(), "objects".to_string());

    let stances = vec![
        ActorStance::FirstPerson,
        ActorStance::SecondPerson,
        ActorStance::ThirdPerson,
    ];
    let tenses = vec![Tense::Present];

    let results = generate_template_permutations(
        &template,
        &["viewer_1".to_string()],
        &stances,
        &tenses,
        &subsets,
        &bindings,
        false,
    )
    .expect("Failed to generate permutations");

    // 6 actors * 3 objects * 3 stances * 1 tense = 54 permutations
    assert_eq!(results.len(), 54);

    for result in results {
        // The item (rusty sword) is NEVER the viewer ("viewer_1"), so it should NEVER evaluate
        // to "you", "me", or "us". It will always be "the rusty sword" or "it".
        assert!(!result.contains("at you"));
        assert!(!result.contains("at me"));
        assert!(!result.contains("at us"));

        // Verify that the engine correctly outputted the 3rd person description
        assert!(
            result.contains("rusty sword.")
                || result.contains("Excalibur.")
                || result.contains("arbalest.")
                || result.contains("it.")
        );
    }
}

#[test]
fn test_debug_permutations_subset_errors() {
    use crate::debug::generate_template_permutations;
    use crate::engine::Template;
    use crate::models::{ActorStance, TemplateEntity, Tense};
    use std::collections::HashMap;

    let template =
        Template::compile("{*A:source:subj} [source:look].").expect("Failed to compile template");

    let bindings = HashMap::new();
    let stances = vec![ActorStance::SecondPerson];
    let tenses = vec![Tense::Present];

    // 1. Missing subset (no 'actors' fallback)
    let empty_subsets: HashMap<String, Vec<&dyn TemplateEntity>> = HashMap::new();
    let result_missing = generate_template_permutations(
        &template,
        &["viewer_1".to_string()],
        &stances,
        &tenses,
        &empty_subsets,
        &bindings,
        false,
    );
    assert_eq!(
        result_missing.expect_err("Expected an error"),
        "Subset 'actors' not found and no 'actors' fallback available."
    );

    // 2. Empty subset
    let mut empty_vec_subsets: HashMap<String, Vec<&dyn TemplateEntity>> = HashMap::new();
    empty_vec_subsets.insert("actors".to_string(), vec![]);
    let result_empty = generate_template_permutations(
        &template,
        &["viewer_1".to_string()],
        &stances,
        &tenses,
        &empty_vec_subsets,
        &bindings,
        false,
    );
    assert_eq!(
        result_empty.expect_err("Expected an error"),
        "Subset 'actors' is empty."
    );
}
