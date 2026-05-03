use crate::models::{ActorStance, Gender};
use arc_swap::ArcSwap;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

include!(concat!(env!("OUT_DIR"), "/irregular_verbs.rs"));

/// A global runtime dictionary for custom irregular verbs injected by builders.
static CUSTOM_VERBS: OnceLock<ArcSwap<HashMap<String, String>>> = OnceLock::new();

fn get_custom_verbs() -> &'static ArcSwap<HashMap<String, String>> {
    CUSTOM_VERBS.get_or_init(|| ArcSwap::from_pointee(HashMap::new()))
}

/// Adds a custom irregular verb override to the runtime dictionary.
///
/// # Errors
/// Returns an error if the verb already exists in either the static dictionary or the runtime dictionary.
pub fn add_irregular_verb(base: &str, conjugated: &str) -> Result<(), String> {
    let lower_base = base.to_lowercase();
    if IRREGULAR_VERBS.contains_key(lower_base.as_str()) {
        return Err(format!(
            "Verb '{lower_base}' already exists in the static dictionary."
        ));
    }

    let lower_conjugated = conjugated.to_lowercase();
    let custom_verbs = get_custom_verbs();
    loop {
        let current_map = custom_verbs.load();
        if current_map.contains_key(&lower_base) {
            return Err(format!(
                "Verb '{lower_base}' already exists in the runtime dictionary."
            ));
        }

        let mut new_map = (**current_map).clone();
        new_map.insert(lower_base.clone(), lower_conjugated.clone());

        let prev = custom_verbs.compare_and_swap(&current_map, Arc::new(new_map));
        if Arc::ptr_eq(&prev, &current_map) {
            break Ok(());
        }
    }
}

/// Forces the addition of a custom irregular verb override, overwriting any existing
/// entries in the runtime dictionary.
///
/// **Note:** While this cannot physically remove entries from the compile-time `phf::Map`,
/// the runtime dictionary takes precedence during conjugation, effectively overriding static entries.
pub fn force_add_irregular_verb(base: &str, conjugated: &str) {
    let lower_base = base.to_lowercase();
    let lower_conjugated = conjugated.to_lowercase();
    get_custom_verbs().rcu(|current_map| {
        let mut new_map = (**current_map).clone();
        new_map.insert(lower_base.clone(), lower_conjugated.clone());
        Arc::new(new_map)
    });
}

/// Removes a custom irregular verb override from the runtime dictionary.
///
/// Returns `true` if the verb was successfully removed, or `false` if the verb
/// was not found in the runtime dictionary.
#[must_use]
pub fn remove_irregular_verb(base: &str) -> bool {
    let lower_base = base.to_lowercase();

    let custom_verbs = get_custom_verbs();
    loop {
        let current_map = custom_verbs.load();
        if !current_map.contains_key(&lower_base) {
            break false;
        }

        let mut new_map = (**current_map).clone();
        new_map.remove(&lower_base);

        let prev = custom_verbs.compare_and_swap(&current_map, Arc::new(new_map));
        if Arc::ptr_eq(&prev, &current_map) {
            break true;
        }
    }
}

/// Clears all custom irregular verb overrides from the runtime dictionary.
pub fn clear_irregular_verbs() {
    get_custom_verbs().store(Arc::new(HashMap::new()));
}

struct PronounSet {
    subj: &'static str,
    obj: &'static str,
    poss: &'static str,
    abs_poss: &'static str,
    reflex: &'static str,
}

static MALE_PRONOUNS: PronounSet = PronounSet {
    subj: "he",
    obj: "him",
    poss: "his",
    abs_poss: "his",
    reflex: "himself",
};

static FEMALE_PRONOUNS: PronounSet = PronounSet {
    subj: "she",
    obj: "her",
    poss: "her",
    abs_poss: "hers",
    reflex: "herself",
};

static NEUTRAL_PRONOUNS: PronounSet = PronounSet {
    subj: "it",
    obj: "it",
    poss: "its",
    abs_poss: "its",
    reflex: "itself",
};

static PLURAL_PRONOUNS: PronounSet = PronounSet {
    subj: "they",
    obj: "them",
    poss: "their",
    abs_poss: "theirs",
    reflex: "themselves",
};

static VIEWER_SINGULAR_PRONOUNS: PronounSet = PronounSet {
    subj: "you",
    obj: "you",
    poss: "your",
    abs_poss: "yours",
    reflex: "yourself",
};

static VIEWER_PLURAL_PRONOUNS: PronounSet = PronounSet {
    subj: "you",
    obj: "you",
    poss: "your",
    abs_poss: "yours",
    reflex: "yourselves",
};

