use crate::grammar::resolve_article;
use std::borrow::Cow;
use std::collections::HashMap;

/// Represents the grammatical gender of an entity for correct pronoun resolution.
///
/// The `Plural` variant is critical for supporting both literal swarms (e.g., "a pack of wolves")
/// and singular entities that utilize non-binary they/them pronouns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    /// Male grammatical gender (he/him).
    Male,
    /// Female grammatical gender (she/her).
    Female,
    /// Neutral grammatical gender (it/its).
    Neutral,
    /// Plural grammatical gender (they/them). Often used for non-binary or swarms.
    Plural,
}

/// A generic trait implemented by game objects to allow them to be referenced
/// dynamically within text templates.
///
/// By requiring `viewer_id` in its methods, this trait ensures that text rendering
/// is strictly bound to the observer's epistemological state, supporting mechanics
/// like stealth, disguises, and recognition.
pub trait TemplateEntity {
    /// Evaluates whether the given `viewer_id` represents this entity or
    /// is considered a part of this entity (such as a member of a group).
    ///
    /// # Arguments
    /// * `viewer_id` - The unique identifier of the observing entity.
    fn contains_viewer(&self, viewer_id: &str) -> bool;

    /// Returns the grammatical gender of the entity used for pronoun resolution.
    fn gender(&self) -> Gender;

    /// Determines if the entity is treated as grammatically plural.
    ///
    /// This is strictly used for subject-verb agreement to ensure verbs remain
    /// uninflected for swarms or groups (e.g., "the wolves attack" instead of "attacks").
    fn is_plural(&self) -> bool;

    /// Returns the display name of the entity, explicitly tailored to the observer.
    ///
    /// Implementers are responsible for returning `"you"` when `contains_viewer` is true,
    /// ensuring a consistent Actor Stance for both individuals and groups.
    ///
    /// Returning a `Cow` (Clone-on-Write) allows the implementation to borrow the
    /// underlying string in most cases, avoiding heap allocations unless dynamic
    /// formatting (like appending a disguise title) is required.
    ///
    /// # Arguments
    /// * `viewer_id` - The unique identifier of the observing entity.
    fn display_name_for<'a>(&'a self, viewer_id: &str) -> Cow<'a, str>;

    /// Determines if the entity's current identity is a proper noun.
    ///
    /// If `true`, the rendering engine will automatically suppress indefinite (`a/an`)
    /// and definite (`the`) articles. This must be perspective-aware to account for
    /// situations where a stranger sees a common noun ("a tall man") while a friend
    /// sees a proper noun ("Aldran").
    ///
    /// # Arguments
    /// * `viewer_id` - The unique identifier of the observing entity.
    fn is_proper_noun_for(&self, viewer_id: &str) -> bool;

    /// Attempts to downcast this entity to a `GroupEntity`.
    ///
    /// This is used internally by the engine to automatically flatten nested
    /// groups when rendering comma-separated lists.
    #[must_use]
    fn as_group(&self) -> Option<&GroupEntity<'_>> {
        None
    }
}

/// The context environment passed to the rendering engine for a specific view generation.
pub struct RenderContext<'a> {
    /// The unique identifier of the entity actively reading the text.
    pub viewer_id: &'a str,
    /// A mapping of template syntax keys (e.g., "source") to their actual game entities.
    /// Keys are normalized to lowercase by the engine, so ensure your builder mapping uses lowercase keys.
    pub entities: HashMap<&'a str, &'a dyn TemplateEntity>,
}

impl<'a> RenderContext<'a> {
    /// Initializes a new, empty rendering context for the specified viewer.
    ///
    /// # Arguments
    /// * `viewer_id` - The string ID of the observing entity.
    #[must_use]
    pub fn new(viewer_id: &'a str) -> Self {
        Self {
            viewer_id,
            entities: HashMap::new(),
        }
    }

    /// Adds an entity mapping to the context using a fluent builder pattern.
    ///
    /// # Arguments
    /// * `key` - The string key used inside the template tags (e.g., "target").
    /// * `entity` - A reference to the game object implementing `TemplateEntity`.
    #[must_use]
    pub fn with_entity(mut self, key: &'a str, entity: &'a dyn TemplateEntity) -> Self {
        self.entities.insert(key, entity);
        self
    }
}

/// A built-in helper for representing a dynamic group of entities.
///
/// `GroupEntity` automatically aggregates a collection of `TemplateEntity` references.
/// It seamlessly handles Oxford comma formatting, injects "you" if the viewer is in the group,
/// evaluates as plural for verb conjugation, and resolves internal definite articles
/// for common nouns (e.g. outputting "Aldran and the goblin" instead of "Aldran and goblin").
pub struct GroupEntity<'a> {
    /// The list of entities comprising this group.
    pub members: Vec<&'a dyn TemplateEntity>,
}

