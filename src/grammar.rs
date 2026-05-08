use crate::models::{ActorStance, Gender, Tense};
use arc_swap::ArcSwap;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

include!(concat!(env!("OUT_DIR"), "/irregular_verbs.rs"));

/// A global runtime dictionary for custom irregular verbs injected by builders.
static CUSTOM_VERBS: OnceLock<ArcSwap<HashMap<String, (String, String)>>> = OnceLock::new();

fn get_custom_verbs() -> &'static ArcSwap<HashMap<String, (String, String)>> {
    CUSTOM_VERBS.get_or_init(|| ArcSwap::from_pointee(HashMap::new()))
}

/// Retrieves a list of possible past-tense conjugations for an ambiguous base verb, if any.
#[must_use]
pub fn get_collision_options(verb: &str) -> Option<&'static [&'static str]> {
    COLLIDING_VERBS.get(verb).copied()
}

/// Adds a custom irregular verb override to the runtime dictionary.
///
/// # Errors
/// Returns an error if the verb already exists in either the static dictionary or the runtime dictionary.
pub fn add_irregular_verb(base: &str, present: &str, past: &str) -> Result<(), String> {
    let lower_base = base.to_lowercase();
    if IRREGULAR_VERBS.contains_key(lower_base.as_str()) {
        return Err(format!(
            "Verb '{lower_base}' already exists in the static dictionary."
        ));
    }

    let lower_present = present.to_lowercase();
    let lower_past = past.to_lowercase();
    let custom_verbs = get_custom_verbs();
    loop {
        let current_map = custom_verbs.load();
        if current_map.contains_key(&lower_base) {
            return Err(format!(
                "Verb '{lower_base}' already exists in the runtime dictionary."
            ));
        }

        let mut new_map = (**current_map).clone();
        new_map.insert(
            lower_base.clone(),
            (lower_present.clone(), lower_past.clone()),
        );

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
pub fn force_add_irregular_verb(base: &str, present: &str, past: &str) {
    let lower_base = base.to_lowercase();
    let lower_present = present.to_lowercase();
    let lower_past = past.to_lowercase();
    get_custom_verbs().rcu(|current_map| {
        let mut new_map = (**current_map).clone();
        new_map.insert(
            lower_base.clone(),
            (lower_present.clone(), lower_past.clone()),
        );
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
    mut original_verb: &'a str,
    mut lower_verb: &'a str,
    is_capitalized: bool,
    is_viewer: bool,
    is_plural: bool,
    stance: ActorStance,
    tense: Tense,
) -> Cow<'a, str> {
    if lower_verb == "do(aux)" {
        if tense == Tense::Future {
            return Cow::Borrowed(if is_capitalized { "Will" } else { "will" });
        }
        original_verb = if is_capitalized { "Do" } else { "do" };
        lower_verb = "do";
    }

    if tense == Tense::Future {
        // Modal verbs naturally imply future capability or obligation, and cannot be
        // stacked with "will" in standard English (e.g., "will must" is invalid).
        if matches!(
            lower_verb,
            "can"
                | "could"
                | "may"
                | "might"
                | "must"
                | "shall"
                | "should"
                | "will"
                | "would"
                | "ought"
                | "ought to"
        ) {
            return format_verb(original_verb, is_capitalized);
        }

        // Lowercase the first character of the base verb so it sits cleanly after "will",
        // but preserve any inner camelCase (e.g., "MacGyver" -> "will macGyver")
        let prefix = if is_capitalized { "Will " } else { "will " };
        let mut chars = original_verb.chars();

        if let Some(first_char) = chars.next() {
            return Cow::Owned(format!(
                "{prefix}{}{}",
                first_char.to_lowercase(),
                chars.as_str()
            ));
        }

        return Cow::Owned(prefix.trim_end().to_string());
    }

    let is_first_person_singular = is_viewer && stance == ActorStance::FirstPerson && !is_plural;

    if lower_verb == "be" {
        if tense == Tense::Past {
            if is_first_person_singular || (!is_viewer && !is_plural) {
                return format_verb("was", is_capitalized);
            }
            return format_verb("were", is_capitalized);
        }

        if is_first_person_singular {
            return format_verb("am", is_capitalized);
        } else if is_viewer || is_plural {
            return format_verb("are", is_capitalized);
        }
        return format_verb("is", is_capitalized);
    }

    if tense == Tense::Present && (is_viewer || is_plural) {
        return capitalize_cow(Cow::Borrowed(original_verb), is_capitalized);
    }

    // 1. Check full string against runtime overrides
    let custom_map = get_custom_verbs().load();
    if let Some((present, past)) = custom_map.get(lower_verb) {
        let word = if tense == Tense::Past { past } else { present };
        return Cow::Owned(format_verb(word, is_capitalized).into_owned());
    }

    // 2. Check full string against static PHF map
    if let Some(&(present, past)) = IRREGULAR_VERBS.get(lower_verb) {
        let word = if tense == Tense::Past { past } else { present };
        return format_verb(word, is_capitalized);
    }

    // 3. If it's a multi-word phrasal verb, split and conjugate the primary verb
    if let (Some((first_word_original, remainder)), Some((first_word_lower, _))) =
        (original_verb.split_once(' '), lower_verb.split_once(' '))
    {
        let conjugated_first = if let Some((present, past)) = custom_map.get(first_word_lower) {
            let word = if tense == Tense::Past { past } else { present };
            format_verb(word, is_capitalized)
        } else if let Some(&(present, past)) = IRREGULAR_VERBS.get(first_word_lower) {
            let word = if tense == Tense::Past { past } else { present };
            format_verb(word, is_capitalized)
        } else if tense == Tense::Past {
            capitalize_cow(
                conjugate_regular_past_verb(first_word_original, first_word_lower),
                is_capitalized,
            )
        } else {
            capitalize_cow(
                conjugate_regular_verb(first_word_original, first_word_lower),
                is_capitalized,
            )
        };

        let mut s = conjugated_first.into_owned();
        s.push(' ');
        s.push_str(remainder);
        return Cow::Owned(s);
    }

    // 4. Standard fallback for single words
    if tense == Tense::Past {
        capitalize_cow(
            conjugate_regular_past_verb(original_verb, lower_verb),
            is_capitalized,
        )
    } else {
        capitalize_cow(
            conjugate_regular_verb(original_verb, lower_verb),
            is_capitalized,
        )
    }
}

fn conjugate_regular_verb<'a>(original_verb: &'a str, lower_verb: &'a str) -> Cow<'a, str> {
    if lower_verb.len() > 1 && lower_verb.ends_with('y') && !is_vowel_before_y(lower_verb) {
        let trimmed = original_verb
            .get(..original_verb.len() - 1)
            .unwrap_or(original_verb);
        Cow::Owned(format!("{trimmed}ies"))
    } else if lower_verb.ends_with("ch")
        || lower_verb.ends_with("sh")
        // WARNING: Do not "fix" the 'o' rule to account for preceding vowels (e.g. radios vs echoes).
        // This algorithmic fallback MUST strictly mirror the logic in `process.py` used to
        // generate our static irregular verbs map. If we alter this algorithm, we will 
        // break conjugation for verbs that the Python script relegated to the irregular map
        // (or break verbs it assumed would be handled by this rule)!
        || lower_verb.ends_with(['s', 'x', 'z', 'o'])
    {
        Cow::Owned(format!("{original_verb}es"))
    } else {
        Cow::Owned(format!("{original_verb}s"))
    }
}

fn conjugate_regular_past_verb<'a>(original_verb: &'a str, lower_verb: &'a str) -> Cow<'a, str> {
    if lower_verb.ends_with('e') {
        Cow::Owned(format!("{original_verb}d"))
    } else if lower_verb.len() > 1 && lower_verb.ends_with('y') && !is_vowel_before_y(lower_verb) {
        let trimmed = original_verb
            .get(..original_verb.len() - 1)
            .unwrap_or(original_verb);
        Cow::Owned(format!("{trimmed}ied"))
    } else {
        Cow::Owned(format!("{original_verb}ed"))
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

/// Conditionally pushes a string to an output buffer, capitalizing the first letter if requested.
#[inline]
pub(crate) fn push_capitalized_if(output: &mut String, text: &str, should_capitalize: bool) {
    if should_capitalize && text.chars().next().is_some_and(char::is_lowercase) {
        let mut c = text.chars();
        if let Some(f) = c.next() {
            output.extend(f.to_uppercase());
            output.push_str(c.as_str());
        }
    } else {
        output.push_str(text);
    }
}

/// Conditionally capitalizes the first letter of a `Cow<str>`, returning a new `Cow`.
#[inline]
pub(crate) fn capitalize_cow(text: Cow<'_, str>, should_capitalize: bool) -> Cow<'_, str> {
    if should_capitalize && text.chars().next().is_some_and(char::is_lowercase) {
        Cow::Owned(capitalize_first(&text))
    } else {
        text
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

/// Converts a number from 0-999 to its cardinal word representation.
fn to_cardinal_words_lt_1000(n: usize) -> String {
    if n == 0 {
        return "zero".to_string();
    }
    to_words_lt_1000(n, false)
}

/// Converts a number from 0-999 to its ordinal word representation.
fn to_ordinal_words_lt_1000(n: usize) -> String {
    if n == 0 {
        return "zeroth".to_string();
    }
    to_words_lt_1000(n, true)
}

const CARDINAL_ONES: &[&str] = &[
    "", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
];

const CARDINAL_TEENS: &[&str] = &[
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];

const DECADES: &[&str] = &[
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

#[inline]
fn make_word_ordinal(last_word: &mut String) {
    const REPLACEMENTS: &[(&str, &str)] = &[
        ("one", "first"),
        ("two", "second"),
        ("three", "third"),
        ("five", "fifth"),
        ("eight", "eighth"),
        ("nine", "ninth"),
        ("twelve", "twelfth"),
    ];

    if last_word.ends_with('y') {
        last_word.pop(); // Remove the 'y'
        last_word.push_str("ieth");
        return;
    }

    for &(suffix, replacement) in REPLACEMENTS {
        if last_word.ends_with(suffix) {
            last_word.truncate(last_word.len() - suffix.len());
            last_word.push_str(replacement);
            return;
        }
    }

    last_word.push_str("th");
}

/// Internal helper to convert a number from 0-999 to words, with an ordinal flag.
fn to_words_lt_1000(mut n: usize, is_ordinal: bool) -> String {
    let mut parts = Vec::new();
    if n >= 100 {
        parts.push(format!(
            "{} hundred",
            CARDINAL_ONES.get(n / 100).copied().unwrap_or_default()
        ));
        n %= 100;
    }

    if n > 0 {
        if !parts.is_empty() {
            parts.push("and".to_string());
        }
        if (10..20).contains(&n) {
            parts.push(
                CARDINAL_TEENS
                    .get(n - 10)
                    .copied()
                    .unwrap_or_default()
                    .to_string(),
            );
        } else {
            let tens_digit = n / 10;
            let ones_digit = n % 10;

            if tens_digit >= 2 {
                let mut current_part = DECADES
                    .get(tens_digit)
                    .copied()
                    .unwrap_or_default()
                    .to_string();
                if ones_digit > 0 {
                    current_part = format!(
                        "{}-{}",
                        current_part,
                        CARDINAL_ONES.get(ones_digit).copied().unwrap_or_default()
                    );
                }
                parts.push(current_part);
            } else if ones_digit > 0 {
                parts.push(
                    CARDINAL_ONES
                        .get(ones_digit)
                        .copied()
                        .unwrap_or_default()
                        .to_string(),
                );
            }
        }
    } else if parts.is_empty() {
        return (if is_ordinal { "zeroth" } else { "zero" }).to_string();
    }

    if is_ordinal {
        // Apply the ordinal suffix to the last word.
        if let Some(last_word) = parts.last_mut() {
            make_word_ordinal(last_word);
        }
    }

    parts.join(" ")
}

/// Dynamically returns "a" or "an" based on the phonetic pronunciation of the following word.
#[must_use]
pub fn get_indefinite_article(word: &str) -> &str {
    in_definite::get_a_or_an(word)
}

/// Converts a number into its ordinal word representation (e.g., 3 -> "third").
/// This function handles numbers up to `usize::MAX`.
///
/// If the number `n` exceeds the provided `threshold`, it will be formatted as an integer
/// with the appropriate suffix (e.g., "1001st").
#[must_use]
pub fn number_to_ordinal_word(n: usize, threshold: usize) -> String {
    if n > threshold {
        let suffix = match n % 100 {
            11..=13 => "th",
            _ => match n % 10 {
                1 => "st",
                2 => "nd",
                3 => "rd",
                _ => "th",
            },
        };
        return format!("{n}{suffix}");
    }

    if n == 0 {
        return "zeroth".to_string();
    }

    let scales = [
        (1_000_000_000_000_000_000, "quintillion"),
        (1_000_000_000_000_000, "quadrillion"),
        (1_000_000_000_000, "trillion"),
        (1_000_000_000, "billion"),
        (1_000_000, "million"),
        (1_000, "thousand"),
    ];

    let mut parts = Vec::new();
    let mut remainder = n;

    for (scale_val, scale_name) in &scales {
        if remainder >= *scale_val {
            let count = remainder / scale_val;
            parts.push(format!(
                "{} {}",
                to_cardinal_words_lt_1000(count),
                scale_name
            ));
            remainder %= scale_val;
        }
    }

    if remainder > 0 {
        let is_last_part = parts.is_empty();
        if !is_last_part && remainder < 100 && remainder != 0 {
            parts.push("and".to_string());
        }
        parts.push(to_ordinal_words_lt_1000(remainder));
    } else {
        // The number was an exact multiple of a scale, e.g., 1,000,000
        if let Some(last_part) = parts.last_mut() {
            // For exact multiples, ensure the scale word itself becomes ordinal (e.g., "millionth")
            if last_part.ends_with("ion") {
                // Common ending for large scale words
                last_part.push_str("th");
            } else if last_part.ends_with("dred") {
                // For "hundred"
                *last_part = last_part.replace("dred", "dredth");
            } else {
                // Fallback for "thousand"
                last_part.push_str("th");
            }
        }
    }

    parts.join(" ")
}

bitflags::bitflags! {
    /// Flags to configure article resolution rules.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ArticleFlags: u8 {
        /// The entity is a proper noun (normally suppresses articles).
        const IS_PROPER_NOUN = 1 << 0;
        /// The entity is plural (modifies indefinite articles).
        const IS_PLURAL = 1 << 1;
        /// The builder explicitly forced the article to render.
        const FORCE_ARTICLE = 1 << 2;
        /// The article directly follows a possessive word (normally suppresses articles).
        const AFTER_POSSESSIVE = 1 << 3;
        /// Internal flag indicating the article should be capitalized.
        const IS_CAPITALIZED = 1 << 4;
    }
}

#[inline]
fn apply_ordinal_article(
    base: &str,
    ord: usize,
    collective_noun: Option<&str>,
    current_threshold: usize,
    flags: ArticleFlags,
) -> Cow<'static, str> {
    let ord_word = number_to_ordinal_word(ord, current_threshold);
    let group_word = collective_noun.unwrap_or("set");
    let prefix = if flags.contains(ArticleFlags::AFTER_POSSESSIVE) {
        if flags.contains(ArticleFlags::IS_PLURAL) {
            format!("{ord_word} {group_word} of ")
        } else {
            format!("{ord_word} ")
        }
    } else if flags.contains(ArticleFlags::IS_PLURAL) {
        if base.eq_ignore_ascii_case("some") || base.is_empty() {
            let a_or_an = get_indefinite_article(&ord_word);
            format!("{a_or_an} {ord_word} {group_word} of ")
        } else if base.eq_ignore_ascii_case("these") || base.eq_ignore_ascii_case("this") {
            format!("this {ord_word} {group_word} of ")
        } else if base.eq_ignore_ascii_case("those") || base.eq_ignore_ascii_case("that") {
            format!("that {ord_word} {group_word} of ")
        } else if base.eq_ignore_ascii_case("one")
            || base.eq_ignore_ascii_case("one of")
            || base.eq_ignore_ascii_case("one of the")
        {
            format!("one of the {ord_word} {group_word} of ")
        } else {
            format!("{base} {ord_word} {group_word} of ")
        }
    } else if base.is_empty() {
        let a_or_an = get_indefinite_article(&ord_word);
        format!("{a_or_an} {ord_word} ")
    } else {
        format!("{base} {ord_word} ")
    };
    Cow::Owned(
        if flags.contains(ArticleFlags::IS_CAPITALIZED)
            && !flags.contains(ArticleFlags::AFTER_POSSESSIVE)
        {
            capitalize_first(&prefix)
        } else {
            prefix
        },
    )
}

#[inline]
fn try_resolve_ordinal_article(
    article: &str,
    ord: usize,
    collective_noun: Option<&str>,
    current_threshold: usize, // Renamed to avoid shadowing
    flags: ArticleFlags,
) -> Option<Cow<'static, str>> {
    let mut base = "";
    let mut applies = true;

    if article.eq_ignore_ascii_case("a") || article.eq_ignore_ascii_case("an") {
        base = if flags.contains(ArticleFlags::IS_PLURAL) {
            "some"
        } else {
            ""
        };
        applies = ord > 2;
    } else if article.eq_ignore_ascii_case("another") {
        applies = flags.contains(ArticleFlags::IS_PLURAL)
            || ord > 2
            || flags.contains(ArticleFlags::AFTER_POSSESSIVE);
    } else if article.eq_ignore_ascii_case("the") {
        base = "the";
    } else if article.eq_ignore_ascii_case("this") {
        base = "this";
    } else if article.eq_ignore_ascii_case("that") {
        base = "that";
    } else if article.eq_ignore_ascii_case("one") {
        base = "one";
    } else if article.eq_ignore_ascii_case("one of") || article.eq_ignore_ascii_case("one of the") {
        base = "one of the";
    } else if article.eq_ignore_ascii_case("some") {
        base = "some";
    } else {
        applies = false;
    }

    if applies {
        Some(apply_ordinal_article(
            base,
            ord,
            collective_noun,
            current_threshold,
            flags,
        ))
    } else {
        None
    }
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
    ordinal: Option<usize>,
    collective_noun: Option<&str>,
    current_threshold: usize, // Renamed to avoid shadowing
    mut flags: ArticleFlags,
) -> Option<Cow<'static, str>> {
    // Suppress articles for proper nouns unless the builder explicitly forced it
    if flags.contains(ArticleFlags::IS_PROPER_NOUN) && !flags.contains(ArticleFlags::FORCE_ARTICLE)
    {
        return None;
    }

    // If force_article is set, it overrides the after_possessive suppression logic.
    if flags.contains(ArticleFlags::FORCE_ARTICLE) {
        flags.remove(ArticleFlags::AFTER_POSSESSIVE);
    }

    let mut is_capitalized = flags.contains(ArticleFlags::IS_CAPITALIZED);
    if article.starts_with(char::is_uppercase) {
        is_capitalized = true;
        flags.set(ArticleFlags::IS_CAPITALIZED, true);
    }

    // Fast-path for ordinals to avoid duplicating logic across all article types
    if let Some(ord) = ordinal
        && let Some(resolved) =
            try_resolve_ordinal_article(article, ord, collective_noun, current_threshold, flags)
    {
        return Some(resolved);
    }

    if flags.contains(ArticleFlags::AFTER_POSSESSIVE) {
        return None;
    }

    if article.eq_ignore_ascii_case("a") || article.eq_ignore_ascii_case("an") {
        if flags.contains(ArticleFlags::IS_PLURAL) {
            Some(Cow::Borrowed(if is_capitalized {
                "Some "
            } else {
                "some "
            }))
        } else {
            match (is_capitalized, get_indefinite_article(entity_name)) {
                (true, "an") => Some(Cow::Borrowed("An ")),
                (false, "an") => Some(Cow::Borrowed("an ")),
                (true, _) => Some(Cow::Borrowed("A ")), // Covers "a" and any unexpected strings
                (false, _) => Some(Cow::Borrowed("a ")),
            }
        }
    } else if article.eq_ignore_ascii_case("the") {
        Some(Cow::Borrowed(if is_capitalized { "The " } else { "the " }))
    } else if article.eq_ignore_ascii_case("this") {
        if flags.contains(ArticleFlags::IS_PLURAL) {
            Some(Cow::Borrowed(if is_capitalized {
                "These "
            } else {
                "these "
            }))
        } else {
            Some(Cow::Borrowed(if is_capitalized {
                "This "
            } else {
                "this "
            }))
        }
    } else if article.eq_ignore_ascii_case("that") {
        if flags.contains(ArticleFlags::IS_PLURAL) {
            Some(Cow::Borrowed(if is_capitalized {
                "Those "
            } else {
                "those "
            }))
        } else {
            Some(Cow::Borrowed(if is_capitalized {
                "That "
            } else {
                "that "
            }))
        }
    } else if article.eq_ignore_ascii_case("another") {
        if flags.contains(ArticleFlags::IS_PLURAL) {
            Some(Cow::Borrowed(if is_capitalized {
                "Other "
            } else {
                "other "
            }))
        } else {
            Some(Cow::Borrowed(if is_capitalized {
                "Another "
            } else {
                "another "
            }))
        }
    } else if article.eq_ignore_ascii_case("one") {
        Some(Cow::Borrowed(if is_capitalized { "One " } else { "one " }))
    } else if article.eq_ignore_ascii_case("one of") || article.eq_ignore_ascii_case("one of the") {
        Some(Cow::Borrowed(if is_capitalized {
            "One of the "
        } else {
            "one of the "
        }))
    } else if article.eq_ignore_ascii_case("some") {
        Some(Cow::Borrowed(if is_capitalized {
            "Some "
        } else {
            "some "
        }))
    } else {
        None
    }
}

/// Formats a list of strings into an Oxford comma-separated string.
#[must_use]
pub fn format_oxford_list<'a>(mut items: Vec<Cow<'a, str>>, conjunction: &str) -> Cow<'a, str> {
    match items.len() {
        0 => Cow::Borrowed(""),
        1 => items.pop().unwrap_or_default(),
        2 => {
            let second = items.pop().unwrap_or_default();
            let first = items.pop().unwrap_or_default();
            Cow::Owned(format!("{first} {conjunction} {second}"))
        }
        _ => {
            let last = items.pop().unwrap_or_default();
            Cow::Owned(format!("{}, {conjunction} {}", items.join(", "), last))
        }
    }
}
