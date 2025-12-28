# AI Feature Usage

## Overview
The AI feature integration allows you to get AI-powered assistance for your writing directly within the Paper Shell editor using **Google Gemini API** (AIStudio).

## Setup

### 1. Set Gemini API Key
Before using the AI feature, you need to set your Google AIStudio API key as an environment variable:

```bash
export GEMINI_API_KEY="your-aistudio-api-key-here"
```

For permanent setup, add this to your shell configuration file (`~/.zshrc`, `~/.bashrc`, etc.).

**获取 API Key**: 访问 [Google AI Studio](https://aistudio.google.com/app/apikey) 获取你的 API key。

### 2. Custom API Configuration (Optional)
If you want to use a specific API key programmatically, you can modify the AI backend initialization in `src/app.rs`:

```rust
// Instead of:
ai_backend: Arc::new(AiBackend::new()),

// Use:
ai_backend: Arc::new(AiBackend::new_with_config(
    "your-api-key".to_string()
)),
```

## Features

### Semi-Transparent AI Panel
- **Location**: Top-right corner of the editor
- **Appearance**: Semi-transparent dark panel with a subtle border
- **Always Visible**: The panel stays on top while you write

### AI Assistant Button
- **Button Text**: "🚀 Ask AI"
- **Function**: Sends your current text to the AI for improvement suggestions
- **Processing State**: Changes to "⏳ Working..." while the request is being processed
- **Response Display**: Shows a truncated preview of the AI's response (first 50 characters)

## How to Use

1. **Write your text** in the editor
2. **Click the "🚀 Ask AI" button** in the AI panel
3. **Wait** for the AI to process your request (button shows "⏳ Working...")
4. **View the response** in the panel (truncated preview)
5. **Check the logs** for the full AI response (displayed in the console)

## Architecture

### Backend (`src/backend/ai_backend.rs`)
- Uses Google Gemini API (gemini-pro model)
- Direct HTTP requests with `reqwest`
- Handles async requests using Tokio runtime
- Returns results via message channels

### UI (`src/ui/ai_panel.rs`)
- Renders a semi-transparent panel
- Provides user interaction through buttons
- Shows processing status and response previews

### Integration (`src/app.rs`)
- Manages AI backend lifecycle
- Handles message passing between UI and backend
- Processes AI responses and updates the UI

## Dependencies Added

```toml
reqwest = { version = "0.11", features = ["json"] }
tokio = { version = "1.41", features = ["full"] }
```

## Future Enhancements

Potential improvements for the AI feature:
- Display full response in a separate window
- Multiple AI prompt templates (improve, summarize, expand, etc.)
- Streaming responses for real-time feedback
- Response history
- Ability to apply AI suggestions directly to text
- Custom system prompts
- Support for different AI models
- Local LLM integration
