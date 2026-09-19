//! provider/policy.rs — Model resolution policy (fallback logic).
//!
//! Providers only supply `list_models()`. This layer decides which model
//! is actually used, keeping fallback logic out of individual providers so
//! any future provider (e.g. LlamaCppProvider) inherits it for free.

use super::{ChatProvider, ProviderError};

/// Policy for selecting the effective model.
pub struct ModelPolicy {
    pub requested_model: String,
    pub fallback_model: String,
}

/// Result of model resolution.
pub struct ResolvedModel {
    pub effective_model: String,
    pub fallback: bool,
    pub reason: Option<String>,
}

impl ModelPolicy {
    pub fn new(
        requested_model: impl Into<String>,
        fallback_model: impl Into<String>,
    ) -> Self {
        Self {
            requested_model: requested_model.into(),
            fallback_model: fallback_model.into(),
        }
    }

    /// Resolve which model to use by asking the provider for its model list.
    ///
    /// - If `requested_model` is available → use it, `fallback = false`.
    /// - If only `fallback_model` is available → use it, `fallback = true`.
    /// - If neither is available → `Err(ProviderError::ConnectionFailed(...))`.
    ///
    /// Does NOT fallback on connection errors or server errors; those are
    /// propagated as-is so the caller can show a proper error to the user.
    pub async fn resolve(
        &self,
        provider: &dyn ChatProvider,
    ) -> Result<ResolvedModel, ProviderError> {
        let available = provider.list_models().await?;

        if available.iter().any(|m| m == &self.requested_model) {
            return Ok(ResolvedModel {
                effective_model: self.requested_model.clone(),
                fallback: false,
                reason: None,
            });
        }

        if available.iter().any(|m| m == &self.fallback_model) {
            return Ok(ResolvedModel {
                effective_model: self.fallback_model.clone(),
                fallback: true,
                reason: Some(format!(
                    "Model {} chưa được cài",
                    self.requested_model
                )),
            });
        }

        Err(ProviderError::ConnectionFailed(format!(
            "Model '{}' không tồn tại và fallback '{}' cũng không có.",
            self.requested_model, self.fallback_model
        )))
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ChatMessage, ChatRequest, GenOptions};
    use std::pin::Pin;

    struct MockProvider {
        available: Vec<String>,
    }

    impl ChatProvider for MockProvider {
        fn provider_name(&self) -> &str {
            "mock"
        }

        fn list_models<'a>(
            &'a self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, ProviderError>> + Send + 'a>>
        {
            let models = self.available.clone();
            Box::pin(async move { Ok(models) })
        }

        fn chat<'a>(
            &'a self,
            _request: ChatRequest,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<String, ProviderError>> + Send + 'a>>
        {
            Box::pin(async move { Ok("mock response".into()) })
        }

        fn chat_stream<'a>(
            &'a self,
            _request: ChatRequest,
            _on_chunk: &'a mut (dyn FnMut(String, bool) + Send),
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProviderError>> + Send + 'a>>
        {
            Box::pin(async move { Ok(()) })
        }

        fn cancel_request(&self, _request_id: &str) {}
    }

    fn make_request() -> ChatRequest {
        ChatRequest {
            model: "qwen2.5:1.5b".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
            options: GenOptions { temperature: 0.7, num_predict: 64, repeat_penalty: 1.1, num_ctx: 4096 },
            request_id: "test-req".into(),
        }
    }

    #[test]
    fn test_resolve_with_available_model() {
        let provider = MockProvider {
            available: vec!["qwen2.5:3b".into(), "qwen2.5:1.5b".into()],
        };
        let policy = ModelPolicy::new("qwen2.5:3b", "qwen2.5:1.5b");
        let resolved = tauri::async_runtime::block_on(policy.resolve(&provider)).unwrap();
        assert_eq!(resolved.effective_model, "qwen2.5:3b");
        assert!(!resolved.fallback);
        assert!(resolved.reason.is_none());
    }

    #[test]
    fn test_resolve_fallback_when_requested_unavailable() {
        let provider = MockProvider {
            available: vec!["qwen2.5:1.5b".into()],
        };
        let policy = ModelPolicy::new("qwen2.5:3b", "qwen2.5:1.5b");
        let resolved = tauri::async_runtime::block_on(policy.resolve(&provider)).unwrap();
        assert_eq!(resolved.effective_model, "qwen2.5:1.5b");
        assert!(resolved.fallback);
        assert!(resolved.reason.is_some());
    }

    #[test]
    fn test_resolve_error_both_unavailable() {
        let provider = MockProvider { available: vec![] };
        let policy = ModelPolicy::new("qwen2.5:3b", "qwen2.5:1.5b");
        let result = tauri::async_runtime::block_on(policy.resolve(&provider));
        assert!(result.is_err());
        if let Err(ProviderError::ConnectionFailed(msg)) = result {
            assert!(msg.contains("qwen2.5:3b"));
            assert!(msg.contains("qwen2.5:1.5b"));
        } else {
            panic!("Expected ConnectionFailed");
        }
    }

    // Ensure MockProvider itself satisfies the trait (compile-time check)
    #[test]
    fn test_mock_provider_satisfies_trait() {
        let _: &dyn ChatProvider = &MockProvider { available: vec![] };
        let _req = make_request();
    }
}
