use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Deserialize)]
struct ChatChunk {
    #[serde(default)]
    choices: Vec<Choice>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    delta: Delta,
}

#[derive(Debug, Deserialize)]
struct Delta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    fn push(&mut self, bytes: &[u8]) -> Vec<Result<String, String>> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();

        while let Some((end, separator_len)) = find_event_boundary(&self.buffer) {
            let event = self.buffer.drain(..end).collect::<Vec<_>>();
            self.buffer.drain(..separator_len);
            if let Some(data) = event_data(&event) {
                events.push(parse_event(&data));
            }
        }

        events
    }

    fn finish(&mut self) -> Option<Result<String, String>> {
        if self.buffer.is_empty() {
            return None;
        }
        let event = std::mem::take(&mut self.buffer);
        event_data(&event).map(|data| parse_event(&data))
    }
}

fn find_event_boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    let lf = bytes
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| (position, 2));
    let crlf = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4));

    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 < right.0 { left } else { right }),
        (Some(boundary), None) | (None, Some(boundary)) => Some(boundary),
        (None, None) => None,
    }
}

fn event_data(event: &[u8]) -> Option<Vec<u8>> {
    let normalized = String::from_utf8_lossy(event).replace("\r\n", "\n");
    let lines = normalized
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect::<Vec<_>>();

    (!lines.is_empty()).then(|| lines.join("\n").into_bytes())
}

fn parse_event(data: &[u8]) -> Result<String, String> {
    let data = std::str::from_utf8(data).map_err(|error| format!("invalid SSE UTF-8: {error}"))?;
    if data.trim() == "[DONE]" {
        return Ok(String::new());
    }

    let chunk: ChatChunk = serde_json::from_str(data)
        .map_err(|error| format!("invalid chat completion event: {error}"))?;
    if let Some(error) = chunk.error {
        return Err(error.message);
    }

    Ok(chunk
        .choices
        .into_iter()
        .filter_map(|choice| choice.delta.content)
        .collect())
}

pub fn default_system_prompt() -> String {
    "Improve the selected text while preserving its meaning, tone, and language. Return only the replacement text.".to_string()
}

pub async fn stream_chat(
    config: &AiConfig,
    system_prompt: &str,
    selected_text: &str,
) -> Result<ReceiverStream<Result<String, String>>, String> {
    if config.base_url.trim().is_empty() {
        return Err("API base URL is empty".to_string());
    }
    if config.model.trim().is_empty() {
        return Err("model is empty".to_string());
    }

    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let mut request = client.post(url).json(&json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": selected_text }
        ],
        "stream": true
    }));
    if !config.api_key.trim().is_empty() {
        request = request.bearer_auth(config.api_key.trim());
    }

    let response = request
        .send()
        .await
        .map_err(|error| format!("AI request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value.pointer("/error/message")?.as_str().map(str::to_owned))
            .unwrap_or_else(|| body.trim().to_string());
        return Err(if detail.is_empty() {
            format!("AI endpoint returned {status}")
        } else {
            format!("AI endpoint returned {status}: {detail}")
        });
    }

    let (sender, receiver) = mpsc::channel(32);
    tokio::spawn(async move {
        let mut bytes = response.bytes_stream();
        let mut decoder = SseDecoder::default();

        while let Some(next) = bytes.next().await {
            match next {
                Ok(chunk) => {
                    for event in decoder.push(&chunk) {
                        if sender.send(event).await.is_err() {
                            return;
                        }
                    }
                }
                Err(error) => {
                    let _ = sender.send(Err(format!("AI stream failed: {error}"))).await;
                    return;
                }
            }
        }

        if let Some(event) = decoder.finish() {
            let _ = sender.send(event).await;
        }
    });

    Ok(ReceiverStream::new(receiver))
}

#[cfg(test)]
mod tests {
    use super::SseDecoder;

    #[test]
    fn decodes_events_split_across_network_chunks() {
        let payload = "data: {\"choices\":[{\"delta\":{\"content\":\"hé\"}}]}\n\ndata: [DONE]\n\n";
        let bytes = payload.as_bytes();
        let split = payload.find('é').unwrap() + 1;
        let mut decoder = SseDecoder::default();

        assert!(decoder.push(&bytes[..split]).is_empty());
        let events = decoder.push(&bytes[split..]);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].as_ref().unwrap(), "hé");
        assert_eq!(events[1].as_ref().unwrap(), "");
    }

    #[test]
    fn decodes_crlf_and_ignores_comments() {
        let mut decoder = SseDecoder::default();
        let events = decoder.push(
            b": keep-alive\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\r\n\r\n",
        );

        assert_eq!(events[0].as_ref().unwrap(), "ok");
    }

    #[test]
    fn surfaces_api_errors() {
        let mut decoder = SseDecoder::default();
        let events = decoder.push(b"data: {\"error\":{\"message\":\"bad key\"}}\n\n");

        assert_eq!(events[0].as_ref().unwrap_err(), "bad key");
    }
}
