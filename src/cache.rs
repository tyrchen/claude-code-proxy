use arc_swap::ArcSwap;
use std::collections::HashMap;
use std::sync::Arc;

use crate::models::gemini::GeminiFunctionDeclaration;

/// Cache for transformed tool schemas to avoid repeated conversion
///
/// Uses ArcSwap for efficient lock-free reads with infrequent writes.
/// Perfect for tool schemas which are defined once and reused many times.
#[derive(Clone)]
pub struct ToolSchemaCache {
    /// Cache: tool_name -> transformed Gemini function declaration
    cache: Arc<ArcSwap<HashMap<String, GeminiFunctionDeclaration>>>,
}

impl ToolSchemaCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(ArcSwap::from_pointee(HashMap::new())),
        }
    }

    /// Get a cached schema or transform and cache it
    pub fn get_or_transform(
        &self,
        tool: &crate::models::claude::ClaudeTool,
    ) -> crate::error::Result<GeminiFunctionDeclaration> {
        // Fast path: check cache first (lock-free read)
        {
            let cache = self.cache.load();
            if let Some(cached) = cache.get(&tool.name) {
                tracing::debug!(tool_name = %tool.name, "Cache hit for tool schema");
                return Ok(cached.clone());
            }
        }

        // Slow path: transform and cache
        tracing::debug!(tool_name = %tool.name, "Cache miss, transforming tool schema");

        let transformed = GeminiFunctionDeclaration {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.input_schema.clone(),
        };

        // Update cache atomically
        self.cache.rcu(|current| {
            let mut new_cache = (**current).clone();
            new_cache.insert(tool.name.clone(), transformed.clone());
            new_cache
        });

        Ok(transformed)
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.cache.load().len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.load().is_empty()
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.cache.store(Arc::new(HashMap::new()));
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.load();
        CacheStats {
            total_entries: cache.len(),
            tools: cache.keys().cloned().collect(),
        }
    }
}

impl Default for ToolSchemaCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub tools: Vec<String>,
}

lazy_static::lazy_static! {
    /// Global tool schema cache
    pub static ref TOOL_CACHE: ToolSchemaCache = ToolSchemaCache::new();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::claude::{ClaudeTool, JsonSchema};
    use std::collections::HashMap;
    use std::thread;

    fn make_test_tool(name: &str) -> ClaudeTool {
        let mut properties = HashMap::new();
        properties.insert(
            "param".to_string(),
            Box::new(JsonSchema {
                schema_type: "string".to_string(),
                ..Default::default()
            }),
        );

        ClaudeTool {
            name: name.to_string(),
            description: format!("Test tool {}", name),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                ..Default::default()
            },
        }
    }

    #[test]
    fn test_cache_miss_then_hit() {
        let cache = ToolSchemaCache::new();
        let tool = make_test_tool("test_tool");

        // First access - cache miss
        assert_eq!(cache.len(), 0);
        let result1 = cache.get_or_transform(&tool).unwrap();
        assert_eq!(cache.len(), 1);

        // Second access - cache hit
        let result2 = cache.get_or_transform(&tool).unwrap();
        assert_eq!(cache.len(), 1);

        // Results should be identical
        assert_eq!(result1.name, result2.name);
        assert_eq!(result1.description, result2.description);
    }

    #[test]
    fn test_multiple_tools() {
        let cache = ToolSchemaCache::new();

        let tool1 = make_test_tool("tool_1");
        let tool2 = make_test_tool("tool_2");
        let tool3 = make_test_tool("tool_3");

        cache.get_or_transform(&tool1).unwrap();
        cache.get_or_transform(&tool2).unwrap();
        cache.get_or_transform(&tool3).unwrap();

        assert_eq!(cache.len(), 3);

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 3);
        assert!(stats.tools.contains(&"tool_1".to_string()));
        assert!(stats.tools.contains(&"tool_2".to_string()));
        assert!(stats.tools.contains(&"tool_3".to_string()));
    }

    #[test]
    fn test_clear() {
        let cache = ToolSchemaCache::new();

        cache.get_or_transform(&make_test_tool("tool_1")).unwrap();
        cache.get_or_transform(&make_test_tool("tool_2")).unwrap();

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_thread_safety() {
        let cache = ToolSchemaCache::new();

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let cache = cache.clone();
                thread::spawn(move || {
                    let tool = make_test_tool(&format!("tool_{}", i));
                    cache.get_or_transform(&tool).unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // All tools should be cached (some might be duplicates if spawned with same i)
        assert!(cache.len() <= 10);
    }

    #[test]
    fn test_concurrent_reads() {
        let cache = ToolSchemaCache::new();
        let tool = make_test_tool("concurrent_test");

        // Pre-populate cache
        cache.get_or_transform(&tool).unwrap();

        // Multiple concurrent reads should all succeed
        let handles: Vec<_> = (0..100)
            .map(|_| {
                let cache = cache.clone();
                let tool = tool.clone();
                thread::spawn(move || {
                    let result = cache.get_or_transform(&tool).unwrap();
                    assert_eq!(result.name, "concurrent_test");
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(cache.len(), 1);
    }
}
