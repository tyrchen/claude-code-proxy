//! # Claude Code Proxy
//!
//! A high-performance protocol translation proxy that allows Claude Code CLI to use Google Gemini models.
//!
//! ## Overview
//!
//! This library provides the core functionality for translating between:
//! - **Claude Messages API** (Anthropic) - Request format
//! - **Gemini GenerateContent API** (Google) - Backend format
//!
//! The proxy handles:
//! - Request transformation and validation
//! - Real-time streaming response conversion
//! - SSE (Server-Sent Events) generation
//! - Model name mapping
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use claude_code_proxy::config::ProxyConfig;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load configuration for Gemini provider
//! let config = ProxyConfig::from_env("gemini")?;
//!
//! // Or load configuration for Kimi provider
//! let config = ProxyConfig::from_env("kimi")?;
//!
//! // See examples/simple_transform.rs for request transformation
//! // See examples/streaming_demo.rs for streaming SSE responses
//! # Ok(())
//! # }
//! ```
//!
//! ## Modules
//!
//! - [`config`] - Configuration loading and validation
//! - [`error`] - Error types and handling
//! - [`models`] - Data structures for Claude and Gemini APIs
//! - [`proxy`] - Pingora proxy implementation
//! - [`streaming`] - JSON parser and SSE event generator
//! - [`transform`] - Request/response transformation logic

pub mod cache;
pub mod client;
pub mod config;
pub mod error;
pub mod handler;
pub mod metrics;
pub mod models;
pub mod provider;
pub mod state;
pub mod streaming;
pub mod transform;
pub mod validation;

pub use config::ProxyConfig;
pub use error::{ProxyError, Result};
