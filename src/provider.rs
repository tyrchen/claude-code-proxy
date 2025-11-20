use bytes::Bytes;
use futures::Stream;
use std::future::Future;
use std::pin::Pin;

use crate::error::Result;

/// Type alias for the streaming response from a provider
pub type ProviderStream = Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>> + Send>>;

/// Type alias for the future returned by stream_generate_content
pub type StreamFuture = Pin<Box<dyn Future<Output = Result<ProviderStream>> + Send>>;

/// Trait for AI provider clients that support streaming content generation
pub trait Provider: Send + Sync {
    /// Stream generate content from the provider
    ///
    /// # Arguments
    /// * `model` - The model name to use
    /// * `body` - The request body as bytes
    ///
    /// # Returns
    /// A stream of bytes from the provider's response
    fn stream_generate_content(&self, model: &str, body: Bytes) -> StreamFuture;

    /// Whether this provider needs request transformation
    /// Returns true for providers like Gemini that need Claude->Gemini transformation
    /// Returns false for providers like Kimi that are Claude-compatible
    fn needs_transformation(&self) -> bool;

    /// Get the provider name for logging
    fn name(&self) -> &str;
}
