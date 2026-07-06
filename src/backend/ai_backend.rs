use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;
use thiserror::Error;

use crate::config::AiPanelConfig;
use crate::messages::ResponseMessage;

#[derive(Error, Debug)]
pub enum AiError {
    #[error("API error: {0}")]
    ApiError(String),

    #[allow(dead_code)]
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<AiChatMessage>,
    stream: bool,
    think: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct KimiChatRequest {
    model: String,
    messages: Vec<AiChatMessage>,
    stream: bool,
    max_completion_tokens: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct AiChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct OllamaOptions {
    num_predict: i32,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaResponseMessage,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct KimiChatResponse {
    choices: Vec<KimiChoice>,
}

#[derive(Deserialize)]
struct KimiChoice {
    message: KimiResponseMessage,
}

#[derive(Deserialize)]
struct KimiResponseMessage {
    content: String,
}

pub struct AiBackend {
    provider: String,
    model: String,
    api_url: String,
    api_key: String,
}

impl Default for AiBackend {
    fn default() -> Self {
        AiBackend {
            provider: "ollama".to_string(),
            model: "qwen3:8b".to_string(),
            api_url: "http://localhost:11434/api/chat".to_string(),
            api_key: String::new(),
        }
    }
}

impl AiBackend {
    pub fn from_config(config: &AiPanelConfig) -> Self {
        Self::new(
            Some(config.provider.clone()),
            Some(config.model_name.clone()),
            Some(config.api_url.clone()),
            Some(config.api_key.clone()),
        )
    }

    pub fn new(
        provider: Option<String>,
        model: Option<String>,
        api_url: Option<String>,
        api_key: Option<String>,
    ) -> Self {
        let provider = provider
            .and_then(normalize_provider)
            .or_else(|| infer_provider(api_url.as_deref()))
            .unwrap_or_else(|| "ollama".to_string());

        let model = model_env_for_provider(&provider)
            .or_else(|| model.and_then(|model| normalize_model(&provider, model)))
            .unwrap_or_else(|| default_model_for_provider(&provider));

        let api_url = api_url_env_for_provider(&provider)
            .or_else(|| api_url.and_then(|api_url| normalize_api_url(&provider, api_url)))
            .unwrap_or_else(|| default_api_url_for_provider(&provider));

        let api_key = api_key
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("MOONSHOT_API_KEY").ok())
            .or_else(|| std::env::var("KIMI_API_KEY").ok())
            .or_else(|| std::env::var("GEMINI_API_KEY").ok())
            .unwrap_or_default();

        Self {
            provider,
            model,
            api_url,
            api_key,
        }
    }

    pub fn discuss_writing_context(
        &self,
        editor_content: &str,
        conversation: &[AiChatMessage],
        sender: Sender<ResponseMessage>,
    ) {
        let mut messages = vec![AiChatMessage {
            role: "system".to_string(),
            content: format!(
                "你是一个结对写作伙伴，正在陪用户思考一篇正在编辑的文本。

你的边界非常重要：
- 绝对不要续写、补全、代写、改写用户文章。
- 不要输出可以直接粘贴进正文的段落、标题、小节或句子。
- 不要把任务理解成润色、扩写、总结全文。
- 你的价值在 research、思考、讨论、反问、辨析和反馈。

你的工作方式：
- 根据编辑器快照，推测用户正在关心什么问题、可能卡在哪里、隐含假设是什么。
- 给出可讨论的思路、研究线索、反例、结构风险、需要追问的问题。
- 用简洁中文回答；如果用户用英文提问，可以跟随英文。
- 尽量像结对伙伴说话，不要像报告。
- 控制在 300 字以内，优先给出最有用的 3-5 个观察。

当前编辑器快照：
{}",
                editor_content
            ),
        }];

        let start = conversation.len().saturating_sub(12);
        messages.extend(conversation[start..].iter().cloned());

        self.send_request(messages, sender);
    }
    pub fn send_request(&self, messages: Vec<AiChatMessage>, sender: Sender<ResponseMessage>) {
        let api_key = self.api_key.clone();

        let provider = self.provider.clone();
        let model = self.model.clone();
        let api_url = self.api_url.clone();
        let api_key = api_key.clone();

        thread::spawn(move || {
            let result = Self::blocking_send_request(provider, model, api_url, api_key, messages);

            let _ = sender.send(ResponseMessage::AiResponse(result));
        });
    }

    fn blocking_send_request(
        provider: String,
        model: String,
        api_url: String,
        api_key: String,
        messages: Vec<AiChatMessage>,
    ) -> Result<String, AiError> {
        let is_local_ollama = is_local_ollama_url(&api_url);
        let mut client_builder =
            Client::builder().timeout(Duration::from_secs(if is_local_ollama { 180 } else { 60 }));
        if is_local_ollama {
            client_builder = client_builder.no_proxy();
        }
        let client = client_builder
            .build()
            .map_err(|e| AiError::ApiError(format!("Failed to build AI client: {}", e)))?;

        let mut request = client.post(&api_url);
        if !api_key.is_empty() && !is_local_ollama {
            request = request.bearer_auth(api_key);
        }

        let response = if provider == "kimi" || api_url.contains("/chat/completions") {
            let request_body = KimiChatRequest {
                model,
                stream: false,
                max_completion_tokens: 512,
                messages,
            };
            request
                .json(&request_body)
                .send()
                .map_err(|e| AiError::ApiError(format!("AI Request failed: {}", e)))?
        } else {
            let request_body = OllamaChatRequest {
                model,
                stream: false,
                think: false,
                options: OllamaOptions { num_predict: 512 },
                messages,
            };
            request
                .json(&request_body)
                .send()
                .map_err(|e| AiError::ApiError(format!("AI Request failed: {}", e)))?
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(AiError::ApiError(format!(
                "API error {}: {}",
                status, error_text
            )));
        }

        if provider == "kimi" || api_url.contains("/chat/completions") {
            let kimi_response: KimiChatResponse = response
                .json()
                .map_err(|e| AiError::ApiError(format!("Failed to parse AI response: {}", e)))?;
            kimi_response
                .choices
                .first()
                .map(|choice| choice.message.content.clone())
                .ok_or_else(|| AiError::ApiError("No response content".to_string()))
        } else {
            let ollama_response: OllamaChatResponse = response
                .json()
                .map_err(|e| AiError::ApiError(format!("Failed to parse AI response: {}", e)))?;
            Ok(ollama_response.message.content)
        }
    }
}

fn normalize_provider(provider: String) -> Option<String> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "kimi" | "moonshot" => Some("kimi".to_string()),
        "ollama" => Some("ollama".to_string()),
        _ => None,
    }
}

