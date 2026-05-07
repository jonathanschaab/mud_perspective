use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;

/// The unique sentinel string used by the engine to temporarily force the Director Stance.
pub const NULL_VIEWER: &str = "\0__MUD_PERSPECTIVE_NULL_VIEWER__\0";

static PRONOUNS: phf::Set<&'static str> = phf::phf_set! {
    "he", "him", "his", "himself",
    "she", "her", "hers", "herself",
    "it", "its", "itself",
    "they", "them", "their", "theirs", "themselves",
    "you", "your", "yours", "yourself", "yourselves",
    "i", "me", "my", "mine", "myself",
    "we", "us", "our", "ours", "ourselves",
};

static VIEWER_PRONOUNS: phf::Set<&'static str> = phf::phf_set! {
    "you", "your", "yours", "yourself", "yourselves",
    "i", "me", "my", "mine", "myself",
    "we", "us", "our", "ours", "ourselves",
};

/// Represents the grammatical gender of an entity for correct pronoun resolution.
///
/// The `Plural` variant is critical for supporting both literal swarms (e.g., "a pack of wolves")
/// and singular entities that utilize non-binary they/them pronouns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
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

/// The grammatical stance used to refer to the viewing entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
pub enum ActorStance {
    /// Refers to the viewer in the first person ("I", "me", "my").
    FirstPerson,
    /// Refers to the viewer in the second person ("you", "your"). This is the default.
    #[default]
    SecondPerson,
    /// Refers to the viewer in the third person by their name (Director Stance).
    ThirdPerson,
}

/// The grammatical tense used to render the template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
pub enum Tense {
    /// Renders verbs in the present tense (e.g., "walks").
    #[default]
    Present,
    /// Renders verbs in the past tense (e.g., "walked").
    Past,
    /// Renders verbs in the future tense (e.g., "will walk").
    Future,
}

/// A generic trait implemented by game objects to allow them to be referenced
/// dynamically within text templates.
///
/// By requiring `viewer_id` in its methods, this trait ensures that text rendering
/// is strictly bound to the observer's epistemological state, supporting mechanics
/// like stealth, disguises, and recognition.
///
/// **Note on Forced Perspectives:** When a template forces the Director Stance
/// (e.g., `{+source}`), the engine temporarily passes a unique sentinel string
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

    /// Returns an optional long display name for the entity, explicitly tailored to the observer.
    ///
    /// If provided, the rendering engine will automatically use this description instead
    /// of `display_name_for` when it detects a name collision with another recently
    /// mentioned entity, preventing the need to fall back to "another".
    ///
    /// # Arguments
    /// * `viewer_id` - The unique identifier of the observing entity.
    fn long_display_name_for<'a>(&'a self, _viewer_id: &str) -> Option<Cow<'a, str>> {
        None
    }

    /// Returns an optional collective noun for this entity if it represents a group.
    /// This is used for plural ordinal phrasing (e.g., "a third pack of wolves").
    /// If `None`, the engine defaults to "set".
    fn collective_noun(&self) -> Option<&str> {
        None
    }

    /// Returns an optional list of adjectives that describe this entity.
    /// Used by `RenderContext::resolve_target` to allow players to target entities
    /// using descriptive adjectives (e.g., "large" in "large wolf") without
    /// treating the adjectives as independent noun aliases.
    fn adjectives(&self) -> Option<&[&str]> {
        None
    }

    /// Returns an optional list of adjective synonyms for this entity.
    /// Used by `RenderContext::resolve_target` to allow fuzzy matching (e.g., "big" for "large").
    /// These are NOT used for rendering to avoid phrases like "the big large wolf".
    fn adjective_synonyms(&self) -> Option<&[&str]> {
        None
    }

    /// Determines if the entity's current identity is a proper noun.
    ///
    /// If `true`, the rendering engine will automatically suppress indefinite (`a/an`)
    /// and definite (`the`) articles. It is also used by the `@` drop-possessive modifier
    /// to determine if a narrative owner should be dropped (e.g., `"Aldran wields Excalibur"`).
    /// This must be perspective-aware to account for situations where a stranger sees a
    /// common noun ("a tall man") while a friend sees a proper noun ("Aldran").
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
    /// This enables dot-notation in templates (e.g., `{source.weapon}` or `{source's @target.weapon}`) so that
    /// body parts, equipped items, or targets can be resolved dynamically and
    /// provide their own names, pronouns, and pluralities.
    #[must_use]
    fn get_property(&self, _property_name: &str) -> Option<&dyn TemplateEntity> {
        None
    }

    /// Returns an optional list of alternative names or aliases the user can use to target this entity.
    /// Used exclusively by `RenderContext::resolve_target`.
    fn aliases(&self) -> Option<&[&str]> {
        None
    }
}

/// Tracks the assignment of ordinals for entities with colliding names.
#[derive(Debug, Clone, Default)]
pub struct OrdinalState {
    /// The next ordinal to assign (e.g., 1, 2, 3...)
    pub next_ordinal: usize,
    /// Maps an entity's template key to its assigned ordinal (e.g., "w1" -> 1)
    pub members: HashMap<String, usize>,
}

