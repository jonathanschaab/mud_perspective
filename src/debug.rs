use crate::engine::{PerspectiveEngine, Template};
use crate::models::{ActorStance, Gender, GroupEntity, RenderContext, TemplateEntity, Tense};
use std::borrow::Cow;

/// A simple entity used for debugging and template permutation testing.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DebugEntity {
    /// The unique identifier of the entity.
    pub id: String,
    /// The display name of the entity.
    pub name: String,
    /// The grammatical gender of the entity.
    pub gender: Gender,
    /// Whether the entity is plural.
    pub is_plural: bool,
    /// Whether the entity is a proper noun.
    pub is_proper_noun: bool,
    /// The combinatorial subset this entity belongs to (e.g., "actors", "objects").
    #[serde(default = "default_subset")]
    pub subset: String,
}

/// Configuration options for a specific entity subset.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SubsetConfig {
    /// Determines whether entities in this subset can be used as the active viewer
    /// for First-Person and Second-Person testing. Defaults to `true`.
    #[serde(default = "default_viewer_capable")]
    pub viewer_capable: bool,
}

fn default_viewer_capable() -> bool {
    true
}

/// The root payload structure for a custom `entities.json` file.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EntitiesPayload {
    /// Optional configuration rules mapped by subset name.
    #[serde(default)]
    pub subsets: std::collections::HashMap<String, SubsetConfig>,
    /// The list of custom entities to load for template testing.
    pub entities: Vec<DebugEntity>,
}

impl EntitiesPayload {
    /// Checks if a given subset name is valid either by explicit configuration or entity presence.
    #[must_use]
    pub fn has_subset(&self, subset_name: &str) -> bool {
        self.subsets.contains_key(subset_name)
            || self.entities.iter().any(|e| e.subset == subset_name)
    }
}

fn default_subset() -> String {
    "actors".to_string()
}

impl TemplateEntity for DebugEntity {
    fn contains_viewer(&self, viewer_id: &str) -> bool {
        self.id == viewer_id
    }
    fn gender(&self) -> Gender {
        self.gender
    }
    fn is_plural(&self) -> bool {
        self.is_plural
    }
    fn is_proper_noun_for(&self, _viewer_id: &str) -> bool {
        self.is_proper_noun
    }
    fn display_name_for<'a>(&'a self, viewer_id: &str) -> Cow<'a, str> {
        if self.contains_viewer(viewer_id) {
            Cow::Borrowed("you")
        } else {
            Cow::Borrowed(&self.name)
        }
    }
}

/// Provides a standard set of diverse entities for testing templates.
/// This includes the viewer, a third-person male proper noun, a female proper noun,
/// a neutral common noun (singular), and a neutral common noun (plural).
#[must_use]
pub fn standard_test_entities() -> Vec<DebugEntity> {
    vec![
        DebugEntity {
            id: "viewer_1".to_string(),
            name: "Aldran".to_string(),
            gender: Gender::Male,
            is_plural: false,
            is_proper_noun: true,
            subset: "actors".to_string(),
        },
        DebugEntity {
            id: "char_2".to_string(),
            name: "Elara".to_string(),
            gender: Gender::Female,
            is_plural: false,
            is_proper_noun: true,
            subset: "actors".to_string(),
        },
        DebugEntity {
            id: "mob_1".to_string(),
            name: "goblin".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
            subset: "actors".to_string(),
        },
        DebugEntity {
            id: "mob_2".to_string(),
            name: "wolves".to_string(),
            gender: Gender::Plural,
            is_plural: true,
            is_proper_noun: false,
            subset: "actors".to_string(),
        },
        DebugEntity {
            id: "item_1".to_string(),
            name: "rusty sword".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
            subset: "objects".to_string(),
        },
        DebugEntity {
            id: "char_3".to_string(),
            name: "Iris".to_string(),
            gender: Gender::Female,
            is_plural: false,
            is_proper_noun: true,
            subset: "actors".to_string(),
        },
        DebugEntity {
            id: "mob_3".to_string(),
            name: "octopus".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
            subset: "actors".to_string(),
        },
        DebugEntity {
            id: "item_2".to_string(),
            name: "Excalibur".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: true,
            subset: "objects".to_string(),
        },
        DebugEntity {
            id: "item_3".to_string(),
            name: "arbalest".to_string(),
            gender: Gender::Neutral,
            is_plural: false,
            is_proper_noun: false,
            subset: "objects".to_string(),
        },
    ]
}

/// Runs a template through a comprehensive battery of tests using standard entities,
/// tenses, and stances, and returns every possible rendered output.
///
/// This tests singulars, plurals, proper nouns, common nouns, and group entities across
/// all tenses and stances.
///
/// # Arguments
/// * `template` - The compiled template to test.
///
/// # Errors
/// Returns an error if the template requires too many combinations to render safely, or if the
/// internal standard test entities list is modified to contain fewer than 3 entities.
pub fn test_template_with_standard_entities<S: std::hash::BuildHasher>(
    template: &Template,
    bindings: &std::collections::HashMap<String, String, S>,
    lookahead: bool,
) -> Result<Vec<String>, String> {
    let entities = standard_test_entities();
    let mut subsets: std::collections::HashMap<String, Vec<&dyn TemplateEntity>> =
        std::collections::HashMap::new();

    let mut actors = Vec::new();
    let mut objects = Vec::new();
    let mut viewer_ids = vec!["bystander_1".to_string()];

    for e in &entities {
        if e.subset == "objects" {
            objects.push(e as &dyn TemplateEntity);
        } else {
            actors.push(e as &dyn TemplateEntity);
            viewer_ids.push(e.id.clone());
        }
    }

    let group = GroupEntity::new(vec![
        *actors.first().ok_or("Missing viewer entity")?,
        *actors.get(2).ok_or("Missing mob entity")?,
    ]);
    actors.push(&group);

    subsets.insert("actors".to_string(), actors);
    subsets.insert("objects".to_string(), objects);

    let stances = vec![
        ActorStance::FirstPerson,
        ActorStance::SecondPerson,
        ActorStance::ThirdPerson,
    ];
    let tenses = vec![Tense::Present, Tense::Past, Tense::Future];

    generate_template_permutations(
        template,
        &viewer_ids,
        &stances,
        &tenses,
        &subsets,
        bindings,
        lookahead,
    )
}