fn infer_provider(api_url: Option<&str>) -> Option<String> {
    let api_url = api_url?;
    if api_url.contains("moonshot.ai") || api_url.contains("/chat/completions") {
        Some("kimi".to_string())
    } else if is_local_ollama_url(api_url) {
        Some("ollama".to_string())
    } else {
        None
    }
}

fn normalize_model(provider: &str, model: String) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() || trimmed.starts_with("gemini-") {
        None
    } else if provider == "ollama" && trimmed.starts_with("kimi-") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_api_url(provider: &str, api_url: String) -> Option<String> {
    let trimmed = api_url.trim();
    if trimmed.is_empty()
        || trimmed.contains("generativelanguage.googleapis.com")
        || trimmed.contains(":generateContent")
        || (provider == "ollama" && trimmed.contains("moonshot.ai"))
    {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn is_local_ollama_url(api_url: &str) -> bool {
    api_url.contains("localhost:11434") || api_url.contains("127.0.0.1:11434")
}

fn default_model_for_provider(provider: &str) -> String {
    match provider {
        "kimi" => "kimi-k2.7-code".to_string(),
        _ => "qwen3:8b".to_string(),
    }
}

fn default_api_url_for_provider(provider: &str) -> String {
    match provider {
        "kimi" => "https://api.moonshot.ai/v1/chat/completions".to_string(),
        _ => "http://localhost:11434/api/chat".to_string(),
    }
}

fn model_env_for_provider(provider: &str) -> Option<String> {
    match provider {
        "kimi" => std::env::var("KIMI_MODEL")
            .ok()
            .or_else(|| std::env::var("MOONSHOT_MODEL").ok())
            .or_else(|| std::env::var("PAPER_SHELL_AI_MODEL").ok()),
        _ => std::env::var("OLLAMA_MODEL")
            .ok()
            .or_else(|| std::env::var("PAPER_SHELL_AI_MODEL").ok()),
    }
}

fn api_url_env_for_provider(provider: &str) -> Option<String> {
    match provider {
        "kimi" => std::env::var("KIMI_API_URL")
            .ok()
            .or_else(|| std::env::var("MOONSHOT_API_URL").ok())
            .or_else(|| std::env::var("PAPER_SHELL_AI_API_URL").ok()),
        _ => std::env::var("OLLAMA_API_URL")
            .ok()
            .or_else(|| std::env::var("PAPER_SHELL_AI_API_URL").ok()),
    }
}