/// The context environment passed to the rendering engine for a specific view generation.
#[derive(Clone)]
pub struct RenderContext<'a> {
    /// The unique identifier of the entity actively reading the text.
    pub viewer_id: &'a str,
    /// The narrative stance used to refer to the viewing entity.
    pub stance: ActorStance,
    /// The grammatical tense used to render the template.
    pub tense: Tense,
    /// Enables the AST Pre-Pass for omniscient short/long description and ordinal disambiguation.
    pub lookahead: bool,
    /// The number at which ordinal words ("third") switch to integer form ("3rd"). Defaults to 999.
    /// Set to 0 to always use integer form.
    pub ordinal_word_threshold: usize,
    /// If true, disables the transliteration of Unicode characters (like `Ä`) to ASCII (`A`) during target resolution.
    /// Defaults to false.
    pub strict_diacritics: bool,
    /// The maximum number of entities to track for anaphora resolution before evicting the oldest.
    /// Defaults to 15. Set to 0 for unbounded growth.
    /// If all entities in memory are pinned, this limit will be temporarily exceeded to preserve narrative continuity.
    pub anaphora_limit: usize,
    /// The maximum number of adjectives to evaluate when generating combinations for disambiguation.
    /// Capping this prevents exponential O(2^N) blowup if an entity has a large number of adjectives. Defaults to 5.
    /// The absolute mathematical maximum is 127 to prevent bitshift overflow.
    pub adjective_disambiguation_limit: usize,
    /// If true, automatically clears the anaphora memory at the end of each successful render call.
    /// Defaults to false.
    pub auto_clear: bool,
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
    pub recent_entities: RefCell<Vec<RecentEntity>>,
    /// Tracks the assignment of ordinals for entities with colliding names.
    pub ordinals: RefCell<HashMap<String, OrdinalState>>,
    /// A memoization cache for target resolution within the lifespan of this context.
    pub(crate) target_cache: RefCell<HashMap<String, Vec<TargetMatch<'a>>>>,
}

bitflags::bitflags! {
    /// Flags representing cached boolean properties of a recently mentioned entity.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RecentEntityFlags: u8 {
        /// The cached grammatical plurality.
        const IS_PLURAL = 1 << 0;
        /// Cached result of `contains_viewer` for the standard viewer ID.
        const IS_VIEWER_NORMAL = 1 << 1;
        /// Cached result of `contains_viewer` for the forced Director Stance (`NULL_VIEWER`).
        const IS_VIEWER_FORCED = 1 << 2;
        /// Whether this entity is protected from automatic LRU eviction.
        const IS_PINNED = 1 << 3;
    }
}

/// Cached properties of a recently mentioned entity to avoid redundant trait method calls.
#[derive(Debug, Clone)]
pub struct RecentEntity {
    /// The template key of the entity.
    pub key: String,
    /// The cached grammatical gender.
    pub gender: Gender,
    /// The cached boolean flags for this entity.
    pub flags: RecentEntityFlags,
    /// Dynamic inline adjectives injected during rendering.
    pub adjectives: Vec<String>,
    /// The most recent non-pronoun name or description used for this entity during rendering.
    pub resolved_name: Option<String>,
}

/// A snapshot of the engine's anaphora resolution memory.
///
/// Used to transfer perfect narrative continuity (including ambiguity detection)
/// across different rendering contexts or server ticks.
#[derive(Debug, Clone, Default)]
pub struct AnaphoraState {
    /// The key of the last mentioned entity.
    pub last_mentioned: Option<String>,
    /// The key of the active subject in the current clause.
    pub active_subject: Option<String>,
    /// A cache of recently introduced entities to prevent pronoun collisions.
    pub recent_entities: Vec<RecentEntity>,
    /// Tracks the assignment of stable ordinals for display names.
    pub ordinals: HashMap<String, OrdinalState>,
}

impl AnaphoraState {
    /// Retrieves the most recent non-pronoun name or description used for the specified entity.
    /// This allows builders to determine if an entity was described as "red wolf" or "large wolf"
    /// during disambiguation.
    #[must_use]
    pub fn latest_name(&self, key: &str) -> Option<&str> {
        self.recent_entities
            .iter()
            .find(|r| r.key == key)
            .and_then(|r| r.resolved_name.as_deref())
    }

    /// Retrieves the most recent ordinal assigned to the specified entity, if any.
    #[must_use]
    pub fn current_ordinal(&self, key: &str) -> Option<usize> {
        for state in self.ordinals.values() {
            if let Some(&ordinal) = state.members.get(key) {
                return Some(ordinal);
            }
        }
        None
    }

    /// Retrieves any inline adjectives that were dynamically injected for this entity via templates.
    #[must_use]
    pub fn inline_adjectives(&self, key: &str) -> Option<&[String]> {
        self.recent_entities
            .iter()
            .find(|r| r.key == key)
            .map(|r| r.adjectives.as_slice())
    }

    /// Checks if the specified entity is currently tracked in the anaphora memory.
    /// This is useful for determining if an entity has already been introduced to the scene.
    #[must_use]
    pub fn has_seen_entity(&self, key: &str) -> bool {
        self.recent_entities.iter().any(|r| r.key == key)
    }

    /// Checks if the specified entity is currently pinned in the anaphora memory.
    #[must_use]
    pub fn is_entity_pinned(&self, key: &str) -> bool {
        self.recent_entities
            .iter()
            .find(|r| r.key == key)
            .is_some_and(|r| r.flags.contains(RecentEntityFlags::IS_PINNED))
    }
}

/// Represents a matched entity and an optional sub-element path requested by the user.
#[derive(Clone)]
pub struct TargetMatch<'a> {
    /// The template key of the matched entity.
    pub key: String,
    /// A reference to the matched entity.
    pub entity: &'a dyn TemplateEntity,
    /// An optional path to a sub-element (e.g., "sword" in "Aldran's sword").
    pub path: Option<String>,
    /// True if a `path` is present but the entity's `get_property` could not confirm it exists.
    pub path_uncertain: bool,
}

