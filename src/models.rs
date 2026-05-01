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
///
/// **Note on Forced Perspectives:** When a template forces the Director Stance
/// (e.g., `{+source}`), the engine temporarily passes a highly unique sentinel string
/// (`"\0__MUD_PERSPECTIVE_NULL_VIEWER__\0"`) as the `viewer_id` to bypass recognition.
/// Ensure your actual entity IDs do not match this sentinel.
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
    /// You should **never** return `"you"` from this method. Simply return the entity's
    /// 3rd-person name (e.g. "Aldran" or "the goblin"). The rendering engine will automatically
    /// substitute "you" when `contains_viewer` returns true.
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

    /// Returns a slice of the entity's members if it acts as a collection/list.
    ///
    /// Used by the rendering engine to automatically distribute template articles
    /// across the individual items and format them as an Oxford comma list.
    #[must_use]
    fn group_members(&self) -> Option<&[&dyn TemplateEntity]> {
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

impl<'a> GroupEntity<'a> {
    /// Creates a new `GroupEntity` representing a list of entities.
    #[must_use]
    pub fn new(members: Vec<&'a dyn TemplateEntity>) -> Self {
        Self { members }
    }
}

/// The maximum recursion depth for group flattening to prevent stack overflows.
const MAX_GROUP_DEPTH: usize = 16;

/// Recursively flattens nested groups into a single 1D list of underlying entities.
pub(crate) fn flatten_group<'c>(
    members: &[&'c dyn TemplateEntity],
    flat_list: &mut Vec<&'c dyn TemplateEntity>,
    depth: usize,
) {
    if depth > MAX_GROUP_DEPTH {
        tracing::warn!("Max group recursion depth exceeded. Truncating group.");
        return;
    }
    for &m in members {
        if let Some(group) = m.group_members() {
            flatten_group(group, flat_list, depth + 1);
        } else {
            flat_list.push(m);
        }
    }
}

impl GroupEntity<'_> {
    /// Returns the single underlying member of this group if it contains exactly one leaf entity.
    /// Returns `None` if the group is empty or contains multiple members.
    fn single_leaf_member(&self) -> Option<&dyn TemplateEntity> {
        fn find_leaves<'c>(
            members: &[&'c dyn TemplateEntity],
            count: &mut usize,
            leaf: &mut Option<&'c dyn TemplateEntity>,
            depth: usize,
        ) {
            if depth > MAX_GROUP_DEPTH || *count > 1 {
                return;
            }
            for &m in members {
                if let Some(group_m) = m.group_members() {
                    find_leaves(group_m, count, leaf, depth + 1);
                    if *count > 1 {
                        return;
                    }
                } else {
                    *count += 1;
                    if *count == 1 {
                        *leaf = Some(m);
                    } else {
                        return;
                    }
                }
            }
        }

        let mut count = 0;
        let mut leaf = None;

        find_leaves(&self.members, &mut count, &mut leaf, 0);

        if count == 1 { leaf } else { None }
    }
}

impl TemplateEntity for GroupEntity<'_> {
    fn group_members(&self) -> Option<&[&dyn TemplateEntity]> {
        Some(&self.members)
    }

    fn contains_viewer(&self, viewer_id: &str) -> bool {
        self.members.iter().any(|m| m.contains_viewer(viewer_id))
    }

    fn gender(&self) -> Gender {
        self.single_leaf_member()
            .map_or(Gender::Plural, TemplateEntity::gender)
    }

    fn is_plural(&self) -> bool {
        self.single_leaf_member()
            .is_none_or(TemplateEntity::is_plural)
    }

    fn display_name_for<'b>(&'b self, viewer_id: &str) -> Cow<'b, str> {
        let mut flat_members = Vec::with_capacity(self.members.len());
        flatten_group(&self.members, &mut flat_members, 0);

        let mut has_viewer = false;
        let mut visible_others = Vec::with_capacity(flat_members.len());

        for &m in &flat_members {
            if m.contains_viewer(viewer_id) {
                has_viewer = true;
            } else {
                let name = m.display_name_for(viewer_id);
                if !name.is_empty() {
                    visible_others.push(name);
                }
            }
        }

        let total_visible = visible_others.len() + usize::from(has_viewer);

        if total_visible == 0 {
            return Cow::Borrowed("");
        }

        if total_visible == 1 {
            if has_viewer {
                return Cow::Borrowed("you");
            }
            return visible_others.pop().unwrap_or_default();
        }

        let mut names: Vec<Cow<'b, str>> = Vec::with_capacity(total_visible);

        // If the viewer is in this group, they are always listed first as "you"
        if has_viewer {
            names.push(Cow::Borrowed("you"));
        }

        names.extend(visible_others);
        crate::grammar::format_oxford_list(names)
    }

    fn is_proper_noun_for(&self, viewer_id: &str) -> bool {
        self.single_leaf_member()
            .is_none_or(|m| m.is_proper_noun_for(viewer_id))
    }
}