static FIRST_PERSON_SINGULAR_PRONOUNS: PronounSet = PronounSet {
    subj: "I",
    obj: "me",
    poss: "my",
    abs_poss: "mine",
    reflex: "myself",
};

static FIRST_PERSON_PLURAL_PRONOUNS: PronounSet = PronounSet {
    subj: "we",
    obj: "us",
    poss: "our",
    abs_poss: "ours",
    reflex: "ourselves",
};

/// Returns the correct pronoun based on gender, type, and perspective.
///
/// # Errors
/// Returns a `String` error if the provided `p_type` is an unknown or unsupported pronoun case.
pub fn resolve_pronoun(
    gender: Gender,
    p_type: &str,
    is_viewer: bool,
    is_plural: bool,
    stance: ActorStance,
) -> Result<&'static str, String> {
    let (pronoun_set, context) = if is_viewer {
        let set = match stance {
            ActorStance::FirstPerson => {
                if is_plural {
                    &FIRST_PERSON_PLURAL_PRONOUNS
                } else {
                    &FIRST_PERSON_SINGULAR_PRONOUNS
                }
            }
            _ => {
                if is_plural {
                    &VIEWER_PLURAL_PRONOUNS
                } else {
                    &VIEWER_SINGULAR_PRONOUNS
                }
            }
        };
        (set, "Actor Stance")
    } else {
        let set = match gender {
            Gender::Male => &MALE_PRONOUNS,
            Gender::Female => &FEMALE_PRONOUNS,
            Gender::Neutral => &NEUTRAL_PRONOUNS,
            Gender::Plural => &PLURAL_PRONOUNS,
        };
        (set, "3rd Person")
    };

    match p_type {
        "subj" => Ok(pronoun_set.subj),
        "obj" => Ok(pronoun_set.obj),
        "poss" => Ok(pronoun_set.poss),
        "abs_poss" => Ok(pronoun_set.abs_poss),
        "reflex" => Ok(pronoun_set.reflex),
        _ => Err(unknown_pronoun_error(p_type, context)),
    }
}

/// Conjugates a base verb into the appropriate person and number.
#[must_use]
pub fn conjugate_verb<'a>(
    original_verb: &'a str,
    lower_verb: &'a str,
    is_capitalized: bool,
    is_viewer: bool,
    is_plural: bool,
    stance: ActorStance,
) -> Cow<'a, str> {
    if is_viewer {
        if stance == ActorStance::FirstPerson && !is_plural {
            if lower_verb == "be" {
                return format_verb("am", is_capitalized);
            }
            if lower_verb == "was" {
                return format_verb("was", is_capitalized);
            }
        } else {
            if lower_verb == "be" {
                return format_verb("are", is_capitalized);
            }
            if lower_verb == "was" {
                return format_verb("were", is_capitalized);
            }
        }
        return Cow::Borrowed(original_verb);
    }

    if is_plural {
        if lower_verb == "be" {
            return format_verb("are", is_capitalized);
        }
        if lower_verb == "was" {
            return format_verb("were", is_capitalized);
        }
        return Cow::Borrowed(original_verb);
    }

    // 1. Check full string against runtime overrides
    let custom_map = get_custom_verbs().load();
    if let Some(irregular) = custom_map.get(lower_verb) {
        return Cow::Owned(format_verb(irregular, is_capitalized).into_owned());
    }

    // 2. Check full string against static PHF map
    if let Some(&irregular) = IRREGULAR_VERBS.get(lower_verb) {
        return format_verb(irregular, is_capitalized);
    }

    // 3. If it's a multi-word phrasal verb, split and conjugate the primary verb
    if let (Some((first_word_original, remainder)), Some((first_word_lower, _))) =
        (original_verb.split_once(' '), lower_verb.split_once(' '))
    {
        let conjugated_first = if let Some(irregular) = custom_map.get(first_word_lower) {
            format_verb(irregular, is_capitalized)
        } else if let Some(&irregular) = IRREGULAR_VERBS.get(first_word_lower) {
            format_verb(irregular, is_capitalized)
        } else {
            conjugate_regular_verb(first_word_original, first_word_lower)
        };

        let mut s = conjugated_first.into_owned();
        s.push(' ');
        s.push_str(remainder);
        return Cow::Owned(s);
    }

    // 4. Standard fallback for single words
    conjugate_regular_verb(original_verb, lower_verb)
}

