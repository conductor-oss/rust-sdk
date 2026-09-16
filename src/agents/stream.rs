// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Server-Sent Events decoding for a running agent execution.
//!
//! Wraps the byte stream returned by `crate::client::AgentClient::stream` (`GET
//! /agent/stream/{execution_id}`, python-sdk's `stream_sse`) and turns it into a sequence of
//! [`AgentEvent`]s. Per `docs/agents/parity-plan.md` §2, `execution_id` is mandatory on every
//! variant — `Handoff`/`Sequential`/`Parallel` strategies put a pending `HUMAN` step in a nested
//! sub-execution, and `approve`/`reject`/`respond` must target that inner id, not a caller-held
//! top-level one, so there is no variant that can be constructed without one.
//!
//! The SSE wire format itself (`data:` lines, blank-line frame boundaries, optional `event:`/
//! `id:`/`retry:` lines and `:`-prefixed comments this parser ignores) is the standard documented
//! at <https://html.spec.whatwg.org/multipage/server-sent-events.html>; only the `data:` payload
//! (parsed as this crate's own `AgentEvent` JSON shape) is meaningful here.

use futures::{StreamExt as _, TryStreamExt as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ConductorError, Result};

/// A single decoded event from an agent execution's SSE stream.
///
/// Every variant carries `execution_id` — see the module docs for why it can't be optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    /// A chat/log message emitted during the run.
    #[serde(rename = "message")]
    Message {
        #[serde(rename = "executionId")]
        execution_id: String,
        content: Value,
    },
    /// A lifecycle progress update (e.g. "model call started").
    #[serde(rename = "progress")]
    Progress {
        #[serde(rename = "executionId")]
        execution_id: String,
        message: String,
    },
    /// The execution is paused on a human-in-the-loop tool call awaiting a decision.
    #[serde(rename = "waiting")]
    Waiting {
        #[serde(rename = "executionId")]
        execution_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        args: Value,
    },
    /// The execution finished successfully.
    #[serde(rename = "done")]
    Done {
        #[serde(rename = "executionId")]
        execution_id: String,
        output: Value,
    },
    /// The execution failed.
    #[serde(rename = "error")]
    Error {
        #[serde(rename = "executionId")]
        execution_id: String,
        error: String,
    },
}

impl AgentEvent {
    /// The execution id carried by every variant.
    #[must_use]
    pub fn execution_id(&self) -> &str {
        match self {
            AgentEvent::Message { execution_id, .. }
            | AgentEvent::Progress { execution_id, .. }
            | AgentEvent::Waiting { execution_id, .. }
            | AgentEvent::Done { execution_id, .. }
            | AgentEvent::Error { execution_id, .. } => execution_id,
        }
    }
}

/// Incremental SSE frame decoder: buffers raw bytes and yields complete frames (delimited by a
/// blank line) as they become available, tolerating a frame's bytes arriving split across
/// multiple pushes. Pure/sync and independent of any HTTP transport, so it's testable without a
/// live server (see the tests below).
#[derive(Debug, Default)]
struct SseDecoder {
    buffer: String,
    finished: bool,
}

impl SseDecoder {
    fn new() -> Self {
        Self::default()
    }

    /// Append newly received bytes to the internal buffer.
    fn push(&mut self, chunk: &[u8]) {
        self.buffer.push_str(&String::from_utf8_lossy(chunk));
    }

    /// Signal that the underlying stream has ended; any bytes still buffered afterwards are
    /// treated as one final, unterminated frame.
    fn finish(&mut self) {
        self.finished = true;
    }

    /// Pop the next complete frame's `data:` payload out of the buffer, if one is available.
    fn next_data(&mut self) -> Option<String> {
        loop {
            let boundary = Self::find_boundary(&self.buffer).or_else(|| {
                (self.finished && !self.buffer.is_empty()).then(|| {
                    let len = self.buffer.len();
                    (len, len)
                })
            })?;
            let (frame_end, consumed) = boundary;
            let raw_frame = self.buffer[..frame_end].to_string();
            self.buffer.drain(..consumed);

            let data = Self::extract_data(&raw_frame);
            if data.is_empty() {
                // Blank/comment-only frame (e.g. an SSE heartbeat `: ping`) — keep reading; the
                // buffer strictly shrinks each iteration (`consumed` is always > 0 here), so this
                // always terminates once real data or an empty buffer is reached.
                continue;
            }
            return Some(data);
        }
    }

