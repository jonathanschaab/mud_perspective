//! A Rust library designed to handle perspective-aware text generation for MUDs and interactive fiction.

#![deny(missing_docs)]

/// Thread-safe caching for compiled templates.
pub mod cache;
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

#[cfg(test)]
mod tests;
