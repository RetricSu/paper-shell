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
    model: String,
    api_url: String,
    api_key: String,
}

impl Default for AiBackend {
    fn default() -> Self {
        AiBackend {
            model: "gemini-2.5-flash-lite-preview-09-2025".to_string(),
            api_url: "https://generativelanguage.googleapis.com/v1beta/models/".to_string(),
            api_key: String::new(),
        }
    }
}

impl AiBackend {
    pub fn new(model: Option<String>, api_url: Option<String>, api_key: Option<String>) -> Self {
        // 1. Model
        let model = model
            .or_else(|| std::env::var("GEMINI_MODEL").ok()) // 如果前面是 None，尝试读环境变量
            .unwrap_or_else(|| "gemini-2.5-flash-lite-preview-09-2025".to_string()); // 如果还是 None，用默认值

        // 2. API URL
        let api_url = api_url
            .or_else(|| std::env::var("GEMINI_API_URL").ok())
            .unwrap_or_else(|| {
                "https://generativelanguage.googleapis.com/v1beta/models/".to_string()
            });

        // 3. API Key
        let api_key = api_key
            .or_else(|| std::env::var("GEMINI_API_KEY").ok())
            .unwrap_or_else(|| {
                tracing::warn!("GEMINI_API_KEY not found, using empty string");
                String::new()
            });

        Self {
            model,
            api_url,
            api_key,
        }
    }

    pub fn send_request(&self, prompt: String, sender: Sender<Result<String, AiError>>) {
        let api_key = self.api_key.clone();

        let model = self.model.clone();
        let api_url = self.api_url.clone();
        let api_key = api_key.clone();

        thread::spawn(move || {
            let result = Self::blocking_send_request(model, api_url, api_key, prompt);
            let _ = sender.send(result);
        });
    }

    fn blocking_send_request(
        model: String,
        api_url: String,
        api_key: String,
        prompt: String,
    ) -> Result<String, AiError> {
        let client = Client::new();

        let url = format!("{}{}:generateContent?key={}", api_url, model, api_key);

        let request_body = GeminiRequest {
            contents: vec![Content {
                parts: vec![Part { text: prompt }],
            }],
        };

        let response = client
            .post(&url)
            .json(&request_body)
            .send()
            .map_err(|e| AiError::ApiError(format!("AI Request failed: {}", e)))?;

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

        let gemini_response: GeminiResponse = response
            .json()
            .map_err(|e| AiError::ApiError(format!("Failed to parse AI response: {}", e)))?;

        let content = gemini_response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone())
            .ok_or_else(|| AiError::ApiError("No response content".to_string()))?;

        Ok(content)
    }
}
