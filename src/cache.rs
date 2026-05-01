use crate::engine::Template;
use moka::sync::Cache;
use std::sync::Arc;

/// A highly concurrent, thread-safe cache for compiled text templates.
///
/// Because MUDs process high volumes of text concurrently, this cache wraps the
/// underlying owned ASTs in an `Arc`. This allows multiple network threads to safely
/// read and render the same compiled template simultaneously without lock contention on cache hits.
pub struct TemplateCache {
    inner: Cache<String, Arc<Template>>,
}

impl TemplateCache {
    /// Initializes a new thread-safe template cache.
    ///
    /// # Arguments
    /// * `capacity` - The maximum number of templates to keep in memory. Once
    ///   exceeded, the least recently used templates are automatically evicted.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Cache::builder()
                .max_capacity(capacity.max(1) as u64)
                .build(),
        }
    }

    /// Retrieves a compiled template from the cache, or compiles it on the fly if missing.
    ///
    /// # Arguments
    /// * `raw` - The raw template string to fetch or compile.
    ///
    /// # Errors
    /// Returns a `String` describing the syntax error if a cache miss occurs and
    /// the subsequent compilation fails.
    pub fn get_or_compile(&self, raw: &str) -> Result<Arc<Template>, String> {
        // First, try a lock-free read using a borrowed string slice to avoid allocation on cache hits.
        if let Some(template) = self.inner.get(raw) {
            return Ok(template);
        }

        // `try_get_with` ensures that if multiple threads request the same missing
        // template simultaneously, only one thread will execute the compilation closure.
        // The others will wait and receive the compiled result safely.
        self.inner
            .try_get_with(raw.to_string(), || {
                tracing::debug!("Cache miss: Compiling AST for template.");
                Template::compile(raw).map(Arc::new)
            })
            .map_err(|e| (*e).clone())
    }
}
