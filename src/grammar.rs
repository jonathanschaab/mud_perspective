use crate::models::Gender;
use phf::phf_map;
use std::borrow::Cow;

// O(1) static lookup table for the most common irregular verbs
// This prevents you from having to evaluate slow logic trees for basic words.
static IRREGULAR_VERBS: phf::Map<&'static str, &'static str> = phf_map! {
    "be" => "is",
    "have" => "has",
    "do" => "does",
    "go" => "goes",
    "say" => "says",
    "was" => "was",

    // Modal verbs mapped to themselves to prevent "s" suffixes
    "can" => "can",
    "could" => "could",
    "will" => "will",
    "would" => "would",
    "shall" => "shall",
    "should" => "should",
    "may" => "may",
    "might" => "might",
    "must" => "must",
};

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

/// Returns the correct pronoun based on gender, type, and perspective.
///
/// # Errors
/// Returns a `String` error if the provided `p_type` is an unknown or unsupported pronoun case.
pub fn resolve_pronoun(
    gender: Gender,
    p_type: &str,
    is_viewer: bool,
    is_plural: bool,
) -> Result<&'static str, String> {
    let (pronoun_set, context) = if is_viewer {
        let set = if is_plural {
            &VIEWER_PLURAL_PRONOUNS
        } else {
            &VIEWER_SINGULAR_PRONOUNS
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
) -> Cow<'a, str> {
    // 1st/2nd person (viewer) AND 3rd-person plural subjects use the base uninflected verb,
    // EXCEPT for the highly irregular verb "to be" which becomes "are".
    if is_viewer || is_plural {
        if lower_verb == "be" {
            return format_verb("are", is_capitalized);
        }
        if lower_verb == "was" {
            return format_verb("were", is_capitalized);
        }
        // If you want strict 1st person singular ("I am") you can split `is_viewer` logic later,
        // but for Actor Stance ("You"), "are" is always correct.
        return Cow::Borrowed(original_verb);
    }

    // 1. Check our static PHF map for irregular overrides (3rd person singular)
    if let Some(&irregular) = IRREGULAR_VERBS.get(lower_verb) {
        return format_verb(irregular, is_capitalized);
    }

    // 2. Fallback algorithmic suffix rules for standard verbs
    if lower_verb.ends_with("ch")
        || lower_verb.ends_with("sh")
        || lower_verb.ends_with(['s', 'x', 'z'])
    {
        Cow::Owned(format!("{original_verb}es"))
    } else if lower_verb.len() > 1 && lower_verb.ends_with('y') && !is_vowel_before_y(lower_verb) {
        let trimmed = &original_verb[..original_verb.len() - 1];
        Cow::Owned(format!("{trimmed}ies"))
    } else {
        Cow::Owned(format!("{original_verb}s"))
    }
}

#[inline]
fn format_verb(verb: &str, is_capitalized: bool) -> Cow<'_, str> {
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
///
/// # Panics
/// Panics if internal vector bounds checks fail (though the function logic guarantees this is impossible).
#[must_use]
pub fn format_oxford_list(mut items: Vec<Cow<'_, str>>) -> Cow<'_, str> {
    match items.len() {
        0 => Cow::Borrowed(""),
        1 => items.pop().unwrap(),
        2 => Cow::Owned(format!("{} and {}", items[0], items[1])),
        _ => {
            let last = items.pop().unwrap();
            Cow::Owned(format!("{}, and {}", items.join(", "), last))
        }
    }
}