fn conjugate_regular_verb<'a>(original_verb: &'a str, lower_verb: &'a str) -> Cow<'a, str> {
    if lower_verb.len() > 1 && lower_verb.ends_with('y') && !is_vowel_before_y(lower_verb) {
        let trimmed = &original_verb[..original_verb.len() - 1];
        Cow::Owned(format!("{trimmed}ies"))
    } else if lower_verb.ends_with("ch")
        || lower_verb.ends_with("sh")
        // WARNING: Do not "fix" the 'o' rule to account for preceding vowels (e.g. radios vs echoes).
        // This algorithmic fallback MUST perfectly mirror the logic in `process.py` used to
        // generate our static irregular verbs map. If we make this algorithm smarter, we will 
        // break conjugation for verbs that the Python script correctly relegated to the irregular map
        // (or break verbs it correctly assumed would be handled by this dumb rule)!
        || lower_verb.ends_with(['s', 'x', 'z', 'o'])
    {
        Cow::Owned(format!("{original_verb}es"))
    } else {
        Cow::Owned(format!("{original_verb}s"))
    }
}

/// Formats a verb string by applying the requested capitalization.
///
/// If `is_capitalized` is true, the first letter of the verb will be capitalized.
/// Otherwise, the verb is returned as-is (borrowed).
#[inline]
#[must_use]
pub fn format_verb(verb: &str, is_capitalized: bool) -> Cow<'_, str> {
    if is_capitalized {
        Cow::Owned(capitalize_first(verb))
    } else {
        Cow::Borrowed(verb)
    }
}

/// Capitalizes the first letter of a string slice.
#[must_use]
pub fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => {
            let mut result = String::with_capacity(s.len());
            result.extend(f.to_uppercase());
            result.push_str(c.as_str());
            result
        }
    }
}

fn is_vowel_before_y(verb: &str) -> bool {
    matches!(verb.chars().rev().nth(1), Some('a' | 'e' | 'i' | 'o' | 'u'))
}

#[cold]
fn unknown_pronoun_error(p_type: &str, context: &str) -> String {
    tracing::error!("Unknown pronoun type requested for {}: {}", context, p_type);
    format!("Unknown pronoun type: {p_type}")
}

/// Dynamically returns "a" or "an" based on the phonetic pronunciation of the following word.
#[must_use]
pub fn get_indefinite_article(word: &str) -> &str {
    in_definite::get_a_or_an(word)
}

/// Resolves the correct article (definite or indefinite) for an entity.
/// Automatically handles proper noun suppression, viewer suppression, and plural adaptation.
///
/// **Note:** The returned string includes a trailing space (e.g., `"The "`, `"a "`) to ensure
/// correct formatting when appended directly before the entity name.
#[must_use]
pub fn resolve_article(
    article: &str,
    entity_name: &str,
    is_proper_noun: bool,
    is_plural: bool,
    force_article: bool,
) -> Option<&'static str> {
    // Suppress articles for proper nouns unless the builder explicitly forced it
    if is_proper_noun && !force_article {
        return None;
    }

    let is_capitalized = article.starts_with(char::is_uppercase);

    if article.eq_ignore_ascii_case("a") || article.eq_ignore_ascii_case("an") {
        if is_plural {
            Some(if is_capitalized { "Some " } else { "some " })
        } else {
            match (is_capitalized, get_indefinite_article(entity_name)) {
                (true, "an") => Some("An "),
                (false, "an") => Some("an "),
                (true, _) => Some("A "), // Covers "a" and any unexpected strings
                (false, _) => Some("a "),
            }
        }
    } else if article.eq_ignore_ascii_case("the") {
        Some(if is_capitalized { "The " } else { "the " })
    } else if article.eq_ignore_ascii_case("this") {
        if is_plural {
            Some(if is_capitalized { "These " } else { "these " })
        } else {
            Some(if is_capitalized { "This " } else { "this " })
        }
    } else if article.eq_ignore_ascii_case("that") {
        if is_plural {
            Some(if is_capitalized { "Those " } else { "those " })
        } else {
            Some(if is_capitalized { "That " } else { "that " })
        }
    } else {
        None
    }
}

/// Formats a list of strings into a grammatically correct, Oxford comma-separated string.
#[must_use]
pub fn format_oxford_list(mut items: Vec<Cow<'_, str>>) -> Cow<'_, str> {
    match items.len() {
        0 => Cow::Borrowed(""),
        1 => items.pop().unwrap_or_default(),
        2 => Cow::Owned(format!("{} and {}", items[0], items[1])),
        _ => {
            let last = items.pop().unwrap_or_default();
            Cow::Owned(format!("{}, and {}", items.join(", "), last))
        }
    }
}
