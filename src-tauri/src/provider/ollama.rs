//! provider/ollama.rs — Ollama HTTP provider (self-contained).
//!
//! Responsibilities:
//! - HTTP communication with Ollama API.
//! - URL auto-discovery (tries OLLAMA_URL env var, then default ports).
//! - Caches the active URL internally (no shared state with AiTaskManager).
//!
//! NOT responsible for:
//! - Model selection or fallback (→ policy.rs).
//! - Allowlist validation (→ settings.rs + commands.rs).
//! - Event emission to the frontend (→ commands.rs).
//! - Task lifecycle / cancellation (→ AiTaskManager + JoinHandle::abort).

use super::{ChatMessage, ChatProvider, ChatRequest, ProviderError};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Private Ollama API shapes
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
}

#[derive(serde::Serialize, Clone)]
struct OllamaApiRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    options: OllamaOptions,
}

#[derive(serde::Deserialize)]
struct OllamaTagResponse {
    models: Vec<OllamaModelEntry>,
}

#[derive(serde::Deserialize)]
struct OllamaModelEntry {
    name: String,
}

// ---------------------------------------------------------------------------
// OllamaProvider
// ---------------------------------------------------------------------------

pub struct OllamaProvider {
    /// Ordered list of URLs to try during discovery (first match wins).
    candidate_urls: Vec<String>,
    /// Cached working URL (discovered lazily on first call).
    active_url: Arc<Mutex<Option<String>>>,
    /// Long-timeout client for actual AI requests.
    client: reqwest::Client,
}

impl OllamaProvider {
    /// Create a provider that auto-discovers Ollama's URL.
    ///
    /// Priority:
    /// 1. `OLLAMA_URL` environment variable (single URL).
    /// 2. Default ports: 11435 then 11434 (Ollama typical port).
    pub fn new_auto() -> Self {
        let candidate_urls = std::env::var("OLLAMA_URL")
            .map(|url| vec![url])
            .unwrap_or_else(|_| {
                vec![
                    "http://127.0.0.1:11435".to_string(),
                    "http://127.0.0.1:11434".to_string(),
                ]
            });

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to build reqwest client for OllamaProvider");

        Self {
            candidate_urls,
            active_url: Arc::new(Mutex::new(None)),
            client,
        }
    }

    /// Resolve the active Ollama URL.
    ///
    /// Returns cached URL on success; tries candidates sequentially if not yet cached.
    /// Returns `Err(ProviderError::ConnectionFailed)` if no candidate responds.
    async fn resolve_url(&self) -> Result<String, ProviderError> {
        // Fast path: already discovered.
        {
            let cached = self.active_url.lock().await;
            if let Some(url) = &*cached {
                return Ok(url.clone());
            }
        }

        // Discovery: short 2s timeout, try candidates in order.
        let discovery_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .map_err(|e| ProviderError::ConnectionFailed(
                format!("Failed to build discovery client: {}", e)
            ))?;

        for url in &self.candidate_urls {
            match discovery_client
                .get(format!("{}/api/tags", url))
                .send()
                .await
            {
                Ok(res) if res.status().is_success() => {
                    let mut cached = self.active_url.lock().await;
                    *cached = Some(url.clone());
                    log::info!("OllamaProvider: discovered active URL = {}", url);
                    return Ok(url.clone());
                }
                _ => continue,
            }
        }

        Err(ProviderError::ConnectionFailed(
            format!(
                "Không thể kết nối đến Ollama trên: {}",
                self.candidate_urls.join(", ")
            )
        ))
    }

    fn build_options(req: &ChatRequest) -> OllamaOptions {
        OllamaOptions {
            temperature:    Some(req.options.temperature),
            num_predict:    Some(req.options.num_predict),
            repeat_penalty: Some(req.options.repeat_penalty),
            num_ctx:        Some(req.options.num_ctx),
        }
    }
}

impl ChatProvider for OllamaProvider {
    fn provider_name(&self) -> &str {
        "ollama"
    }

    // -----------------------------------------------------------------------
    fn list_models<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, ProviderError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = self.resolve_url().await?;

            let res = self.client
                .get(format!("{}/api/tags", url))
                .timeout(std::time::Duration::from_secs(2))
                .send()
                .await
                .map_err(|e| ProviderError::ConnectionFailed(
                    format!("Không thể kết nối Ollama: {}", e)
                ))?;

            if !res.status().is_success() {
                return Err(ProviderError::ServerError(
                    format!("HTTP {}", res.status())
                ));
            }

            let tags: OllamaTagResponse = res.json().await
                .map_err(|e| ProviderError::ParseError(
                    format!("Không thể parse /api/tags: {}", e)
                ))?;

