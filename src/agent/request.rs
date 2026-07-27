use async_openai::types::chat::*;
use futures::StreamExt;
use reqwest::StatusCode;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::agent::AgentEvent;
use crate::config::Config;

/// Send a chat completion request to the OpenAI-compatible API with retry logic.
/// Returns the HTTP response stream on success, or an error message on failure.
pub async fn send_request(
    config: &Config,
    messages: Vec<ChatCompletionRequestMessage>,
    tools: Vec<ChatCompletionTools>,
    http_client: &reqwest::Client,
) -> Result<reqwest::Response, String> {
    let mut request = CreateChatCompletionRequestArgs::default()
        .model(&config.model)
        .messages(messages)
        .tools(tools)
        .stream(true)
        .build()
        .map_err(|e| format!("failed to build chat completion request: {}", e))?;

    if let Some(max_tokens) = config.max_tokens {
        request.max_completion_tokens = Some(max_tokens);
    }

    let url = format!("{}/chat/completions", config.base_url);
    let max_retries = 3;

    for retry in 0..=max_retries {
        let result = http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .json(&request)
            .send()
            .await;

        match result {
            Ok(resp) => {
                if resp.status().is_success() {
                    return Ok(resp);
                }

                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let error_msg = format!("API error: HTTP {} - {}", status, body);

                if retry < max_retries && should_retry_status(status) {
                    let wait = std::time::Duration::from_millis(1000 * (2u64.pow(retry as u32)));
                    tracing::warn!(
                        "API error (HTTP {}), retrying in {}s...",
                        status,
                        wait.as_secs()
                    );
                    tokio::time::sleep(wait).await;
                    continue;
                }

                tracing::error!("{}", error_msg);
                return Err(error_msg);
            }
            Err(e) => {
                if retry < max_retries {
                    let wait = std::time::Duration::from_millis(1000 * (2u64.pow(retry as u32)));
                    tracing::warn!("Network error ({}), retrying in {}s...", e, wait.as_secs());
                    tokio::time::sleep(wait).await;
                    continue;
                }
                let error_msg = format!("API error after {} retries: {}", max_retries, e);
                tracing::error!("{}", error_msg);
                return Err(error_msg);
            }
        }
    }

    // Unreachable, but kept for compiler
    Err("API request failed after all retries".to_string())
}

pub fn should_retry_status(status: StatusCode) -> bool {
    status.is_server_error()
        || status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
}

/// Fix common malformed values in streaming response chunks.
pub fn fix_response_value(value: &mut Value) {
    let Some(choices) = value.get_mut("choices") else {
        return;
    };
    let Some(arr) = choices.as_array_mut() else {
        return;
    };
    for choice in arr {
        if let Some(fr) = choice.get_mut("finish_reason")
            && fr == ""
        {
            *fr = Value::Null;
        }
        let Some(delta) = choice.get_mut("delta") else {
            continue;
        };
        let Some(tool_calls) = delta.get_mut("tool_calls") else {
            continue;
        };
        let Some(arr2) = tool_calls.as_array_mut() else {
            continue;
        };
        for tc in arr2 {
            if let Some(t) = tc.get_mut("type")
                && t == ""
            {
                *t = Value::String("function".to_string());
            }
        }
    }
}

/// Parse a streaming SSE response into accumulated text and tool call states.
pub struct ParsedStream {
    pub content: String,
    pub tool_calls: Vec<ToolCallState>,
}

#[derive(Debug, Clone)]
pub struct ToolCallState {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl ToolCallState {
    fn new() -> Self {
        ToolCallState {
            id: String::new(),
            name: String::new(),
            arguments: String::new(),
        }
    }
}

pub async fn parse_stream(
    http_resp: reqwest::Response,
    event_tx: Option<mpsc::UnboundedSender<AgentEvent>>,
) -> Result<ParsedStream, String> {
    let mut content_buffer = String::new();
    let mut tool_calls: Vec<ToolCallState> = Vec::new();

    let byte_stream = http_resp
        .bytes_stream()
        .map(|r| r.map_err(std::io::Error::other));
    let mut event_stream = std::pin::pin!(eventsource_stream::EventStream::new(byte_stream));

    loop {
        let timeout_dur = if content_buffer.is_empty() {
            std::time::Duration::from_secs(30)
        } else {
            std::time::Duration::from_secs(8)
        };

        let event_result = match tokio::time::timeout(timeout_dur, event_stream.next()).await {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!("Stream idle timeout reached, ending stream");
                break;
            }
        };

        match event_result {
            Some(Ok(event)) => {
                if event.data.trim() == "[DONE]" {
                    break;
                }
                if event.event == "keepalive" {
                    continue;
                }

                let mut value: Value = match serde_json::from_str(&event.data) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("Failed to parse chunk JSON: {}", e);
                        continue;
                    }
                };

                fix_response_value(&mut value);

                if let Some(usage) = value.get("usage") {
                    let prompt = usage
                        .get("prompt_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let completion = usage
                        .get("completion_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    // Emit the usage event directly from the parser.
                    if let Some(ref tx) = event_tx {
                        let _ = tx.send(AgentEvent::TokenUsage { prompt, completion });
                    }
                    // Don't pollute the content stream with usage text.
                    continue;
                }

                let stream_chunk: CreateChatCompletionStreamResponse =
                    match serde_json::from_value(value) {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::warn!("Failed to deserialize chunk: {}", e);
                            continue;
                        }
                    };

                if let Some(choice) = stream_chunk.choices.first() {
                    let delta = &choice.delta;

                    if let Some(ref text) = delta.content {
                        content_buffer.push_str(text);
                        if let Some(ref tx) = event_tx {
                            let _ = tx.send(AgentEvent::Token(text.clone()));
                        }
                    }

                    if let Some(ref tool_call_deltas) = delta.tool_calls {
                        for tc_delta in tool_call_deltas {
                            let idx = tc_delta.index as usize;

                            while tool_calls.len() <= idx {
                                tool_calls.push(ToolCallState::new());
                            }

                            if let Some(ref id) = tc_delta.id
                                && !id.is_empty()
                            {
                                tool_calls[idx].id = id.clone();
                            }
                            if let Some(ref func) = tc_delta.function {
                                if let Some(ref name) = func.name
                                    && !name.is_empty()
                                {
                                    tracing::info!("func.name: {}", name);
                                    tool_calls[idx].name = name.clone();
                                }
                                if let Some(ref args) = func.arguments {
                                    tool_calls[idx].arguments.push_str(args);
                                }
                            }
                        }
                    }
                    tracing::info!("tool_calls: {:?}", tool_calls);
                    if choice.finish_reason.is_some() {
                        break;
                    }
                }
            }
            Some(Err(e)) => {
                let err_msg = format!("Stream error: {}", e);
                tracing::error!("{}", err_msg);
                return Err(err_msg);
            }
            None => {
                break;
            }
        }
    }

    Ok(ParsedStream {
        content: content_buffer,
        tool_calls,
    })
}
