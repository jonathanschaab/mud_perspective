use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use crate::engine::Template;

/// A thread-safe Least Recently Used (LRU) cache for compiled text templates.
///
/// Because MUDs process high volumes of text concurrently, this cache wraps the 
/// underlying owned ASTs in an `Arc`. This allows multiple network threads to safely 
/// read and render the same compiled template simultaneously without incurring cloning costs,
/// and safely stores templates regardless of the lifetime of the original raw database strings.
pub struct TemplateCache {
    // We use Arc<Template> so multiple threads can read the same compiled AST 
    // simultaneously without having to clone the underlying Vec of Tokens.
    inner: Mutex<LruCache<String, Arc<Template>>>,
}

impl TemplateCache {
    /// Initializes a new thread-safe template cache.
    ///
    /// # Arguments
    /// * `capacity` - The maximum number of templates to keep in memory. Once 
    ///   exceeded, the least recently used templates are automatically evicted.
    pub fn new(capacity: usize) -> Self {
        Self {
            // Default to a minimum capacity of 1 to prevent a panic if capacity is 0.
            inner: Mutex::new(LruCache::new(NonZeroUsize::new(capacity.max(1)).unwrap())),
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
        // First, check if the template is already in the cache, holding the lock briefly.
        if let Some(template) = self.inner.lock().unwrap().get(raw) {
            return Ok(Arc::clone(template));
        }

        // If not, compile it. This is the slow part, and it happens outside the lock
        // to prevent holding up other threads that might need the cache for other templates.
        let compiled_template = Template::compile(raw)?;
        let arc_template = Arc::new(compiled_template);

        // Now, re-acquire the lock to insert the compiled template.
        let mut cache = self.inner.lock().unwrap();

        // It's possible another thread also had a cache miss and inserted the template
        // while we were compiling. We check again. If it exists, we use it. If not,
        // we insert our newly compiled one. This is a common "get or insert" pattern.
        let final_template = cache.get(raw).map(Arc::clone).unwrap_or_else(|| {
            cache.put(raw.to_string(), Arc::clone(&arc_template));
            arc_template
        });

        Ok(final_template)
    }
}