            Ok(tags.models.into_iter().map(|m| m.name).collect())
        })
    }

    // -----------------------------------------------------------------------
    fn chat<'a>(
        &'a self,
        request: ChatRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, ProviderError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = self.resolve_url().await?;

            let body = OllamaApiRequest {
                model:    request.model.clone(),
                messages: request.messages.clone(),
                stream:   false,
                options:  Self::build_options(&request),
            };

            let res = self.client
                .post(format!("{}/api/chat", url))
                .json(&body)
                .send()
                .await
                .map_err(|e| ProviderError::ConnectionFailed(
                    format!("Lỗi gọi Ollama /api/chat: {}", e)
                ))?;

            if !res.status().is_success() {
                return Err(ProviderError::ServerError(
                    format!("HTTP {}", res.status())
                ));
            }

            // Non-streaming: parse full response as JSON
            let json: serde_json::Value = res.json().await
                .map_err(|e| ProviderError::ParseError(
                    format!("Lỗi parse response chat: {}", e)
                ))?;

            json.get("message")
                .and_then(|m| m.get("content"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| ProviderError::ParseError(
                    "Response thiếu message.content".into()
                ))
        })
    }

    // -----------------------------------------------------------------------
    /// Stream response chunks.
    ///
    /// Calls `on_chunk(text, is_done)` for each token. When `is_done = true`,
    /// the function has already called `on_chunk` and returns `Ok(())`.
    ///
    /// # Error contract
    /// - Connection/HTTP failures → `Err(ProviderError::ConnectionFailed)`
    /// - Any JSON parse failure on a line → `Err(ProviderError::ParseError)`
    ///   (caller is responsible for emitting `ai-stream-error` to frontend)
    /// - Stream EOF without `done: true` → returns `Ok(())` with a warning log
    ///   (Ollama sometimes closes streams cleanly without a done packet)
    ///
    /// # Cancellation
    /// Not handled internally. The caller aborts the `JoinHandle` which drops
    /// this future and the underlying `reqwest` response stream.
    fn chat_stream<'a>(
        &'a self,
        request: ChatRequest,
        on_chunk: &'a mut (dyn FnMut(String, bool) + Send),
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProviderError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = self.resolve_url().await?;

            let body = OllamaApiRequest {
                model:    request.model.clone(),
                messages: request.messages.clone(),
                stream:   true,
                options:  Self::build_options(&request),
            };

            let mut res = self.client
                .post(format!("{}/api/chat", url))
                .json(&body)
                .send()
                .await
                .map_err(|e| ProviderError::ConnectionFailed(
                    format!("Lỗi gọi Ollama stream: {}", e)
                ))?;

            if !res.status().is_success() {
                let status = res.status();
                let body_text = res.text().await.unwrap_or_default();
                return Err(ProviderError::ServerError(
                    format!("HTTP {} — {}", status, body_text)
                ));
            }

            let mut buffer: Vec<u8> = Vec::new();

            loop {
                match res.chunk().await {
                    Ok(Some(chunk)) => {
                        buffer.extend_from_slice(&chunk);

                        // Process all complete lines (terminated by \n)
                        while let Some(idx) = buffer.iter().position(|&b| b == b'\n') {
                            let line_bytes = buffer.drain(..=idx).collect::<Vec<u8>>();
                            let trimmed = line_bytes.trim_ascii();
                            if trimmed.is_empty() {
                                continue;
                            }

                            let json: serde_json::Value =
                                serde_json::from_slice(trimmed).map_err(|e| {
                                    ProviderError::ParseError(format!(
                                        "Không parse được JSON chunk: {} (raw: {:?})",
                                        e,
                                        String::from_utf8_lossy(trimmed)
                                    ))
                                })?;

                            let content = json
                                .get("message")
                                .and_then(|m| m.get("content"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let is_done = json
                                .get("done")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            on_chunk(content, is_done);

                            if is_done {
                                return Ok(());
                            }
                        }
                    }

                    Ok(None) => {
                        // Stream ended without a `done: true` packet.
                        // Attempt to flush any remaining buffer bytes.
                        let trimmed = buffer.trim_ascii();
                        if !trimmed.is_empty() {
                            match serde_json::from_slice::<serde_json::Value>(trimmed) {
                                Ok(json) => {
                                    let content = json
                                        .get("message")
                                        .and_then(|m| m.get("content"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let is_done = json
                                        .get("done")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);
                                    on_chunk(content, is_done);
                                }
                                Err(e) => {
                                    return Err(ProviderError::ParseError(format!(
                                        "Lỗi parse buffer dư khi EOF: {}",
                                        e
                                    )));
                                }
                            }
                        } else {
                            // Empty buffer at EOF: Ollama ended stream cleanly.
                            log::warn!(
                                "OllamaProvider: stream ended without done=true packet \
                                 (request_id={}). Treating as success.",
                                request.request_id
                            );
                        }
                        return Ok(());
                    }

                    Err(e) => {
                        return Err(ProviderError::ConnectionFailed(
                            format!("Lỗi đọc dữ liệu mạng: {}", e)
                        ));
                    }
                }
            }
        })
    }

    // -----------------------------------------------------------------------
    /// Hint to cancel an in-flight request by request_id.
    ///
    /// OllamaProvider does not implement true per-request cancellation;
    /// primary cancellation is via `AiTaskManager` → `JoinHandle::abort()`,
    /// which drops this future and the underlying reqwest stream.
    ///
    /// This method exists to satisfy the ChatProvider contract and may be
    /// used for future provider implementations that support explicit cancellation.
    fn cancel_request(&self, request_id: &str) {
        log::debug!(
            "OllamaProvider::cancel_request({}) — cancellation handled by JoinHandle::abort()",
            request_id
        );
    }
}
