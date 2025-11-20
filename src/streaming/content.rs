use serde_json::json;

/// Type of content block in a Claude message
#[derive(Debug, Clone, PartialEq)]
pub enum ContentBlockType {
    Text,
    ToolUse,
}

/// Represents a single content block with index tracking
#[derive(Debug, Clone)]
pub struct ContentBlock {
    /// Zero-based index of this block in the response
    pub index: usize,
    /// Type of content block
    pub block_type: ContentBlockType,
    /// The actual content as JSON value
    pub content: serde_json::Value,
    /// Whether this block is complete
    pub is_complete: bool,
}

impl ContentBlock {
    pub fn new_text(index: usize) -> Self {
        Self {
            index,
            block_type: ContentBlockType::Text,
            content: json!({"type": "text", "text": ""}),
            is_complete: false,
        }
    }

    pub fn new_tool_use(index: usize, tool_use_id: String, name: String) -> Self {
        Self {
            index,
            block_type: ContentBlockType::ToolUse,
            content: json!({
                "type": "tool_use",
                "id": tool_use_id,
                "name": name,
                "input": {}
            }),
            is_complete: false,
        }
    }

    /// Append text to a text block
    pub fn append_text(&mut self, text: &str) {
        if let Some(existing_text) = self.content.get_mut("text")
            && let Some(s) = existing_text.as_str()
        {
            *existing_text = json!(format!("{}{}", s, text));
        }
    }

    /// Set tool input
    pub fn set_tool_input(&mut self, input: serde_json::Value) {
        if let Some(input_field) = self.content.get_mut("input") {
            *input_field = input;
        }
    }

    /// Mark as complete
    pub fn complete(&mut self) {
        self.is_complete = true;
    }
}

/// Manages multiple content blocks with index tracking
/// Mirrors Claude's native multi-block content structure
#[derive(Debug)]
pub struct ContentBlockManager {
    blocks: Vec<ContentBlock>,
    current_index: usize,
}

impl ContentBlockManager {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            current_index: 0,
        }
    }

    /// Start a new text block
    pub fn start_text_block(&mut self) -> usize {
        let index = self.current_index;
        self.blocks.push(ContentBlock::new_text(index));
        self.current_index += 1;
        index
    }

    /// Start a new tool use block
    pub fn start_tool_use_block(&mut self, tool_use_id: String, name: String) -> usize {
        let index = self.current_index;
        self.blocks
            .push(ContentBlock::new_tool_use(index, tool_use_id, name));
        self.current_index += 1;
        index
    }

    /// Get mutable reference to block by index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut ContentBlock> {
        self.blocks.iter_mut().find(|b| b.index == index)
    }

    /// Get reference to block by index
    pub fn get(&self, index: usize) -> Option<&ContentBlock> {
        self.blocks.iter().find(|b| b.index == index)
    }

    /// Get the current (latest) block
    pub fn current_block_mut(&mut self) -> Option<&mut ContentBlock> {
        self.blocks.last_mut()
    }

    /// Get all blocks
    pub fn blocks(&self) -> &[ContentBlock] {
        &self.blocks
    }

    /// Get count of blocks
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Check if a text block exists and has content
    pub fn has_text_content(&self) -> bool {
        self.blocks.iter().any(|b| {
            b.block_type == ContentBlockType::Text
                && b.content
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
        })
    }

    /// Check if has any tool use blocks
    pub fn has_tool_use(&self) -> bool {
        self.blocks
            .iter()
            .any(|b| b.block_type == ContentBlockType::ToolUse)
    }

    /// Reset for new message
    pub fn reset(&mut self) {
        self.blocks.clear();
        self.current_index = 0;
    }
}

impl Default for ContentBlockManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_text_block() {
        let mut manager = ContentBlockManager::new();
        let idx = manager.start_text_block();
        assert_eq!(idx, 0);

        manager.get_mut(idx).unwrap().append_text("Hello");
        manager.get_mut(idx).unwrap().append_text(" World");

        assert_eq!(manager.len(), 1);
        assert!(manager.has_text_content());
        assert!(!manager.has_tool_use());
    }

    #[test]
    fn test_mixed_blocks() {
        let mut manager = ContentBlockManager::new();

        // Add text block
        let text_idx = manager.start_text_block();
        manager.get_mut(text_idx).unwrap().append_text("Question");

        // Add tool use block
        let tool_idx =
            manager.start_tool_use_block("toolu_123".to_string(), "TodoWrite".to_string());
        manager
            .get_mut(tool_idx)
            .unwrap()
            .set_tool_input(json!({"todos": []}));

        assert_eq!(manager.len(), 2);
        assert!(manager.has_text_content());
        assert!(manager.has_tool_use());
        assert_eq!(text_idx, 0);
        assert_eq!(tool_idx, 1);
    }

    #[test]
    fn test_block_indexing() {
        let mut manager = ContentBlockManager::new();

        manager.start_text_block();
        manager.start_tool_use_block("toolu_1".to_string(), "Tool1".to_string());
        manager.start_text_block();
        manager.start_tool_use_block("toolu_2".to_string(), "Tool2".to_string());

        assert_eq!(manager.len(), 4);

        // Verify indices are sequential
        for i in 0..4 {
            assert!(manager.get(i).is_some());
            assert_eq!(manager.get(i).unwrap().index, i);
        }
    }
}
