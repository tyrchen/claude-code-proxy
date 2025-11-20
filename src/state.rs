use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Tool call metadata stored in state with enhanced context tracking
#[derive(Clone, Debug)]
pub struct ToolCallMetadata {
    pub function_name: String,
    pub thought_signature: Option<String>,
    pub args: serde_json::Value,
    pub timestamp: Instant,
    /// Request sequence number for this tool call (for debugging multi-turn)
    pub request_index: usize,
    /// Conversation/session ID (currently using a placeholder, can be enhanced)
    pub conversation_id: String,
    /// Original tool_use_id for round-trip verification
    pub original_id: String,
}

/// Conversation state for tracking tool calls across multi-turn interactions
///
/// This maintains the mapping between Claude's tool_use_id and Gemini's function metadata,
/// which is necessary because:
/// - Claude generates unique IDs for each tool call
/// - When Claude sends tool results, it references these IDs
/// - Gemini needs function names AND thought signatures for function responses
///
/// Enhanced with conversation context tracking for better debugging and round-trip verification.
///
/// Thread-safe and designed for concurrent access across async tasks.
#[derive(Clone)]
pub struct ConversationState {
    /// Maps tool_use_id -> ToolCallMetadata
    tool_mappings: Arc<DashMap<String, ToolCallMetadata>>,

    /// How long to keep mappings before cleanup (default: 1 hour)
    retention_duration: Duration,

    /// Request counter for tracking conversation flow
    request_counter: Arc<AtomicUsize>,
}

