//! provider/llama_cpp.rs — llama.cpp HTTP provider.
//!
//! Responsibilities:
//! - HTTP communication with local llama-server.exe.
//! - SSE streaming parser tailored for llama.cpp format.
//! - Mapping logical model name to the internal absolute GGUF path.

use super::{ChatMessage, ChatProvider, ChatRequest, ProviderError};
use std::pin::Pin;

// ---------------------------------------------------------------------------
// Private API shapes
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone)]
struct LlamaCppRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    n_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
    // Note: num_ctx is NOT sent in JSON request. 
    // It is applied globally when launching llama-server.exe using `-c`.
}

// ---------------------------------------------------------------------------
// LlamaCppProvider
// ---------------------------------------------------------------------------

struct LlamaState {
    #[allow(dead_code)]
    manager: Option<std::sync::Arc<crate::provider::llama_server::LlamaServerManager>>,
    model: String,
    num_ctx: u32,
    base_url: String,
}

pub struct LlamaCppProvider {
    state: tokio::sync::Mutex<Option<LlamaState>>,
    client: reqwest::Client,
}

impl LlamaCppProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to build reqwest client for LlamaCppProvider");

        Self {
            state: tokio::sync::Mutex::new(None),
            client,
        }
    }

    #[cfg(test)]
    pub fn new_for_test(base_url: String, logical_model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap();
        Self {
            state: tokio::sync::Mutex::new(Some(LlamaState {
                manager: None,
                model: logical_model,
                num_ctx: 4096,
                base_url,
            })),
            client,
        }
    }

    async fn ensure_server(&self, logical_model: &str, num_ctx: u32) -> Result<String, ProviderError> {
        let mut state = self.state.lock().await;
        if let Some(st) = &*state {
            // In test mode (manager is None), just return the injected url.
            if st.model == logical_model && (st.num_ctx == num_ctx || st.manager.is_none()) {
                return Ok(st.base_url.clone());
            } else {
                log::info!("LlamaCpp config changed (model or num_ctx). Restarting sidecar...");
                *state = None;
            }
        }

        log::info!("Initializing llama.cpp sidecar for model: {}", logical_model);
        
        let exe_path = resolve_exe_path().map_err(|e| ProviderError::ConnectionFailed(e))?;
        let gguf_path = resolve_gguf_path(logical_model).map_err(|e| ProviderError::ConnectionFailed(e))?;
        
        let manager = crate::provider::llama_server::LlamaServerManager::start(&exe_path, &gguf_path, num_ctx).await?;
        let base_url = format!("http://127.0.0.1:{}", manager.port);
        let manager_arc = std::sync::Arc::new(manager);
        
        *state = Some(LlamaState {
            manager: Some(manager_arc),
            model: logical_model.to_string(),
            num_ctx,
            base_url: base_url.clone(),
        });
        
        Ok(base_url)
    }
}

impl ChatProvider for LlamaCppProvider {
    fn provider_name(&self) -> &str {
        "llama.cpp"
    }

