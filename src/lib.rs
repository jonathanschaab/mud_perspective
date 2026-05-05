//! A Rust library designed to handle perspective-aware text generation for MUDs and interactive fiction.

#![deny(missing_docs)]
#![warn(
    clippy::pedantic,
    clippy::cargo,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable,
    clippy::indexing_slicing
)]
#![allow(
    // module_name_repetitions will complain if a struct is named `TemplateCache` inside the `cache` module.
    // It is highly subjective, and usually fine to disable.
    clippy::module_name_repetitions,
    // Often unavoidable when relying on third-party crates that use different versions of the same dependency.
    clippy::multiple_crate_versions
)]

/// Thread-safe caching for compiled templates.
pub mod cache;
/// Utilities for testing and debugging templates.
pub mod debug;
/// The core AST and template rendering engine.
pub mod engine;
/// Grammar rules and NLP helpers for English.
pub mod grammar;
/// Core data structures and traits for perspective rendering.
pub mod models;

/// Renders a perspective-aware message ergonomically by abstracting the context initialization.
///
/// This declarative macro transforms the verbose builder pattern into a clean,
/// single-line function call, allowing developers to inject entity mappings as
/// trailing key-value pairs.
///
/// # Arguments
/// * `viewer_id` - The string ID of the observing entity.
/// * `template` - The compiled `Template` or `Arc<Template>` to render.
/// * `key => entity` - A comma-separated list mapping template string keys to `&dyn TemplateEntity`.
///
/// # Returns
/// A `Result<String, String>` containing the final formatted text.
///
/// # Example
/// ```ignore
/// let output = render_msg!(
///     "observer_1",
///     &compiled_template,
///     "source" => &player_entity,
///     "target" => &enemy_entity
/// );
/// ```
#[macro_export]
macro_rules! render_msg {
    // Matches the viewer ID, the template, and a comma-separated list of key => entity pairs
    ($viewer:expr, $template:expr $(, $key:expr => $entity:expr)* $(,)?) => {{
        // We use $crate to ensure the macro works no matter where it's called from
        let ctx = $crate::models::RenderContext::new($viewer)
        $(
            // This line repeats for every key-value pair passed to the macro
           .with_entity($key, $entity)
        )*;

        // Render and return the Result
        $crate::engine::PerspectiveEngine::render($template, &ctx)
    }};
}

/// Ergonomically registers multiple custom irregular verbs into the runtime dictionary at once.
///
/// This macro is useful during server initialization for injecting a large
/// number of custom or lore-specific verbs. It silently overwrites any existing
/// runtime entries for the provided base verbs.
///
/// # Example
/// ```ignore
/// mud_perspective::register_custom_verbs! {
///     "yeet" => ("yeetses", "yeeted"),
///     "make do" => ("makes do", "made do"),
///     "respawn" => ("respawns", "respawned"),
/// };
/// ```
#[macro_export]
macro_rules! register_custom_verbs {
    ($($base:expr => ($present:expr, $past:expr)),* $(,)?) => {
        $( $crate::grammar::force_add_irregular_verb($base, $present, $past); )*
    };
}

#[cfg(test)]
mod tests;