impl ConversationState {
    /// Create a new conversation state with default retention (1 hour)
    pub fn new() -> Self {
        Self {
            tool_mappings: Arc::new(DashMap::new()),
            retention_duration: Duration::from_secs(3600),
            request_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Create with custom retention duration
    pub fn with_retention(retention_duration: Duration) -> Self {
        Self {
            tool_mappings: Arc::new(DashMap::new()),
            retention_duration,
            request_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Increment and get the next request index
    pub fn next_request_index(&self) -> usize {
        self.request_counter.fetch_add(1, Ordering::SeqCst)
    }

    /// Get current request count
    pub fn current_request_count(&self) -> usize {
        self.request_counter.load(Ordering::SeqCst)
    }

    /// Register a tool use mapping when transforming Gemini function call to Claude tool_use
    ///
    /// This should be called when:
    /// - Gemini responds with a function call
    /// - We transform it to a Claude tool_use block
    /// - We need to remember the mapping (including thought signature and args) for when it comes back in history
    ///
    /// Enhanced version with conversation context tracking
    pub fn register_tool_use_with_context(
        &self,
        tool_use_id: String,
        function_name: String,
        thought_signature: Option<String>,
        args: serde_json::Value,
        conversation_id: Option<String>,
    ) {
        let request_index = self.next_request_index();
        let conv_id = conversation_id.unwrap_or_else(|| "default".to_string());
        let original_id = tool_use_id.clone();

        tracing::debug!(
            tool_use_id = %tool_use_id,
            function_name = %function_name,
            request_index = request_index,
            conversation_id = %conv_id,
            has_signature = thought_signature.is_some(),
            "Registering tool use mapping with context"
        );

        self.tool_mappings.insert(
            tool_use_id,
            ToolCallMetadata {
                function_name,
                thought_signature,
                args,
                timestamp: Instant::now(),
                request_index,
                conversation_id: conv_id,
                original_id,
            },
        );
    }

    /// Legacy method for backward compatibility
    pub fn register_tool_use(
        &self,
        tool_use_id: String,
        function_name: String,
        thought_signature: Option<String>,
        args: serde_json::Value,
    ) {
        self.register_tool_use_with_context(
            tool_use_id,
            function_name,
            thought_signature,
            args,
            None,
        );
    }

    /// Retrieve function name for a given tool_use_id
    ///
    /// This should be called when:
    /// - Claude sends a tool_result block
    /// - We need to transform it to a Gemini function response
    /// - We need to look up the original function name
    pub fn get_function_name(&self, tool_use_id: &str) -> Option<String> {
        self.tool_mappings
            .get(tool_use_id)
            .map(|entry| entry.value().function_name.clone())
    }

    /// Retrieve complete metadata (name + thought signature) for a given tool_use_id
    pub fn get_metadata(&self, tool_use_id: &str) -> Option<ToolCallMetadata> {
        self.tool_mappings
            .get(tool_use_id)
            .map(|entry| entry.value().clone())
    }

    /// Get the number of tracked tool calls
    pub fn len(&self) -> usize {
        self.tool_mappings.len()
    }

    /// Check if state is empty
    pub fn is_empty(&self) -> bool {
        self.tool_mappings.is_empty()
    }

    /// Clean up old mappings that exceed retention duration
    ///
    /// This should be called periodically to prevent unbounded memory growth.
    /// Returns the number of entries removed.
    pub fn cleanup_old_entries(&self) -> usize {
        let now = Instant::now();
        let retention = self.retention_duration;

        let to_remove: Vec<String> = self
            .tool_mappings
            .iter()
            .filter_map(|entry| {
                let (id, metadata) = (entry.key(), entry.value());
                if now.duration_since(metadata.timestamp) > retention {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            self.tool_mappings.remove(&id);
        }

        if count > 0 {
            tracing::info!(
                removed = count,
                total_remaining = self.len(),
                "Cleaned up old tool mappings"
            );
        }

        count
    }

    /// Clear all mappings (useful for testing)
    pub fn clear(&self) {
        self.tool_mappings.clear();
        self.request_counter.store(0, Ordering::SeqCst);
    }

    /// Get all tool calls for a specific conversation (for debugging)
    pub fn get_by_conversation(&self, conversation_id: &str) -> Vec<(String, ToolCallMetadata)> {
        self.tool_mappings
            .iter()
            .filter(|entry| entry.value().conversation_id == conversation_id)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Get tool calls sorted by request index (for debugging conversation flow)
    pub fn get_sorted_by_request_index(&self) -> Vec<(String, ToolCallMetadata)> {
        let mut entries: Vec<_> = self
            .tool_mappings
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        entries.sort_by_key(|(_, metadata)| metadata.request_index);
        entries
    }

    /// Verify round-trip: check if tool_use_id matches original_id
    pub fn verify_round_trip(&self, tool_use_id: &str) -> bool {
        self.tool_mappings
            .get(tool_use_id)
            .map(|entry| entry.value().original_id == tool_use_id)
            .unwrap_or(false)
    }
}

impl Default for ConversationState {
    fn default() -> Self {
        Self::new()
    }
}

lazy_static::lazy_static! {
    /// Global singleton state for development and simple deployments
    ///
    /// For production with multiple users, consider using per-session state
    /// by extracting session IDs from request headers or implementing
    /// a session manager.
    pub static ref GLOBAL_STATE: ConversationState = ConversationState::new();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_register_and_retrieve() {
        let state = ConversationState::new();

        state.register_tool_use(
            "toolu_123".to_string(),
            "get_weather".to_string(),
            None,
            serde_json::json!({}),
        );

        let result = state.get_function_name("toolu_123");
        assert_eq!(result, Some("get_weather".to_string()));
    }

    #[test]
    fn test_missing_mapping() {
        let state = ConversationState::new();

        let result = state.get_function_name("toolu_nonexistent");
        assert_eq!(result, None);
    }

    #[test]
    fn test_multiple_mappings() {
        let state = ConversationState::new();

        state.register_tool_use(
            "toolu_1".to_string(),
            "func_a".to_string(),
            None,
            serde_json::json!({}),
        );
        state.register_tool_use(
            "toolu_2".to_string(),
            "func_b".to_string(),
            None,
            serde_json::json!({}),
        );
        state.register_tool_use(
            "toolu_3".to_string(),
            "func_c".to_string(),
            None,
            serde_json::json!({}),
        );

        assert_eq!(state.len(), 3);
        assert_eq!(
            state.get_function_name("toolu_1"),
            Some("func_a".to_string())
        );
        assert_eq!(
            state.get_function_name("toolu_2"),
            Some("func_b".to_string())
        );
        assert_eq!(
            state.get_function_name("toolu_3"),
            Some("func_c".to_string())
        );
    }

    #[test]
    fn test_thread_safety() {
        let state = ConversationState::new();
        let state_clone = state.clone();

        // Register from main thread
        state.register_tool_use(
            "toolu_main".to_string(),
            "main_func".to_string(),
            None,
            serde_json::json!({}),
        );

        // Register from another thread
        let handle = thread::spawn(move || {
            state_clone.register_tool_use(
                "toolu_thread".to_string(),
                "thread_func".to_string(),
                None,
                serde_json::json!({}),
            );
        });

        handle.join().unwrap();

        // Both should be accessible
        assert_eq!(
            state.get_function_name("toolu_main"),
            Some("main_func".to_string())
        );
        assert_eq!(
            state.get_function_name("toolu_thread"),
            Some("thread_func".to_string())
        );
        assert_eq!(state.len(), 2);
    }

    #[test]
    fn test_cleanup_old_entries() {
        let state = ConversationState::with_retention(Duration::from_millis(100));

        state.register_tool_use(
            "toolu_old".to_string(),
            "old_func".to_string(),
            None,
            serde_json::json!({}),
        );
        state.register_tool_use(
            "toolu_new".to_string(),
            "new_func".to_string(),
            None,
            serde_json::json!({}),
        );

        // Wait for first entry to expire
        thread::sleep(Duration::from_millis(150));

        // Add another new entry
        state.register_tool_use(
            "toolu_newer".to_string(),
            "newer_func".to_string(),
            None,
            serde_json::json!({}),
        );

        // Cleanup should remove the old entry
        let removed = state.cleanup_old_entries();

        // At least the old one should be removed
        assert!(removed >= 1);
        assert!(state.get_function_name("toolu_old").is_none());

        // New entries should still exist
        assert!(state.get_function_name("toolu_newer").is_some());
    }

    #[test]
    fn test_clear() {
        let state = ConversationState::new();

        state.register_tool_use(
            "toolu_1".to_string(),
            "func_1".to_string(),
            None,
            serde_json::json!({}),
        );
        state.register_tool_use(
            "toolu_2".to_string(),
            "func_2".to_string(),
            None,
            serde_json::json!({}),
        );

        assert_eq!(state.len(), 2);

        state.clear();

        assert_eq!(state.len(), 0);
        assert!(state.is_empty());
    }

    #[test]
    fn test_overwrite_mapping() {
        let state = ConversationState::new();

        state.register_tool_use(
            "toolu_123".to_string(),
            "first_func".to_string(),
            None,
            serde_json::json!({}),
        );
        state.register_tool_use(
            "toolu_123".to_string(),
            "second_func".to_string(),
            None,
            serde_json::json!({}),
        );

        // Should have the second value
        assert_eq!(
            state.get_function_name("toolu_123"),
            Some("second_func".to_string())
        );
        assert_eq!(state.len(), 1);
    }
}
