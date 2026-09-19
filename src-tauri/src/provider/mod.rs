//! provider/mod.rs — Abstract AI chat provider trait and shared types.
//!
//! Design decisions:
//! - Uses `Pin<Box<dyn Future>>` instead of `async fn` to make the trait object-safe
//!   (required for `Box<dyn ChatProvider>` without the `async-trait` crate).
//! - Uses `&mut (dyn FnMut(String, bool) + Send)` for streaming callbacks
//!   instead of `impl FnMut` for the same object-safety reason.
//! - Providers handle HTTP communication ONLY; model selection and fallback
//!   policy live in `policy.rs`.

pub mod ollama;
pub mod policy;
pub mod llama_cpp;
pub mod llama_server;

use std::pin::Pin;

/// A single message in a chat conversation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Generation options forwarded to the model.
#[derive(Debug, Clone)]
pub struct GenOptions {
    pub temperature: f32,
    pub num_predict: u32,
    pub repeat_penalty: f32,
    pub num_ctx: u32,
}

/// Full request payload sent to a ChatProvider.
/// `model` is already resolved (effective model after policy) before this struct is created.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub options: GenOptions,
    pub request_id: String,
}

/// Errors a provider can return.
#[derive(Debug)]
pub enum ProviderError {
    /// Network/connection failure. Includes timeout.
    ConnectionFailed(String),
    /// HTTP or server-side error (non-2xx).
    ServerError(String),
    /// JSON or response parsing error.
    ParseError(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::ConnectionFailed(s) => write!(f, "Kết nối thất bại: {}", s),
            ProviderError::ServerError(s)      => write!(f, "Lỗi server: {}", s),
            ProviderError::ParseError(s)       => write!(f, "Lỗi parse: {}", s),
        }
    }
}

/// Object-safe async trait for AI chat providers.
///
/// # Object safety
/// All async methods return `Pin<Box<dyn Future + Send + 'a>>` to preserve
/// object safety when used as `Box<dyn ChatProvider>`.
/// `chat_stream` takes `&mut (dyn FnMut + Send)` for the same reason.
///
/// # Cancellation
/// Cancellation is NOT handled inside the provider. The caller (commands.rs)
/// aborts the `JoinHandle` which drops the spawned task, which drops the
/// in-flight `reqwest` response stream, implicitly cancelling the HTTP request.
pub trait ChatProvider: Send + Sync {
    fn provider_name(&self) -> &str;

    /// List available model names.
    /// Uses a short timeout (2 s) suitable for status checks.
    fn list_models<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, ProviderError>> + Send + 'a>>;

    /// Non-streaming chat: returns full response as a single String.
    fn chat<'a>(
        &'a self,
        request: ChatRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, ProviderError>> + Send + 'a>>;

    /// Streaming chat: calls `on_chunk(text, is_done)` for each chunk.
    /// When `is_done = true`, the provider has finished and returns `Ok(())`.
    /// Callers must NOT assume `on_chunk` is called after `is_done = true`.
    fn chat_stream<'a>(
        &'a self,
        request: ChatRequest,
        on_chunk: &'a mut (dyn FnMut(String, bool) + Send),
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProviderError>> + Send + 'a>>;

    /// Hint to cancel an in-flight request by request_id.
    fn cancel_request(&self, request_id: &str);
}

pub struct ProviderRegistry {
    ollama: std::sync::Arc<dyn ChatProvider>,
    llama_cpp: std::sync::Arc<dyn ChatProvider>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            ollama: std::sync::Arc::new(ollama::OllamaProvider::new_auto()),
            llama_cpp: std::sync::Arc::new(llama_cpp::LlamaCppProvider::new()),
        }
    }

    /// Resolves and returns the requested provider.
    pub async fn get(&self, provider_name: &str) -> Result<std::sync::Arc<dyn ChatProvider>, ProviderError> {
        match provider_name {
            "ollama" => Ok(self.ollama.clone()),
            "llama.cpp" => Ok(self.llama_cpp.clone()),
            _ => Err(ProviderError::ConnectionFailed(format!("Provider '{}' không được hỗ trợ", provider_name))),
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
