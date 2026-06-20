//! Pricing dimensions: the axes along which usage is priced.

use serde::{Deserialize, Serialize};

/// A priceable dimension of an LLM/agent usage event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingDimension {
    /// Input tokens not served from cache.
    InputUncached,
    /// Input tokens served from a prompt cache.
    CacheRead,
    /// Tokens written to a prompt cache.
    CacheWrite,
    /// Output (completion) tokens.
    Output,
    /// Reasoning / thinking tokens.
    ReasoningOutput,
    /// A tool invocation.
    ToolCall,
    /// A web-search call.
    WebSearch,
}

/// The modality of the priced content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
    None,
}

/// A context-length tier (long-context tokens are often priced higher).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextTier {
    Standard,
    Long200k,
    Long1m,
}

/// The unit a component is priced in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Token,
    Char,
    Image,
    Second,
    Call,
}
