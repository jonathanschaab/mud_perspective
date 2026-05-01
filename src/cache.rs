use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use crate::engine::Template;

/// A thread-safe Least Recently Used (LRU) cache for compiled text templates.
///
/// Because MUDs process high volumes of text concurrently, this cache wraps the 
/// underlying ASTs in an `Arc`. This allows multiple network threads to safely 
/// read and render the same compiled template simultaneously without incurring 
/// cloning costs.
pub struct TemplateCache<'a> {
    // We use Arc<Template> so multiple threads can read the same compiled AST 
    // simultaneously without having to clone the underlying Vec of Tokens.
    inner: Mutex<LruCache<String, Arc<Template<'a>>>>,
}

impl<'a> TemplateCache<'a> {
    /// Initializes a new thread-safe template cache.
    ///
    /// # Arguments
    /// * `capacity` - The maximum number of templates to keep in memory. Once 
    ///   exceeded, the least recently used templates are automatically evicted.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(LruCache::new(NonZeroUsize::new(capacity).unwrap())),
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
    pub fn get_or_compile(&self, raw: &'a str) -> Result<Arc<Template<'a>>, String> {
        let mut cache = self.inner.lock().unwrap();

        // 1. Cache Hit: If the template is already compiled, return an Arc pointer to it.
        // The `lru` crate efficiently updates the access order in O(1) time.
        if let Some(template) = cache.get(raw) {
            return Ok(Arc::clone(template));
        }

        // 2. Cache Miss: Compile the raw string into an AST.
        let compiled_template = Template::compile(raw)?;
        let arc_template = Arc::new(compiled_template);

        // 3. Store it in the cache for future use.
        cache.put(raw.to_string(), Arc::clone(&arc_template));

        Ok(arc_template)
    }
}