    fn list_models<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, ProviderError>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut available = Vec::new();
            if resolve_gguf_path("qwen2.5:1.5b").is_ok() {
                available.push("qwen2.5:1.5b".to_string());
            }
            if resolve_gguf_path("qwen2.5:3b").is_ok() {
                available.push("qwen2.5:3b".to_string());
            }
            Ok(available)
        })
    }

    fn chat<'a>(
        &'a self,
        request: ChatRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, ProviderError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = self.ensure_server(&request.model, request.options.num_ctx).await?;
            let gguf_path = resolve_gguf_path(&request.model).unwrap_or_default();
            let body = LlamaCppRequest {
                model:    gguf_path,
                messages: request.messages.clone(),
                stream:   false,
                temperature:    Some(request.options.temperature),
                n_predict:      Some(request.options.num_predict),
                repeat_penalty: Some(request.options.repeat_penalty),
            };

            let res = self.client
                .post(format!("{}/v1/chat/completions", url))
                .json(&body)
                .send()
                .await
                .map_err(|e| ProviderError::ConnectionFailed(
                    format!("Loi goi llama.cpp chat: {}", e)
                ))?;

            if !res.status().is_success() {
                return Err(ProviderError::ServerError(
                    format!("HTTP {}", res.status())
                ));
            }

            let json: serde_json::Value = res.json().await
                .map_err(|e| ProviderError::ParseError(
                    format!("Loi parse response: {}", e)
                ))?;

            let content = json
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|c| c.get(0))
                .and_then(|choice| choice.get("message"))
                .and_then(|msg| msg.get("content"))
                .and_then(|s| s.as_str())
                .ok_or_else(|| ProviderError::ParseError(
                    "Response thieu choices[0].message.content".into()
                ))?;

            Ok(content.to_string())
        })
    }

    fn chat_stream<'a>(
        &'a self,
        request: ChatRequest,
        on_chunk: &'a mut (dyn FnMut(String, bool) + Send),
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProviderError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = self.ensure_server(&request.model, request.options.num_ctx).await?;
            let gguf_path = resolve_gguf_path(&request.model).unwrap_or_default();
            
            let body = LlamaCppRequest {
                model:    gguf_path,
                messages: request.messages.clone(),
                stream:   true,
                temperature:    Some(request.options.temperature),
                n_predict:      Some(request.options.num_predict),
                repeat_penalty: Some(request.options.repeat_penalty),
            };

            let mut res = self.client
                .post(format!("{}/v1/chat/completions", url))
                .json(&body)
                .send()
                .await
                .map_err(|e| ProviderError::ConnectionFailed(
                    format!("Loi goi llama.cpp stream: {}", e)
                ))?;

            if !res.status().is_success() {
                let status = res.status();
                let body_text = res.text().await.unwrap_or_default();
                return Err(ProviderError::ServerError(
                    format!("HTTP {} — {}", status, body_text)
                ));
            }

            let mut buffer = Vec::new();
            let mut proper_finish_received = false;

            loop {
                match res.chunk().await {
                    Ok(Some(chunk)) => {
                        buffer.extend_from_slice(&chunk);

                        while let Some(idx) = buffer.iter().position(|&b| b == b'\n') {
                            let line_bytes = buffer.drain(..=idx).collect::<Vec<_>>();
                            
                            let line = String::from_utf8_lossy(&line_bytes);
                            let line = line.trim();

                            if line.is_empty() {
                                continue;
                            }

                            if line.starts_with("data: ") {
                                let data_str = line["data: ".len()..].trim();
                                
                                if data_str == "[DONE]" {
                                    continue;
                                }

                                let json: serde_json::Value = match serde_json::from_str(data_str) {
                                    Ok(j) => j,
                                    Err(e) => {
                                        return Err(ProviderError::ParseError(format!(
                                            "Loi parse JSON chunk SSE: {}", e
                                        )));
                                    }
                                };

                                let choice = json
                                    .get("choices")
                                    .and_then(|c| c.as_array())
                                    .and_then(|c| c.get(0));

                                if let Some(c) = choice {
                                    let content = c
                                        .get("delta")
                                        .and_then(|d| d.get("content"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();

                                    let finish_reason = c
                                        .get("finish_reason")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");

                                    let is_done = finish_reason == "stop" || finish_reason == "length";
                                    
                                    if is_done {
                                        proper_finish_received = true;
                                    }

                                    on_chunk(content, is_done);

                                    if is_done {
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        // Stream ended.
                        if !proper_finish_received {
                            return Err(ProviderError::ConnectionFailed(
                                "Stream ket thuc bat thuong (khong nhan duoc finish_reason)".into()
                            ));
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(ProviderError::ConnectionFailed(
                            format!("Loi doc stream: {}", e)
                        ));
                    }
                }
            }
        })
    }

    fn cancel_request(&self, request_id: &str) {
        log::debug!(
            "LlamaCppProvider::cancel_request({}) — handled by JoinHandle::abort()",
            request_id
        );
    }
}

pub fn resolve_exe_path() -> Result<String, String> {
    if let Ok(p) = std::env::var("LLAMA_SERVER_PATH") {
        return Ok(p);
    }
    
    // Prod: check next to executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let prod_path = parent.join("llama-server.exe");
            if prod_path.exists() {
                return Ok(prod_path.to_string_lossy().to_string());
            }
        }
    }
    
    // Dev: fallback
    let cwd = std::env::current_dir().unwrap_or_default();
    
    // Check current dir
    let dev_path_1 = cwd.join("llama-cpp").join("llama-server.exe");
    if dev_path_1.exists() {
        return Ok(dev_path_1.to_string_lossy().to_string());
    }

    // Check parent dir (Tauri `cargo run` sets CWD to src-tauri)
    let dev_path_2 = cwd.join("../llama-cpp").join("llama-server.exe");
    if dev_path_2.exists() {
        return Ok(dev_path_2.to_string_lossy().to_string());
    }

    Err(format!(
        "Khong tim thay llama-server.exe trong bien moi truong, cung thu muc chay, hoac thu muc dev {:?}",
        dev_path_2.canonicalize().unwrap_or(dev_path_2)
    ))
}

pub fn expected_blob_name(logical_model: &str) -> Option<&'static str> {
    match logical_model {
        "qwen2.5:1.5b" => Some("sha256-183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4"),
        "qwen2.5:3b"   => Some("sha256-5ee4f07cdb9beadbbb293e85803c569b01bd37ed059d2715faa7bb405f31caa6"),
        _ => None,
    }
}