/// Generates all possible string outputs for a given template by permuting
/// through the provided stances, tenses, and entities.
///
/// This is a diagnostic utility for testing templates against all possible
/// grammatical combinations (e.g. self vs other, male vs female, singular vs plural,
/// proper noun vs common noun, present vs past vs future).
///
/// The output strings are prefixed with the combination parameters that generated them.
///
/// # Arguments
/// * `template` - The compiled template to test.
/// * `viewer_ids` - A list of entity IDs to use as the viewer for the render context.
/// * `stances` - A list of stances to test (e.g., `FirstPerson`, `SecondPerson`, `ThirdPerson`).
/// * `tenses` - A list of tenses to test (e.g., `Present`, `Past`, `Future`).
/// * `subsets` - A mapping of subset names (e.g., "actors") to lists of entities.
/// * `bindings` - A mapping of template keys to subset names.
/// * `lookahead` - If true, enables the AST Pre-Pass for collision and ordinal disambiguation.
///
/// # Returns
/// A list of formatted strings describing the inputs and the resulting rendered string.
///
/// # Errors
/// Returns an error if the number of combinations exceeds 100,000, as this indicates an
/// exponentially large test space that will consume excessive memory and time.
pub fn generate_template_permutations<S1: std::hash::BuildHasher, S2: std::hash::BuildHasher>(
    template: &Template,
    viewer_ids: &[String],
    stances: &[ActorStance],
    tenses: &[Tense],
    subsets: &std::collections::HashMap<String, Vec<&dyn TemplateEntity>, S1>,
    bindings: &std::collections::HashMap<String, String, S2>,
    lookahead: bool,
) -> Result<Vec<String>, String> {
    let keys = &template.template_keys;
    let num_keys = keys.len();

    let mut bounds = Vec::with_capacity(num_keys);
    let mut mapped_subsets = Vec::with_capacity(num_keys);
    let mut entity_permutations: usize = 1;

    for key in keys {
        let subset_name = bindings.get(key).map_or("actors", String::as_str);
        let subset = subsets
            .get(subset_name)
            .or_else(|| subsets.get("actors"))
            .ok_or_else(|| {
                format!("Subset '{subset_name}' not found and no 'actors' fallback available.")
            })?;

        if subset.is_empty() {
            return Err(format!("Subset '{subset_name}' is empty."));
        }

        bounds.push(subset.len());
        mapped_subsets.push(subset);
        entity_permutations = entity_permutations
            .checked_mul(subset.len())
            .ok_or_else(|| "Too many combinations: Overflow".to_string())?;
    }

    let stance_tense_combos = stances.len().checked_mul(tenses.len()).unwrap_or(0);
    let viewer_combos = viewer_ids.len().max(1);
    let combinations = stance_tense_combos
        .checked_mul(entity_permutations)
        .and_then(|c| c.checked_mul(viewer_combos))
        .ok_or_else(|| "Too many combinations: Overflow".to_string())?;

    if combinations > 10_000 {
        tracing::warn!(
            "Template permutation threshold reached! {} combinations requested. This may take a long time and use a lot of memory.",
            combinations
        );
    }

    if combinations > 100_000 {
        return Err(format!(
            "Too many combinations: {combinations}. Please test the template in smaller segments or with fewer entities."
        ));
    }

    let mut results = Vec::with_capacity(combinations);
    let mut current_entities = vec![0; num_keys];

    for viewer_id in viewer_ids {
        for stance in stances {
            for tense in tenses {
                for i in 0..entity_permutations {
                    let mut temp = i;
                    for (val, &bound) in current_entities.iter_mut().zip(&bounds) {
                        *val = temp % bound;
                        temp /= bound;
                    }

                    let mut ctx = RenderContext::new(viewer_id)
                        .with_stance(*stance)
                        .with_tense(*tense)
                        .with_lookahead(lookahead);

                    let lookahead_str = if lookahead { ", Lookahead" } else { "" };
                    let mut desc =
                        format!("[Viewer: {viewer_id}, {stance:?}, {tense:?}{lookahead_str}] {{");

                    for (j, key) in keys.iter().enumerate() {
                        let entity_idx = *current_entities
                            .get(j)
                            .ok_or("Internal error: entity index missing")?;
                        let &subset = mapped_subsets
                            .get(j)
                            .ok_or("Internal error: subset missing")?;
                        let &entity = subset
                            .get(entity_idx)
                            .ok_or("Internal error: subset entity missing")?;
                        ctx = ctx.with_entity(key, entity);

                        if j > 0 {
                            desc.push_str(", ");
                        }
                        desc.push_str(key);
                        desc.push_str(": ");
                        desc.push_str(&entity.display_name_for(crate::models::NULL_VIEWER));
                    }
                    desc.push_str("} -> ");

                    match PerspectiveEngine::render(template, &ctx) {
                        Ok(res) => {
                            desc.push_str(&res);
                            results.push(desc);
                        }
                        Err(e) => {
                            desc.push_str("ERROR: ");
                            desc.push_str(&e);
                            results.push(desc);
                        }
                    }
                }
            }
        }
    }

    Ok(results)
}