impl TemplateEntity for GroupEntity<'_> {
    fn as_group(&self) -> Option<&GroupEntity<'_>> {
        Some(self)
    }

    fn contains_viewer(&self, viewer_id: &str) -> bool {
        self.members.iter().any(|m| m.contains_viewer(viewer_id))
    }

    fn gender(&self) -> Gender {
        fn check(members: &[&dyn TemplateEntity], count: &mut usize, single_gender: &mut Gender) {
            for &m in members {
                if let Some(group) = m.as_group() {
                    check(&group.members, count, single_gender);
                } else {
                    *count += 1;
                    if *count == 1 {
                        *single_gender = m.gender();
                    }
                }
            }
        }

        let mut count = 0;
        let mut single_gender = Gender::Plural;

        check(&self.members, &mut count, &mut single_gender);

        if count == 1 {
            single_gender
        } else {
            Gender::Plural
        }
    }

    fn is_plural(&self) -> bool {
        fn check(members: &[&dyn TemplateEntity], count: &mut usize, single_is_plural: &mut bool) {
            for &m in members {
                if let Some(group) = m.as_group() {
                    check(&group.members, count, single_is_plural);
                } else {
                    *count += 1;
                    if *count == 1 {
                        *single_is_plural = m.is_plural();
                    }
                }
            }
        }

        let mut count = 0;
        let mut single_is_plural = false;

        check(&self.members, &mut count, &mut single_is_plural);

        count != 1 || single_is_plural
    }

    fn display_name_for<'b>(&'b self, viewer_id: &str) -> Cow<'b, str> {
        fn collect_visible<'c>(
            members: &[&'c dyn TemplateEntity],
            viewer_id: &str,
            has_viewer: &mut bool,
            visible_others: &mut Vec<(&'c dyn TemplateEntity, Cow<'c, str>)>,
        ) {
            for &m in members {
                if let Some(group) = m.as_group() {
                    collect_visible(&group.members, viewer_id, has_viewer, visible_others);
                } else {
                    let name = m.display_name_for(viewer_id);
                    if name == "you" {
                        *has_viewer = true;
                    } else if !name.is_empty() {
                        visible_others.push((m, name));
                    }
                }
            }
        }

        let mut has_viewer = false;
        let mut visible_others = Vec::new();
        collect_visible(
            &self.members,
            viewer_id,
            &mut has_viewer,
            &mut visible_others,
        );

        let total_visible = visible_others.len() + usize::from(has_viewer);

        if total_visible == 0 {
            return Cow::Borrowed("");
        }

        if total_visible == 1 {
            if has_viewer {
                return Cow::Borrowed("you");
            }
            // Return the single member's raw name so the engine can handle articles properly
            return visible_others.pop().unwrap().1;
        }

        let mut names: Vec<Cow<'b, str>> = Vec::with_capacity(total_visible);

        // If the viewer is in this group, they are always listed first as "you"
        if has_viewer {
            names.push(Cow::Borrowed("you"));
        }

        for (m, name) in visible_others {
            // Dynamically prepend "the " if it is a common noun!
            if let Some(art) = resolve_article(
                "the",
                &name,
                false, // Viewer is handled separately
                m.is_proper_noun_for(viewer_id),
                m.is_plural(),
                false, // Do not force articles for proper nouns
            ) {
                names.push(Cow::Owned(format!("{art}{name}")));
            } else {
                names.push(name);
            }
        }

        if names.len() == 2 {
            Cow::Owned(format!("{} and {}", names[0], names[1]))
        } else {
            let last = names.pop().unwrap();
            Cow::Owned(format!("{}, and {}", names.join(", "), last)) // Oxford comma for 3+ items
        }
    }

    fn is_proper_noun_for(&self, viewer_id: &str) -> bool {
        fn check(
            members: &[&dyn TemplateEntity],
            viewer_id: &str,
            count: &mut usize,
            single_is_proper: &mut bool,
        ) {
            for &m in members {
                if let Some(group) = m.as_group() {
                    check(&group.members, viewer_id, count, single_is_proper);
                } else {
                    *count += 1;
                    if *count == 1 {
                        *single_is_proper = m.is_proper_noun_for(viewer_id);
                    }
                }
            }
        }

        let mut count = 0;
        let mut single_is_proper = true;

        check(&self.members, viewer_id, &mut count, &mut single_is_proper);

        count != 1 || single_is_proper
    }
}
