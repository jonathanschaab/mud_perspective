use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;

/// The highly unique sentinel string used by the engine to temporarily force the Director Stance.
pub const NULL_VIEWER: &str = "\0__MUD_PERSPECTIVE_NULL_VIEWER__\0";

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
/// ([`NULL_VIEWER`]) as the `viewer_id` to bypass recognition.
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

    /// Retrieves a nested sub-entity or property by name.
    ///
    /// This enables dot-notation in templates (e.g., `{source.left_arm}`) so that
    /// body parts, equipped items, or targets can be resolved dynamically and
    /// provide their own names, pronouns, and pluralities.
    #[must_use]
    fn get_property(&self, _property_name: &str) -> Option<&dyn TemplateEntity> {
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
    /// Tracks the key of the most recently rendered entity.
    /// Used by the engine for automatic anaphora (smart pronoun) resolution
    /// to prevent ambiguous pronouns when multiple characters are involved.
    pub last_mentioned: RefCell<Option<String>>,
    /// Tracks the active subject of the current clause (set by verb tags).
    /// Ensures possessive pronouns naturally bind to the subject of the sentence.
    pub active_subject: RefCell<Option<String>>,
    /// Tracks all entities mentioned since the last anaphora clear.
    /// Used to detect ambiguous pronoun collisions between any non-subject entities.
    pub recent_entities: RefCell<Vec<String>>,
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
            last_mentioned: RefCell::new(None),
            active_subject: RefCell::new(None),
            recent_entities: RefCell::new(Vec::new()),
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

    /// Manually sets the most recently mentioned entity for anaphora resolution.
    ///
    /// This allows builders to chain templates together while preserving pronoun
    /// continuity, or to force the engine to treat a specific entity as the current
    /// subject from the very beginning of the template.
    #[must_use]
    pub fn with_last_mentioned(self, key: &str) -> Self {
        *self.last_mentioned.borrow_mut() = Some(key.to_string());
        self.recent_entities.borrow_mut().push(key.to_string());
        self
    }

    /// Retrieves the key of the most recently mentioned entity, if any.
    ///
    /// This can be used to extract the anaphora state after rendering a template
    /// so that it can be passed into a future context.
    #[must_use]
    pub fn last_mentioned(&self) -> Option<String> {
        self.last_mentioned.borrow().clone()
    }

    /// Clears the anaphora resolution memory, treating all subsequent entities as newly introduced.
    pub fn clear_anaphora(&self) {
        *self.last_mentioned.borrow_mut() = None;
        *self.active_subject.borrow_mut() = None;
        self.recent_entities.borrow_mut().clear();
    }
}

/// A built-in helper for representing a dynamic group of entities.
///
/// `GroupEntity` automatically aggregates a collection of `TemplateEntity` references.
/// It delegates Oxford comma formatting, article distribution, and "you" injection to the
/// rendering engine, while evaluating as plural so verbs automatically conjugate correctly
/// (unless the group shrinks to a single member, in which case it evaluates as singular).
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

/// A type alias representing an evaluated group member and its resolved display name.
pub(crate) type EvaluatedMember<'a> = (&'a dyn TemplateEntity, Cow<'a, str>);

/// Flattens a group and partitions the members into the active viewer (if present)
/// and a list of other visible members alongside their evaluated display names.
pub(crate) fn partition_group<'a>(
    members: &[&'a dyn TemplateEntity],
    viewer_id: &str,
) -> (Option<&'a dyn TemplateEntity>, Vec<EvaluatedMember<'a>>) {
    let mut flat_members = Vec::with_capacity(members.len());
    flatten_group(members, &mut flat_members, 0);

    let mut viewer = None;
    let mut others = Vec::with_capacity(flat_members.len());

    for &m in &flat_members {
        if m.contains_viewer(viewer_id) {
            viewer = Some(m);
        } else {
            let name = m.display_name_for(viewer_id);
            if !name.is_empty() {
                others.push((m, name));
            }
        }
    }

    (viewer, others)
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
        let (viewer, others) = partition_group(&self.members, viewer_id);

        let mut names = Vec::with_capacity(others.len() + usize::from(viewer.is_some()));

        if let Some(v) = viewer {
            let name = v.display_name_for(viewer_id);
            if !name.is_empty() {
                names.push(name);
            }
        }

        names.extend(others.into_iter().map(|(_, name)| name));

        if names.is_empty() {
            return Cow::Borrowed("");
        }

        if names.len() == 1 {
            return names.pop().unwrap_or_default();
        }

        crate::grammar::format_oxford_list(names)
    }

    fn is_proper_noun_for(&self, viewer_id: &str) -> bool {
        self.single_leaf_member()
            .is_none_or(|m| m.is_proper_noun_for(viewer_id))
    }
}