impl std::fmt::Debug for TargetMatch<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TargetMatch")
            .field("key", &self.key)
            .field("path", &self.path)
            .field("path_uncertain", &self.path_uncertain)
            .finish_non_exhaustive()
    }
}

impl<'a> TargetMatch<'a> {
    /// Resolves and returns the deepest sub-element `TemplateEntity` requested by the path.
    /// If no path was requested, this returns the base matched entity.
    /// If the requested path is invalid, returns `None`.
    #[must_use]
    pub fn resolve_deep_entity(&self) -> Option<&'a dyn TemplateEntity> {
        if let Some(ref path) = self.path {
            resolve_entity_path(self.entity, path)
        } else {
            Some(self.entity)
        }
    }
}

fn resolve_entity_path<'a>(
    mut current: &'a dyn TemplateEntity,
    path: &str,
) -> Option<&'a dyn TemplateEntity> {
    for prop in path.split('.') {
        if let Some(next) = current.get_property(prop) {
            current = next;
        } else {
            return None;
        }
    }
    Some(current)
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
            stance: ActorStance::SecondPerson,
            tense: Tense::Present,
            lookahead: false,
            ordinal_word_threshold: 999,
            strict_diacritics: false,
            anaphora_limit: 15,
            adjective_disambiguation_limit: 5,
            auto_clear: false,
            entities: HashMap::new(),
            last_mentioned: RefCell::new(None),
            active_subject: RefCell::new(None),
            recent_entities: RefCell::new(Vec::new()),
            ordinals: RefCell::new(HashMap::new()),
            target_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Resolves a natural language description (e.g., "him", "the fourth wolf", "Aldran's sword")
    /// to the corresponding entities in the current context.
    ///
    /// Returns a list of `TargetMatch` objects. If the description is ambiguous (e.g., "him" when
    /// two males are present), the list will contain multiple matches, allowing the caller to ask
    /// the user for clarification.
    ///
    /// If the description references a sub-element (e.g., "his sword"), the `path` field will be
    /// populated with "sword". If the parent entity does not expose this property via `get_property`,
    /// `path_uncertain` will be `true`, allowing the caller to decide whether to accept it.
    #[must_use]
    pub fn resolve_target(&self, description: &str) -> Vec<TargetMatch<'a>> {
        if let Some(cached) = self.target_cache.borrow().get(description) {
            return cached.clone();
        }

        let (base_desc, sub_element_desc) = parse_target_description(description);

        let clean_full_desc_owned = description.to_lowercase().replace('’', "'");
        let clean_full_desc = clean_full_desc_owned
            .trim()
            .trim_end_matches(['.', '!', '?', ',', ';', ':']);

        // 1. Try to resolve the whole description as a flat entity FIRST.
        // If the user mapped "Aldran's sword" directly to an alias, it will catch it here.
        let raw_matches = self
            .resolve_pronoun_targets(clean_full_desc)
            .unwrap_or_else(|| self.resolve_name_targets(clean_full_desc));

        let mut deep_matches = Vec::new();
        let mut fallback_matches = Vec::new();

        if let Some(ref path) = sub_element_desc {
            let base_matches = self
                .resolve_pronoun_targets(&base_desc)
                .unwrap_or_else(|| self.resolve_name_targets(&base_desc));

            for (base_key, base_entity) in base_matches {
                let path_matches = self.resolve_name_targets(path);
                let mut found_deep = false;

                for (path_key, path_entity) in path_matches {
                    // Check structural match (e.g., base_key = "source", path_key = "source.weapon")
                    if path_key.starts_with(&format!("{base_key}.")) {
                        deep_matches.push((path_key, path_entity));
                        found_deep = true;
                    } else {
                        // Check narrative match via ordinals (e.g., base_key = "g1", path_key = "s2" inside "g1::sword")
                        let name = path_entity.display_name_for(self.viewer_id);
                        let ns_key = format!("{base_key}::{name}");
                        if let Some(state) = self.ordinals.borrow().get(&ns_key)
                            && state.members.contains_key(&path_key)
                        {
                            deep_matches.push((path_key, path_entity));
                            found_deep = true;
                        }
                    }
                }

                if !found_deep {
                    fallback_matches.push((base_key, base_entity, path.clone()));
                }
            }
        }

        let mut unique_matches = Vec::new();

        for (k, e) in raw_matches {
            if !unique_matches
                .iter()
                .any(|m: &TargetMatch| is_same_entity(m.entity, e))
            {
                unique_matches.push(TargetMatch {
                    key: k,
                    entity: e,
                    path: None,
                    path_uncertain: false,
                });
            }
        }
        for (k, e) in deep_matches {
            if !unique_matches
                .iter()
                .any(|m: &TargetMatch| is_same_entity(m.entity, e))
            {
                unique_matches.push(TargetMatch {
                    key: k,
                    entity: e,
                    path: None,
                    path_uncertain: false,
                });
            }
        }
        for (k, e, p) in fallback_matches {
            if !unique_matches
                .iter()
                .any(|m: &TargetMatch| is_same_entity(m.entity, e))
            {
                let path_uncertain = resolve_entity_path(e, &p).is_none();
                unique_matches.push(TargetMatch {
                    key: k,
                    entity: e,
                    path: Some(p),
                    path_uncertain,
                });
            }
        }

        self.target_cache
            .borrow_mut()
            .insert(description.to_string(), unique_matches.clone());
        unique_matches
    }

    /// Retrieves the most recent non-pronoun name or description used for the specified entity.
    /// This allows builders to determine if an entity was described as "red wolf" or "large wolf"
    /// during disambiguation.
    #[must_use]
    pub fn latest_name(&self, key: &str) -> Option<String> {
        self.recent_entities
            .borrow()
            .iter()
            .find(|r| r.key == key)
            .and_then(|r| r.resolved_name.clone())
    }

    /// Retrieves the most recent ordinal assigned to the specified entity, if any.
    #[must_use]
    pub fn current_ordinal(&self, key: &str) -> Option<usize> {
        for state in self.ordinals.borrow().values() {
            if let Some(&ordinal) = state.members.get(key) {
                return Some(ordinal);
            }
        }
        None
    }

    /// Retrieves any inline adjectives that were dynamically injected for this entity via templates.
    #[must_use]
    pub fn inline_adjectives(&self, key: &str) -> Option<Vec<String>> {
        self.recent_entities
            .borrow()
            .iter()
            .find(|r| r.key == key)
            .map(|r| r.adjectives.clone())
    }

    /// Checks if the specified entity is currently tracked in the anaphora memory.
    /// This is useful for determining if an entity has already been introduced to the scene.
    #[must_use]
    pub fn has_seen_entity(&self, key: &str) -> bool {
        self.recent_entities.borrow().iter().any(|r| r.key == key)
    }

    /// Checks if the specified entity is currently pinned in the anaphora memory.
    #[must_use]
    pub fn is_entity_pinned(&self, key: &str) -> bool {
        self.recent_entities
            .borrow()
            .iter()
            .find(|r| r.key == key)
            .is_some_and(|r| r.flags.contains(RecentEntityFlags::IS_PINNED))
    }

    /// Resolves a natural language description to the corresponding entities in the current context,
    /// strictly filtering out any matches where the requested sub-element path cannot be confirmed.
    ///
    /// This is equivalent to calling `resolve_target` and retaining only the matches where `path_uncertain` is `false`.
    #[must_use]
    pub fn resolve_target_strict(&self, description: &str) -> Vec<TargetMatch<'a>> {
        self.resolve_target(description)
            .into_iter()
            .filter(|m| !m.path_uncertain)
            .collect()
    }

    /// Retrieves an entity by its template key, supporting dot-notation traversal.
    #[must_use]
    pub fn get_entity(&self, key: &str) -> Option<&'a dyn TemplateEntity> {
        if let Some(&entity) = self.entities.get(key) {
            return Some(entity);
        }
        if let Some((root_key, remainder)) = key.split_once(crate::parser::TAG_PROPERTY_SEP)
            && let Some(&current) = self.entities.get(root_key)
        {
            return resolve_entity_path(current, remainder);
        }
        None
    }

    fn resolve_pronoun_targets(
        &self,
        base_desc: &str,
    ) -> Option<Vec<(String, &'a dyn TemplateEntity)>> {
        if !PRONOUNS.contains(base_desc) {
            return None;
        }

        let mut matches = Vec::new();
        for recent in self.recent_entities.borrow().iter() {
            if let Some(entity) = self.get_entity(recent.key.as_str()) {
                let is_plural = recent.flags.contains(RecentEntityFlags::IS_PLURAL);
                let gender = recent.gender;
                let is_viewer = recent.flags.contains(RecentEntityFlags::IS_VIEWER_NORMAL)
                    || recent.flags.contains(RecentEntityFlags::IS_VIEWER_FORCED);

                let matches_pro = if is_viewer {
                    VIEWER_PRONOUNS.contains(base_desc)
                } else {
                    match base_desc {
                        "he" | "him" | "his" | "himself" => gender == Gender::Male && !is_plural,
                        "she" | "her" | "hers" | "herself" => {
                            gender == Gender::Female && !is_plural
                        }
                        "it" | "its" | "itself" => gender == Gender::Neutral && !is_plural,
                        "they" | "them" | "their" | "theirs" | "themselves" => {
                            gender == Gender::Plural || is_plural
                        }
                        _ => false,
                    }
                };

                if matches_pro {
                    matches.push((recent.key.clone(), entity));
                }
            }
        }

        Some(matches)
    }

    fn resolve_name_targets(&self, base_desc: &str) -> Vec<(String, &'a dyn TemplateEntity)> {
        let clean_desc = strip_articles(base_desc);
        let mut name_matches = Vec::new();
        for recent in self.recent_entities.borrow().iter() {
            if let Some(entity) = self.get_entity(recent.key.as_str())
                && self.entity_matches_name(&recent.key, entity, clean_desc)
            {
                name_matches.push((recent.key.clone(), entity));
            }
        }

        if name_matches.is_empty() {
            for (&key, &entity) in &self.entities {
                if self.entity_matches_name(key, entity, clean_desc) {
                    name_matches.push((key.to_string(), entity));
                }
            }
        }
        name_matches
    }

    fn entity_matches_name(
        &self,
        key: &str,
        entity: &dyn TemplateEntity,
        clean_desc: &str,
    ) -> bool {
        let recents = self.recent_entities.borrow();
        let recent_entity = recents.iter().find(|r| r.key == key);
        let inline_adjectives = recent_entity.map_or(&[][..], |r| r.adjectives.as_slice());

        let short_name = entity.display_name_for(self.viewer_id);
        let short_name_3rd = entity.display_name_for(NULL_VIEWER);
        let long_name = entity.long_display_name_for(self.viewer_id);

        let mut names_to_check = vec![short_name.as_ref(), short_name_3rd.as_ref()];

        // FAST PATH: Check the exact rendered name (which includes disambiguated adjectives)
        // to bypass the adjective validation loop entirely!
        if let Some(r) = recent_entity
            && let Some(ref rn) = r.resolved_name
            && rn != short_name.as_ref()
            && rn != short_name_3rd.as_ref()
        {
            names_to_check.push(rn.as_ref());
        }

        if let Some(ref ln) = long_name {
            names_to_check.push(ln.as_ref());
        }
        if let Some(aliases) = entity.aliases() {
            names_to_check.extend(aliases.iter().copied());
        }

        for name in &names_to_check {
            if check_phrase_match(
                clean_desc,
                name,
                entity,
                inline_adjectives,
                self.strict_diacritics,
            ) {
                return true;
            }
        }

        for name in &names_to_check {
            if self.check_ordinals(name, key, clean_desc, entity, inline_adjectives) {
                return true;
            }
        }

        false
    }

    fn check_ordinals(
        &self,
        name_to_check: &str,
        key: &str,
        clean_desc: &str,
        entity: &dyn TemplateEntity,
        inline_adjectives: &[String],
    ) -> bool {
        let mut ords = Vec::new();
        let suffix_ns = format!("::{name_to_check}");
        let suffix_adj = format!(" {name_to_check}");

        for (k, state) in self.ordinals.borrow().iter() {
            if (k == name_to_check || k.ends_with(&suffix_ns) || k.ends_with(&suffix_adj))
                && let Some(&o) = state.members.get(key)
                && !ords.contains(&o)
            {
                ords.push(o);
            }
        }

        if ords.is_empty() {
            ords.push(1);
        }

        for ord in ords {
            let ord_word = crate::grammar::number_to_ordinal_word(ord, self.ordinal_word_threshold)
                .to_lowercase();
            let ord_str = ord.to_string();

            let is_match = |remainder: &str| -> bool {
                check_phrase_match(
                    remainder,
                    name_to_check,
                    entity,
                    inline_adjectives,
                    self.strict_diacritics,
                )
            };

            let check_prefix = |variant_len: usize| -> bool {
                if clean_desc.len() > variant_len
                    && clean_desc.as_bytes().get(variant_len) == Some(&b' ')
                {
                    let remainder = clean_desc[variant_len + 1..].trim_start();
                    is_match(remainder)
                } else {
                    false
                }
            };

            // 1. Prefix: Ordinal Word
            if clean_desc.starts_with(&ord_word) && check_prefix(ord_word.len()) {
                return true;
            }

            // 2. Prefix: Numeric Ordinal
            if clean_desc.starts_with(&ord_str) {
                let after_num = &clean_desc[ord_str.len()..];
                let has_suffix = after_num.starts_with("th")
                    || after_num.starts_with("st")
                    || after_num.starts_with("nd")
                    || after_num.starts_with("rd");

                if has_suffix && check_prefix(ord_str.len() + 2) {
                    return true;
                }
                if check_prefix(ord_str.len()) {
                    return true;
                }
            }

            let check_postfix = |variant_len: usize| -> bool {
                if clean_desc.len() > variant_len
                    && clean_desc
                        .as_bytes()
                        .get(clean_desc.len() - variant_len - 1)
                        == Some(&b' ')
                {
                    let remainder = clean_desc[..clean_desc.len() - variant_len - 1].trim_end();
                    is_match(remainder)
                } else {
                    false
                }
            };

            // 3. Postfix: Ordinal Word
            if clean_desc.ends_with(&ord_word) && check_postfix(ord_word.len()) {
                return true;
            }

            // 4. Postfix: Numeric Ordinal
            let has_suffix = clean_desc.ends_with("th")
                || clean_desc.ends_with("st")
                || clean_desc.ends_with("nd")
                || clean_desc.ends_with("rd");

            if has_suffix && clean_desc.len() >= 2 + ord_str.len() {
                let num_end_idx = clean_desc.len() - 2;
                if clean_desc[..num_end_idx].ends_with(&ord_str) && check_postfix(ord_str.len() + 2)
                {
                    return true;
                }
            }

            if clean_desc.ends_with(&ord_str) && check_postfix(ord_str.len()) {
                return true;
            }
        }

        false
    }

    // --- Private Setters for Cache Invalidation ---

    fn set_viewer_id(&mut self, viewer_id: &'a str) {
        self.viewer_id = viewer_id;
        self.clear_target_cache();
    }

    fn set_stance(&mut self, stance: ActorStance) {
        self.stance = stance;
        self.clear_target_cache();
    }

    fn set_ordinal_word_threshold(&mut self, threshold: usize) {
        self.ordinal_word_threshold = threshold;
        self.clear_target_cache();
    }

    fn set_strict_diacritics(&mut self, strict: bool) {
        self.strict_diacritics = strict;
        self.clear_target_cache();
    }

    fn add_entity(&mut self, key: &'a str, entity: &'a dyn TemplateEntity) {
        self.entities.insert(key, entity);
        self.clear_target_cache();
    }

    /// Configures the viewer ID for the rendering context.
    /// This is particularly useful when cloning a base context to render the same event for multiple observers.
    #[must_use]
    pub fn with_viewer(mut self, viewer_id: &'a str) -> Self {
        self.set_viewer_id(viewer_id);
        self
    }

    /// Configures the actor stance for the rendering context.
    #[must_use]
    pub fn with_stance(mut self, stance: ActorStance) -> Self {
        self.set_stance(stance);
        self
    }

    /// Configures the grammatical tense for the rendering context.
    #[must_use]
    pub fn with_tense(mut self, tense: Tense) -> Self {
        self.tense = tense;
        self
    }

    /// Enables or disables the AST Pre-Pass to resolve name collisions ahead of time.
    /// This is disabled by default to maximize performance, but can be enabled for critical narrative templates.
    #[must_use]
    pub fn with_lookahead(mut self, lookahead: bool) -> Self {
        self.lookahead = lookahead;
        self
    }

    /// Configures the number at which ordinal words switch to integer form.
    /// Defaults to 999. Set to 0 to always use integer form.
    #[must_use]
    pub fn with_ordinal_word_threshold(mut self, threshold: usize) -> Self {
        self.set_ordinal_word_threshold(threshold);
        self
    }

    /// Enables or disables strict diacritic matching for target resolution.
    /// If `true`, users must input exact accents/diacritics to match entities.
    /// If `false` (default), the engine transliterates Unicode to ASCII for fuzzy matching (e.g., "Ängry" matches "angry").
    #[must_use]
    pub fn with_strict_diacritics(mut self, strict: bool) -> Self {
        self.set_strict_diacritics(strict);
        self
    }

    /// Configures the maximum number of recent entities to track for pronoun ambiguity resolution.
    /// The memory operates as a Least-Recently-Used (LRU) cache.
    #[must_use]
    pub fn with_anaphora_limit(mut self, limit: usize) -> Self {
        self.anaphora_limit = limit;
        self
    }

    /// Configures the maximum number of adjectives to evaluate for disambiguation combinations.
    ///
    /// The absolute maximum allowed value is 127 to prevent integer overflow during the
    /// combinatorial search. If a larger value is provided, it will be clamped to 127
    /// and a warning will be logged.
    #[must_use]
    pub fn with_adjective_disambiguation_limit(mut self, limit: usize) -> Self {
        let max_limit = (u128::BITS - 1) as usize;
        if limit > max_limit {
            tracing::warn!(
                "Adjective disambiguation limit {} exceeds the maximum supported value of {}. Clamping to {}.",
                limit,
                max_limit,
                max_limit
            );
            self.adjective_disambiguation_limit = max_limit;
        } else {
            self.adjective_disambiguation_limit = limit;
        }
        self
    }

    /// Enables or disables automatic clearing of anaphora memory after rendering.
    /// If `true`, the context will automatically call `clear_anaphora()` at the end of every `PerspectiveEngine::render` call,
    /// ensuring each render starts with a fresh narrative memory.
    #[must_use]
    pub fn with_auto_clear(mut self, auto_clear: bool) -> Self {
        self.auto_clear = auto_clear;
        self
    }

    /// Adds an entity mapping to the context using a fluent builder pattern.
    ///
    /// # Arguments
    /// * `key` - The string key used inside the template tags (e.g., "target").
    /// * `entity` - A reference to the game object implementing `TemplateEntity`.
    #[must_use]
    pub fn with_entity(mut self, key: &'a str, entity: &'a dyn TemplateEntity) -> Self {
        self.add_entity(key, entity);
        self
    }

    /// Pins an entity in the anaphora memory so it will never be automatically evicted.
    ///
    /// If the number of pinned entities exceeds the anaphora limit, the limit will be
    /// temporarily bypassed to prevent eviction.
    #[must_use]
    pub fn with_pinned_entity(self, key: &str) -> Self {
        self.pin_anaphora(key);
        self
    }

    /// Explicitly removes an entity from the anaphora memory.
    #[must_use]
    pub fn without_anaphora(self, key: &str) -> Self {
        self.forget_anaphora(key);
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
        if let Some(entity) = self.entities.get(key) {
            let mut recents = self.recent_entities.borrow_mut();

            // Refresh position if already present (LRU)
            if let Some(pos) = recents.iter().position(|r| r.key == key) {
                let mut item = recents.remove(pos);
                item.gender = entity.gender();
                item.flags
                    .set(RecentEntityFlags::IS_PLURAL, entity.is_plural());
                item.flags.set(
                    RecentEntityFlags::IS_VIEWER_NORMAL,
                    entity.contains_viewer(self.viewer_id),
                );
                item.flags.set(
                    RecentEntityFlags::IS_VIEWER_FORCED,
                    entity.contains_viewer(NULL_VIEWER),
                );
                recents.push(item);
            } else {
                let mut flags = RecentEntityFlags::empty();
                flags.set(RecentEntityFlags::IS_PLURAL, entity.is_plural());
                flags.set(
                    RecentEntityFlags::IS_VIEWER_NORMAL,
                    entity.contains_viewer(self.viewer_id),
                );
                flags.set(
                    RecentEntityFlags::IS_VIEWER_FORCED,
                    entity.contains_viewer(NULL_VIEWER),
                );

                recents.push(RecentEntity {
                    key: key.to_string(),
                    gender: entity.gender(),
                    flags,
                    adjectives: Vec::new(),
                    resolved_name: None,
                });
            }

            // Enforce capacity
            let mut last_mentioned = self.last_mentioned.borrow_mut();
            let mut active_subject = self.active_subject.borrow_mut();
            enforce_anaphora_limit(
                self.anaphora_limit,
                &mut recents,
                &mut last_mentioned,
                &mut active_subject,
            );
            self.clear_target_cache();
        }
        self
    }

    /// Extracts a full snapshot of the current anaphora memory state.
    #[must_use]
    pub fn extract_anaphora(&self) -> AnaphoraState {
        AnaphoraState {
            last_mentioned: self.last_mentioned.borrow().clone(),
            active_subject: self.active_subject.borrow().clone(),
            recent_entities: self.recent_entities.borrow().clone(),
            ordinals: self.ordinals.borrow().clone(),
        }
    }

    /// Injects a previously extracted anaphora state to resume narrative continuity.
    /// This replaces any current anaphora state in the context.
    #[must_use]
    pub fn with_anaphora(self, state: AnaphoraState) -> Self {
        *self.last_mentioned.borrow_mut() = state.last_mentioned;
        *self.active_subject.borrow_mut() = state.active_subject;
        *self.recent_entities.borrow_mut() = state.recent_entities;
        *self.ordinals.borrow_mut() = state.ordinals;
        self.clear_target_cache();
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

    /// Retrieves the key of the active grammatical subject in the current clause.
    ///
    /// This is automatically updated by the engine whenever a verb tag is processed
    /// and represents the entity currently driving the narrative action.
    #[must_use]
    pub fn active_subject(&self) -> Option<String> {
        self.active_subject.borrow().clone()
    }

    /// Clears the anaphora resolution memory, treating all subsequent entities as newly introduced.
    pub fn clear_anaphora(&self) {
        *self.last_mentioned.borrow_mut() = None;
        *self.active_subject.borrow_mut() = None;
        self.recent_entities.borrow_mut().clear();
        self.ordinals.borrow_mut().clear();
        self.clear_target_cache();
    }

    /// Pins an entity in the anaphora memory so it will never be automatically evicted.
    ///
    /// If the number of pinned entities exceeds the anaphora limit, the limit will be
    /// temporarily bypassed to prevent eviction.
    pub fn pin_anaphora(&self, key: &str) {
        if let Some(entity) = self.entities.get(key) {
            let mut recents = self.recent_entities.borrow_mut();
            if let Some(pos) = recents.iter().position(|r| r.key == key) {
                let mut item = recents.remove(pos);
                item.flags |= RecentEntityFlags::IS_PINNED;
                item.gender = entity.gender();
                item.flags
                    .set(RecentEntityFlags::IS_PLURAL, entity.is_plural());
                item.flags.set(
                    RecentEntityFlags::IS_VIEWER_NORMAL,
                    entity.contains_viewer(self.viewer_id),
                );
                item.flags.set(
                    RecentEntityFlags::IS_VIEWER_FORCED,
                    entity.contains_viewer(NULL_VIEWER),
                );
                recents.push(item);
            } else {
                let mut flags = RecentEntityFlags::IS_PINNED;
                flags.set(RecentEntityFlags::IS_PLURAL, entity.is_plural());
                flags.set(
                    RecentEntityFlags::IS_VIEWER_NORMAL,
                    entity.contains_viewer(self.viewer_id),
                );
                flags.set(
                    RecentEntityFlags::IS_VIEWER_FORCED,
                    entity.contains_viewer(NULL_VIEWER),
                );

                recents.push(RecentEntity {
                    key: key.to_string(),
                    gender: entity.gender(),
                    flags,
                    adjectives: Vec::new(),
                    resolved_name: None,
                });
            }
            let mut last_mentioned = self.last_mentioned.borrow_mut();
            let mut active_subject = self.active_subject.borrow_mut();
            enforce_anaphora_limit(
                self.anaphora_limit,
                &mut recents,
                &mut last_mentioned,
                &mut active_subject,
            );
            self.clear_target_cache();
        }
    }

    /// Unpins an entity in the anaphora memory, allowing it to be evicted naturally.
    pub fn unpin_anaphora(&self, key: &str) {
        if let Some(item) = self
            .recent_entities
            .borrow_mut()
            .iter_mut()
            .find(|r| r.key == key)
        {
            item.flags.remove(RecentEntityFlags::IS_PINNED);
            self.clear_target_cache();
        }
    }

    /// Explicitly removes a specific entity from the anaphora memory.
    pub fn forget_anaphora(&self, key: &str) {
        if self.last_mentioned.borrow().as_deref() == Some(key) {
            *self.last_mentioned.borrow_mut() = None;
        }
        if self.active_subject.borrow().as_deref() == Some(key) {
            *self.active_subject.borrow_mut() = None;
        }
        self.recent_entities.borrow_mut().retain(|r| r.key != key);
        self.clear_target_cache();
    }

    /// Explicitly clears the memoization cache used by `resolve_target`.
    ///
    /// You normally do not need to call this, as the engine automatically invalidates the cache
    /// whenever the context's state (such as anaphora memory, ordinals, or stances) changes.
    /// However, if you mutate an entity's internal data (like its name or adjectives) using
    /// interior mutability while the `RenderContext` is still active, you should call this
    /// method to ensure subsequent target resolutions reflect the updated data.
    pub fn clear_target_cache(&self) {
        self.target_cache.borrow_mut().clear();
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

/// Compares two `TemplateEntity` trait objects to determine if they point to the same underlying data.
///
/// Comparing trait objects directly using `std::ptr::eq` or `std::ptr::addr_eq` is unreliable
/// because it compares both the data pointer and the vtable pointer. This casts them to `*const ()`
/// first to strictly compare the data pointer, avoiding false negatives from duplicated vtables.
#[inline]
#[must_use]
pub fn is_same_entity(a: &dyn TemplateEntity, b: &dyn TemplateEntity) -> bool {
    std::ptr::eq(
        std::ptr::from_ref(a).cast::<()>(),
        std::ptr::from_ref(b).cast::<()>(),
    )
}

fn strip_articles(mut s: &str) -> &str {
    let prefixes = [
        "the ",
        "a ",
        "an ",
        "some ",
        "this ",
        "that ",
        "another ",
        "other ",
        "these ",
        "those ",
        "one of the ",
        "one of ",
    ];
    let mut modified = true;
    while modified {
        modified = false;
        for prefix in &prefixes {
            if s.len() >= prefix.len()
                && s.as_bytes()
                    .get(..prefix.len())
                    .is_some_and(|b| b.eq_ignore_ascii_case(prefix.as_bytes()))
            {
                s = &s[prefix.len()..];
                modified = true;
                break;
            }
        }
    }
    s
}

#[inline]
fn eq_ignore_case(lower_str: &str, mixed_str: &str, strict: bool) -> bool {
    if lower_str.is_ascii() && mixed_str.is_ascii() {
        lower_str.eq_ignore_ascii_case(mixed_str)
    } else if strict {
        lower_str
            .chars()
            .eq(mixed_str.chars().flat_map(char::to_lowercase))
    } else {
        deunicode::deunicode(lower_str).to_lowercase()
            == deunicode::deunicode(mixed_str).to_lowercase()
    }
}

fn check_phrase_match(
    input: &str,
    target_name: &str,
    entity: &dyn TemplateEntity,
    inline_adjectives: &[String],
    strict_diacritics: bool,
) -> bool {
    let rem_words: Vec<&str> = strip_articles(input).split_whitespace().collect();
    let name_words: Vec<&str> = strip_articles(target_name).split_whitespace().collect();

    if name_words.is_empty() || rem_words.len() < name_words.len() {
        return false;
    }

    let validate_adjectives = |adj_words: &[&str]| -> bool {
        adj_words.is_empty() || {
            let valid_adjs = entity.adjectives().unwrap_or(&[]);
            let synonym_adjs = entity.adjective_synonyms().unwrap_or(&[]);
            adj_words.iter().all(|i_adj| {
                inline_adjectives
                    .iter()
                    .any(|v_adj| eq_ignore_case(i_adj, v_adj, strict_diacritics))
                    || valid_adjs
                        .iter()
                        .any(|v_adj| eq_ignore_case(i_adj, v_adj, strict_diacritics))
                    || synonym_adjs
                        .iter()
                        .any(|v_adj| eq_ignore_case(i_adj, v_adj, strict_diacritics))
            })
        }
    };

    if let Some(col) = entity.collective_noun()
        && rem_words.len() >= name_words.len() + 2
    {
        let (prefix, suffix) = rem_words.split_at(rem_words.len() - name_words.len());
        if let [adj_words @ .., col_word, of_word] = prefix
            && eq_ignore_case(col_word, col, strict_diacritics)
            && eq_ignore_case(of_word, "of", strict_diacritics)
            && suffix
                .iter()
                .zip(name_words.iter())
                .all(|(a, b)| eq_ignore_case(a, b, strict_diacritics))
        {
            return validate_adjectives(adj_words);
        }
    }

    let (adj_words, trailing_words) = rem_words.split_at(rem_words.len() - name_words.len());
    let matches_name = trailing_words
        .iter()
        .zip(name_words.iter())
        .all(|(a, b)| eq_ignore_case(a, b, strict_diacritics));

    if matches_name {
        return validate_adjectives(adj_words);
    }

    false
}

fn parse_target_description(desc: &str) -> (String, Option<String>) {
    let lower = desc.to_lowercase().replace('’', "'");
    let mut text = lower
        .trim()
        .trim_end_matches(['.', '!', '?', ',', ';', ':']);

    let mut base = String::new();
    let mut path = String::new();

    let possessive_pronouns = ["his ", "her ", "its ", "their ", "your ", "my ", "our "];
    for prefix in &possessive_pronouns {
        if text.starts_with(prefix) {
            base.push_str(prefix.trim());
            text = text[prefix.len()..].trim();
            break;
        }
    }

    while !text.is_empty() {
        let (owner, advance, add_s) = if let Some(idx) = text
            .find("'s ")
            .filter(|&i| i == 0 || text.as_bytes().get(i - 1) != Some(&b' '))
        {
            (&text[..idx], idx + 3, false)
        } else if let Some(idx) = text
            .find("s' ")
            .filter(|&i| i == 0 || text.as_bytes().get(i - 1) != Some(&b' '))
        {
            (&text[..idx], idx + 3, true)
        } else if let Some(idx) = text
            .find("' ")
            .filter(|&i| i == 0 || text.as_bytes().get(i - 1) != Some(&b' '))
        {
            (&text[..idx], idx + 2, false)
        } else {
            (text, text.len(), false)
        };

        if base.is_empty() {
            base.push_str(owner);
            if add_s {
                base.push('s');
            }
        } else {
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(owner);
            if add_s {
                path.push('s');
            }
        }

        text = text.get(advance..).unwrap_or_default().trim();
    }

    let path_opt = if path.is_empty() { None } else { Some(path) };

    (base, path_opt)
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

        crate::grammar::format_oxford_list(names, "and")
    }

    fn is_proper_noun_for(&self, viewer_id: &str) -> bool {
        self.single_leaf_member()
            .is_none_or(|m| m.is_proper_noun_for(viewer_id))
    }

    fn aliases(&self) -> Option<&[&str]> {
        self.single_leaf_member().and_then(TemplateEntity::aliases)
    }
}

#[inline]
pub(crate) fn enforce_anaphora_limit(
    limit: usize,
    recents: &mut Vec<RecentEntity>,
    last_mentioned: &mut Option<String>,
    active_subject: &mut Option<String>,
) {
    if limit > 0 {
        while recents.len() > limit {
            // Remove the oldest unpinned entity.
            if let Some(pos) = recents
                .iter()
                .position(|r| !r.flags.contains(RecentEntityFlags::IS_PINNED))
            {
                let removed = recents.remove(pos);
                if last_mentioned.as_deref() == Some(removed.key.as_str()) {
                    *last_mentioned = None;
                }
                if active_subject.as_deref() == Some(removed.key.as_str()) {
                    *active_subject = None;
                }
            } else {
                // Everything is pinned, we must allow memory to exceed the normal limit.
                break;
            }
        }
    }
}