    /// Find the earliest blank-line frame boundary (`"\n\n"` or `"\r\n\r\n"`), returning
    /// `(frame_end, bytes_consumed_including_delimiter)`.
    fn find_boundary(buffer: &str) -> Option<(usize, usize)> {
        let lf = buffer.find("\n\n").map(|i| (i, i + 2));
        let crlf = buffer.find("\r\n\r\n").map(|i| (i, i + 4));
        match (lf, crlf) {
            (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// Join every `data:` line in a frame with `\n`, per the SSE spec; ignores `event:`/`id:`/
    /// `retry:` lines and `:`-prefixed comment lines.
    fn extract_data(raw_frame: &str) -> String {
        let mut lines = Vec::new();
        for line in raw_frame.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if let Some(rest) = line.strip_prefix("data:") {
                lines.push(rest.strip_prefix(' ').unwrap_or(rest));
            }
        }
        lines.join("\n")
    }
}

fn parse_event(data: &str) -> Result<AgentEvent> {
    serde_json::from_str(data).map_err(ConductorError::Json)
}

/// Decoded [`AgentEvent`] stream over a running agent execution's SSE endpoint.
///
/// Constructed from the [`reqwest::Response`] returned by
/// `crate::client::AgentClient::stream`. Drive it with [`AgentStream::next`]:
///
/// ```ignore
/// let mut stream = agent_client.stream(&execution_id).await?.into();
/// while let Some(event) = stream.next().await.transpose()? {
///     match event {
///         AgentEvent::Waiting { execution_id, tool_name, .. } => { /* ... */ }
///         AgentEvent::Done { output, .. } => println!("{output}"),
///         _ => {}
///     }
/// }
/// ```
pub struct AgentStream {
    body: futures::stream::BoxStream<'static, reqwest::Result<bytes::Bytes>>,
    decoder: SseDecoder,
}

impl AgentStream {
    /// Wrap a raw streaming HTTP response (as returned by `AgentClient::stream`).
    pub fn new(response: reqwest::Response) -> Self {
        Self {
            body: response.bytes_stream().boxed(),
            decoder: SseDecoder::new(),
        }
    }

    /// Fetch the next decoded event, or `None` once the stream has ended.
    ///
    /// Matches the `while let Some(event) = stream.next().await.transpose()?` usage shown in
    /// `docs/agents/parity-plan.md` §2.
    pub async fn next(&mut self) -> Option<Result<AgentEvent>> {
        loop {
            if let Some(data) = self.decoder.next_data() {
                return Some(parse_event(&data));
            }
            match self.body.try_next().await {
                Ok(Some(chunk)) => self.decoder.push(&chunk),
                Ok(None) => {
                    self.decoder.finish();
                    return self.decoder.next_data().map(|data| parse_event(&data));
                }
                Err(e) => return Some(Err(ConductorError::Http(e))),
            }
        }
    }
}

impl From<reqwest::Response> for AgentStream {
    fn from(response: reqwest::Response) -> Self {
        Self::new(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting_json(execution_id: &str) -> String {
        serde_json::json!({
            "type": "waiting",
            "executionId": execution_id,
            "toolName": "issue_refund",
            "args": {"amount": 42.0}
        })
        .to_string()
    }

    #[test]
    fn test_decoder_parses_single_frame() {
        let mut decoder = SseDecoder::new();
        decoder.push(format!("data: {}\n\n", waiting_json("exec-1")).as_bytes());
        let data = decoder.next_data().expect("frame should be available");
        let event = parse_event(&data).unwrap();
        assert_eq!(
            event,
            AgentEvent::Waiting {
                execution_id: "exec-1".to_owned(),
                tool_name: "issue_refund".to_owned(),
                args: serde_json::json!({"amount": 42.0}),
            }
        );
        assert_eq!(event.execution_id(), "exec-1");
    }

    #[test]
    fn test_decoder_handles_frame_split_across_chunks() {
        let full = format!("data: {}\n\n", waiting_json("exec-2"));
        let split_at = full.len() / 2;
        let (first, second) = full.split_at(split_at);

        let mut decoder = SseDecoder::new();
        decoder.push(first.as_bytes());
        assert!(
            decoder.next_data().is_none(),
            "no complete frame yet, must not yield"
        );

        decoder.push(second.as_bytes());
        let data = decoder.next_data().expect("frame should now be complete");
        let event = parse_event(&data).unwrap();
        assert_eq!(event.execution_id(), "exec-2");
    }

    #[test]
    fn test_decoder_handles_multiple_data_lines_in_one_frame() {
        let mut decoder = SseDecoder::new();
        decoder.push(b"data: {\"type\":\"message\",\n");
        decoder.push(b"data: \"executionId\":\"exec-3\",\"content\":\"hi\"}\n\n");
        let data = decoder.next_data().expect("frame should be complete");
        let event = parse_event(&data).unwrap();
        assert_eq!(
            event,
            AgentEvent::Message {
                execution_id: "exec-3".to_owned(),
                content: Value::String("hi".to_owned()),
            }
        );
    }

    #[test]
    fn test_decoder_skips_comments_and_id_lines() {
        let mut decoder = SseDecoder::new();
        let frame = format!(
            "id: 7\n: this is a heartbeat comment\nevent: agent\ndata: {}\n\n",
            waiting_json("exec-4")
        );
        decoder.push(frame.as_bytes());
        let data = decoder.next_data().expect("frame should be complete");
        let event = parse_event(&data).unwrap();
        assert_eq!(event.execution_id(), "exec-4");
    }

    #[test]
    fn test_decoder_yields_multiple_queued_frames_in_order() {
        let mut decoder = SseDecoder::new();
        decoder.push(
            format!(
                "data: {}\n\ndata: {}\n\n",
                waiting_json("exec-5"),
                serde_json::json!({"type": "done", "executionId": "exec-5", "output": "ok"}),
            )
            .as_bytes(),
        );

        let first = parse_event(&decoder.next_data().unwrap()).unwrap();
        assert_eq!(first.execution_id(), "exec-5");
        assert!(matches!(first, AgentEvent::Waiting { .. }));

        let second = parse_event(&decoder.next_data().unwrap()).unwrap();
        assert!(matches!(second, AgentEvent::Done { .. }));
        assert!(decoder.next_data().is_none());
    }

    #[test]
    fn test_decoder_flushes_unterminated_final_frame_on_finish() {
        let mut decoder = SseDecoder::new();
        decoder.push(format!("data: {}", waiting_json("exec-6")).as_bytes());
        assert!(decoder.next_data().is_none(), "no blank line yet");

        decoder.finish();
        let data = decoder
            .next_data()
            .expect("finish() should flush the trailing frame");
        let event = parse_event(&data).unwrap();
        assert_eq!(event.execution_id(), "exec-6");
    }

    #[test]
    fn test_parse_event_error_variant() {
        let json = serde_json::json!({
            "type": "error",
            "executionId": "exec-7",
            "error": "tool timed out"
        })
        .to_string();
        let event = parse_event(&json).unwrap();
        assert_eq!(
            event,
            AgentEvent::Error {
                execution_id: "exec-7".to_owned(),
                error: "tool timed out".to_owned(),
            }
        );
    }

    #[test]
    fn test_parse_event_progress_variant() {
        let json = serde_json::json!({
            "type": "progress",
            "executionId": "exec-8",
            "message": "calling model"
        })
        .to_string();
        let event = parse_event(&json).unwrap();
        assert_eq!(
            event,
            AgentEvent::Progress {
                execution_id: "exec-8".to_owned(),
                message: "calling model".to_owned(),
            }
        );
    }

    #[test]
    fn test_parse_event_rejects_malformed_json() {
        let result = parse_event("not json");
        result.unwrap_err();
    }

    #[tokio::test]
    async fn test_agent_stream_yields_events_from_chunked_bytes() {
        let frame1 = format!("data: {}\n\n", waiting_json("exec-9"));
        let frame2 =
            serde_json::json!({"type": "done", "executionId": "exec-9", "output": 1}).to_string();
        let frame2 = format!("data: {frame2}\n\n");

        // Simulate the body arriving as several chunks, with one frame split across two chunks.
        let split = frame1.len() - 3;
        let chunks: Vec<reqwest::Result<bytes::Bytes>> = vec![
            Ok(bytes::Bytes::copy_from_slice(&frame1.as_bytes()[..split])),
            Ok(bytes::Bytes::copy_from_slice(&frame1.as_bytes()[split..])),
            Ok(bytes::Bytes::copy_from_slice(frame2.as_bytes())),
        ];
        let body = futures::stream::iter(chunks).boxed();

        let mut stream = AgentStream {
            body,
            decoder: SseDecoder::new(),
        };

        let first = stream.next().await.unwrap().unwrap();
        assert!(matches!(first, AgentEvent::Waiting { .. }));
        assert_eq!(first.execution_id(), "exec-9");

        let second = stream.next().await.unwrap().unwrap();
        assert!(matches!(second, AgentEvent::Done { .. }));

        assert!(stream.next().await.is_none());
    }
}