pub fn resolve_gguf_path(logical_model: &str) -> Result<String, String> {
    if let Ok(p) = std::env::var("LLAMA_MODEL_PATH") {
        let path_lower = p.to_lowercase();
        if path_lower.contains("3b") && !path_lower.contains("1.5b") {
            if logical_model == "qwen2.5:3b" { return Ok(p); }
        } else if path_lower.contains("1.5b") && !path_lower.contains("3b") {
            if logical_model == "qwen2.5:1.5b" { return Ok(p); }
        } else {
            // Ambiguous path, default to assuming it's the 1.5b fallback
            if logical_model == "qwen2.5:1.5b" { return Ok(p); }
        }
    }
    
    // Fallback heuristic: Try to find Ollama's blobs on Windows
    let expected_sha = expected_blob_name(logical_model);

    if let Some(sha) = expected_sha {
        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let mut path = std::path::PathBuf::from(user_profile);
            path.push(".ollama");
            path.push("models");
            path.push("blobs");
            path.push(sha);
            
            if path.exists() {
                return Ok(path.to_string_lossy().to_string());
            }
        }
    }
    
    Err(format!("Không tìm thấy file GGUF cho model '{}'. Vui lòng set LLAMA_MODEL_PATH.", logical_model))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ChatRequest, GenOptions};
    use mockito::Server;
    use serde_json::json;

    fn make_request() -> ChatRequest {
        ChatRequest {
            model: "logical_qwen".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "test".into() }],
            options: GenOptions {
                temperature: 0.7,
                num_predict: 128,
                repeat_penalty: 1.1,
                num_ctx: 4096,
            },
            request_id: "test-req".into(),
        }
    }

    #[test]
    fn test_provider_is_object_safe() {
        let p = LlamaCppProvider::new();
        let _dyn_p: Box<dyn ChatProvider> = Box::new(p);
    }

    #[test]
    fn test_list_models_returns_empty_or_populated() {
        tauri::async_runtime::block_on(async {
            let provider = LlamaCppProvider::new();
            let models = provider.list_models().await.unwrap();
            
            // Depending on the local filesystem, models could be empty or have elements.
            // We just ensure it doesn't crash or return an error.
            if !models.is_empty() {
                assert!(models.contains(&"qwen2.5:1.5b".to_string()) || models.contains(&"qwen2.5:3b".to_string()));
            }
        })
    }

    #[test]
    fn test_chat_success() {
        tauri::async_runtime::block_on(async {
            let mut server = Server::new_async().await;
            
            let m = server.mock("POST", "/v1/chat/completions")
                .match_body(mockito::Matcher::Json(json!({
                    "model": "",
                    "messages": [{"role": "user", "content": "test"}],
                    "stream": false,
                    "temperature": 0.7,
                    "n_predict": 128,
                    "repeat_penalty": 1.1
                })))
                .with_status(200)
                .with_body(json!({
                    "choices": [{
                        "message": { "content": "response text" }
                    }]
                }).to_string())
                .create_async().await;

            let provider = LlamaCppProvider::new_for_test(server.url(), "logical_qwen".into());
            let res = provider.chat(make_request()).await.unwrap();
            assert_eq!(res, "response text");
            m.assert_async().await;
        })
    }

    #[test]
    fn test_chat_http_error() {
        tauri::async_runtime::block_on(async {
            let mut server = Server::new_async().await;
            let _m = server.mock("POST", "/v1/chat/completions")
                .with_status(500)
                .create_async().await;

            let provider = LlamaCppProvider::new_for_test(server.url(), "logical_qwen".into());
            let err = provider.chat(make_request()).await.unwrap_err();
            assert!(matches!(err, ProviderError::ServerError(_)));
        })
    }

    #[test]
    fn test_chat_stream_success_and_chunk_split() {
        tauri::async_runtime::block_on(async {
            let mut server = Server::new_async().await;
            let stream_body = "data: {\"choices\":[{\"delta\":{\"content\":\"Ch\\u00e0o\"},\"finish_reason\":null}]}\n\n\
                               data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n";

            let m = server.mock("POST", "/v1/chat/completions")
                .with_status(200)
                .with_body(stream_body)
                .create_async().await;

            let provider = LlamaCppProvider::new_for_test(server.url(), "logical_qwen".into());
            
            let mut parts = Vec::new();
            let mut is_done = false;

            let mut cb = |text: String, done: bool| {
                parts.push(text);
                is_done = done;
            };

            provider.chat_stream(make_request(), &mut cb).await.unwrap();
            
            assert_eq!(parts.join(""), "Chào");
            assert!(is_done);
            m.assert_async().await;
        })
    }

    #[test]
    fn test_chat_stream_eof_before_finish() {
        tauri::async_runtime::block_on(async {
            let mut server = Server::new_async().await;
            let stream_body = "data: {\"choices\":[{\"delta\":{\"content\":\"incomplete\"},\"finish_reason\":null}]}\n\n";

            let m = server.mock("POST", "/v1/chat/completions")
                .with_status(200)
                .with_body(stream_body)
                .create_async().await;

            let provider = LlamaCppProvider::new_for_test(server.url(), "logical_qwen".into());
            let mut cb = |_: String, _: bool| {};
            let err = provider.chat_stream(make_request(), &mut cb).await.unwrap_err();
            
            assert!(matches!(err, ProviderError::ConnectionFailed(_)));
            if let ProviderError::ConnectionFailed(msg) = err {
                assert!(msg.contains("bat thuong"));
            }
            m.assert_async().await;
        })
    }

    #[test]
    fn test_chat_stream_json_error() {
        tauri::async_runtime::block_on(async {
            let mut server = Server::new_async().await;
            let stream_body = "data: {\"invalid_json\n\n";

            let _m = server.mock("POST", "/v1/chat/completions")
                .with_status(200)
                .with_body(stream_body)
                .create_async().await;

            let provider = LlamaCppProvider::new_for_test(server.url(), "logical_qwen".into());
            let mut cb = |_: String, _: bool| {};
            let err = provider.chat_stream(make_request(), &mut cb).await.unwrap_err();
            assert!(matches!(err, ProviderError::ParseError(_)));
        })
    }

    #[test]
    fn test_expected_blob_name() {
        assert_eq!(
            expected_blob_name("qwen2.5:1.5b"), 
            Some("sha256-183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4")
        );
        assert_eq!(
            expected_blob_name("qwen2.5:3b"), 
            Some("sha256-5ee4f07cdb9beadbbb293e85803c569b01bd37ed059d2715faa7bb405f31caa6")
        );
        assert_eq!(expected_blob_name("unknown_model"), None);
    }
}
