use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Sender;
use std::thread;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AiError {
    #[error("API error: {0}")]
    ApiError(String),

    #[allow(dead_code)]
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<Content>,
}

#[derive(Serialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct Part {
    text: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct Candidate {
    content: ContentResponse,
}

#[derive(Deserialize)]
struct ContentResponse {
    parts: Vec<PartResponse>,
}

#[derive(Deserialize)]
struct PartResponse {
    text: String,
}

pub struct AiBackend {
    api_key: String,
}

impl Default for AiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AiBackend {
    pub fn new() -> Self {
        // 使用 GEMINI_API_KEY 环境变量
        let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_else(|_| {
            tracing::warn!("GEMINI_API_KEY not found, using empty string");
            String::new()
        });

        Self { api_key }
    }

    #[allow(dead_code)]
    pub fn new_with_config(api_key: String) -> Self {
        Self { api_key }
    }

    /// 发送请求到 Google Gemini API (使用独立线程)
    pub fn send_request(&self, prompt: String, sender: Sender<Result<String, AiError>>) {
        let api_key = self.api_key.clone();

        thread::spawn(move || {
            let result = Self::blocking_send_request(api_key, prompt);
            let _ = sender.send(result);
        });
    }

    fn blocking_send_request(api_key: String, prompt: String) -> Result<String, AiError> {
        let client = Client::new();

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-lite-preview-09-2025:generateContent?key={}",
            api_key
        );

        let request_body = GeminiRequest {
            contents: vec![Content {
                parts: vec![Part { text: prompt }],
            }],
        };

        let response = client
            .post(&url)
            .json(&request_body)
            .send()
            .map_err(|e| AiError::ApiError(format!("请求失败: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().unwrap_or_else(|_| "未知错误".to_string());
            return Err(AiError::ApiError(format!(
                "API 错误 {}: {}",
                status, error_text
            )));
        }

        let gemini_response: GeminiResponse = response
            .json()
            .map_err(|e| AiError::ApiError(format!("解析响应失败: {}", e)))?;

        let content = gemini_response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone())
            .ok_or_else(|| AiError::ApiError("没有响应内容".to_string()))?;

        Ok(content)
    }
}
