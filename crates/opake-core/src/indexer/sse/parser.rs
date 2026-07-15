// SSE line-framing parser.
//
// Server-Sent Events frames each event as a series of lines terminated by
// a double newline:
//
//     event: directory:upsert
//     data: {"directory_uri":"at://...","owner_did":"..."}
//     \n
//
// Lines beginning with `:` are comments (used by the broadcaster for
// `: keepalive\n\n` heartbeats, which we silently drop). Multi-line
// data is concatenated with newlines between chunks. Unrecognized fields
// (`id:`, `retry:`) are parsed but currently ignored — we don't use
// Last-Event-ID replay or server-suggested reconnect delays.
//
// This parser is used by both the native (reqwest) and WASM (EventSource)
// SSE connections. On WASM, EventSource handles framing natively in the
// browser and we only use this parser for testing. On native, we drive it
// over bytes from `reqwest::Response::bytes_stream()`.

use crate::error::Error;
use crate::indexer::sse::events::SseEvent;

/// Accumulates SSE field lines into complete events. Call [`feed_line`]
/// for each `\n`-stripped line; it returns `Some(event)` when a blank line
/// indicates a complete frame.
#[derive(Debug, Default)]
pub struct SseFrameBuffer {
    /// Event type (`event:` field). Defaults to `"message"` per the spec
    /// but we treat missing names as a parse error since the broadcaster
    /// always sets an explicit type.
    event: Option<String>,
    /// Accumulated `data:` payload. Multi-line data is newline-joined.
    data: Vec<u8>,
}

impl SseFrameBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one line (without the trailing `\n`) to the parser. A blank
    /// line signals the end of a frame and causes the buffer to emit an
    /// event (or `None` if the frame was empty / comment-only).
    ///
    /// Returns `None` for in-progress frames and comments. On a blank line,
    /// returns:
    /// - `Some(Ok(event))` for a complete parseable frame
    /// - `Some(Err(_))` for a frame with malformed JSON data
    ///
    /// After a frame is emitted (or errors), the buffer is reset and ready
    /// for the next frame.
    pub fn feed_line(&mut self, line: &[u8]) -> Option<Result<SseEvent, Error>> {
        // Blank line = frame terminator.
        if line.is_empty() {
            return self.flush_frame();
        }

        // Comments start with `:`. The broadcaster uses `: keepalive\n\n`
        // every 15s. Drop them silently.
        if line.starts_with(b":") {
            return None;
        }

        // Field lines are `field: value` or `field:value`. The space after
        // the colon is optional per the spec.
        let (field, value) = match line.iter().position(|&b| b == b':') {
            Some(i) => {
                let field = &line[..i];
                let mut value_start = i + 1;
                if line.get(value_start) == Some(&b' ') {
                    value_start += 1;
                }
                (field, &line[value_start..])
            }
            None => {
                // A line with no colon is treated as a field name with an
                // empty value per the spec. The broadcaster doesn't emit
                // these, but we tolerate them.
                (line, &b""[..])
            }
        };

        match field {
            b"event" => {
                self.event = Some(String::from_utf8_lossy(value).into_owned());
            }
            b"data" => {
                // Multi-line data: newline-join successive chunks.
                if !self.data.is_empty() {
                    self.data.push(b'\n');
                }
                self.data.extend_from_slice(value);
            }
            b"id" | b"retry" => {
                // Not used — we don't do Last-Event-ID replay or honor
                // server-suggested reconnect intervals.
            }
            _ => {
                // Unknown field — skip per spec.
            }
        }

        None
    }

    /// Flush the accumulated frame state as an event, resetting the buffer.
    fn flush_frame(&mut self) -> Option<Result<SseEvent, Error>> {
        // Empty frame (just a blank line after nothing) = comment-only or
        // start-of-stream; yield nothing.
        if self.event.is_none() && self.data.is_empty() {
            return None;
        }

        let event_name = self.event.take();
        let data = std::mem::take(&mut self.data);

        match event_name {
            Some(name) => Some(SseEvent::from_name_and_data(&name, &data)),
            None => {
                // Data without an event type — the spec says this defaults
                // to the "message" type, but our broadcaster never emits
                // such frames. Treat as parse error for visibility.
                Some(Err(Error::Sse("SSE frame missing event: header".into())))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Byte stream helper — splits a chunk of bytes into line-terminated frames.
// ---------------------------------------------------------------------------

/// A running line buffer for feeding arbitrary byte chunks into the frame
/// parser. Used by the native `reqwest` connection, where each poll of
/// `bytes_stream()` yields an arbitrary chunk that may split mid-line.
#[derive(Debug, Default)]
pub struct SseLineAccumulator {
    frame: SseFrameBuffer,
    /// Partial line carried across chunks.
    carry: Vec<u8>,
}

impl SseLineAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk of bytes. Emits any events that became complete as a
    /// result of this chunk. Partial lines are buffered until the next
    /// call delivers their terminator.
    ///
    /// Accepts both `\n` and `\r\n` line endings (the broadcaster uses `\n`
    /// but we normalize for robustness against reverse proxies).
    pub fn feed_bytes(&mut self, chunk: &[u8]) -> Vec<Result<SseEvent, Error>> {
        let mut events = Vec::new();
        // eslint wouldn't approve of this — prepend carry, walk the buffer
        // looking for line terminators. The alternative (slicing with split)
        // allocates per-chunk and is measurably slower in the hot path.
        let mut merged: Vec<u8> = Vec::with_capacity(self.carry.len() + chunk.len());
        merged.append(&mut self.carry);
        merged.extend_from_slice(chunk);

        let mut start = 0;
        let mut i = 0;
        while i < merged.len() {
            if merged[i] == b'\n' {
                // Strip optional preceding \r.
                let line_end = if i > 0 && merged[i - 1] == b'\r' {
                    i - 1
                } else {
                    i
                };
                let line = &merged[start..line_end];
                if let Some(result) = self.frame.feed_line(line) {
                    events.push(result);
                }
                start = i + 1;
            }
            i += 1;
        }

        // Carry the unterminated trailing bytes.
        if start < merged.len() {
            self.carry = merged[start..].to_vec();
        }

        events
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_string(acc: &mut SseLineAccumulator, s: &str) -> Vec<Result<SseEvent, Error>> {
        acc.feed_bytes(s.as_bytes())
    }

    #[test]
    fn parses_single_frame() {
        let mut acc = SseLineAccumulator::new();
        let data = "event: at.opake.directory:delete\ndata: {\"uri\":\"at://a/b/c\"}\n\n";
        let events = feed_string(&mut acc, data);
        assert_eq!(events.len(), 1);
        assert!(events[0].is_ok());
        assert_eq!(
            events[0].as_ref().unwrap().event_name(),
            "at.opake.directory:delete"
        );
    }

    #[test]
    fn drops_keepalive_comments() {
        let mut acc = SseLineAccumulator::new();
        let events = feed_string(&mut acc, ": keepalive\n\n");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn handles_split_chunks() {
        let mut acc = SseLineAccumulator::new();
        // Split an event across three feeds, including mid-data and
        // mid-terminator.
        assert_eq!(feed_string(&mut acc, "event: at.opake.directory:").len(), 0);
        assert_eq!(feed_string(&mut acc, "delete\ndata: {\"uri").len(), 0);
        let events = feed_string(&mut acc, "\":\"at://x\"}\n\n");
        assert_eq!(events.len(), 1);
        match events[0].as_ref().unwrap() {
            SseEvent::DirectoryDelete(d) => {
                assert_eq!(d.uri, "at://x");
            }
            _ => panic!("expected DirectoryDelete"),
        }
    }

    #[test]
    fn handles_crlf_line_endings() {
        let mut acc = SseLineAccumulator::new();
        let events = feed_string(
            &mut acc,
            "event: at.opake.keyring:delete\r\ndata: {\"uri\":\"at://kr\"}\r\n\r\n",
        );
        assert_eq!(events.len(), 1);
        assert!(events[0].is_ok());
    }

    #[test]
    fn parses_multiple_frames_in_one_chunk() {
        let mut acc = SseLineAccumulator::new();
        let data = concat!(
            "event: at.opake.grant:delete\n",
            "data: {\"uri\":\"at://g1\"}\n",
            "\n",
            "event: at.opake.grant:delete\n",
            "data: {\"uri\":\"at://g2\"}\n",
            "\n",
        );
        let events = feed_string(&mut acc, data);
        assert_eq!(events.len(), 2);
        assert!(events[0].is_ok());
        assert!(events[1].is_ok());
    }

    #[test]
    fn multi_line_data_is_newline_joined() {
        // Split a JSON string literal across two `data:` lines. JSON
        // strings can't contain raw newlines, so newline-joining must
        // produce invalid JSON — confirming the parser joins multi-line
        // data with a newline per the SSE spec.
        let mut buf = SseFrameBuffer::new();
        assert!(buf.feed_line(b"event: at.opake.keyring:delete").is_none());
        assert!(buf.feed_line(b"data: {\"uri\": \"at://").is_none());
        assert!(buf.feed_line(b"data: a\"}").is_none());
        let result = buf.feed_line(b"").unwrap();
        // The combined data is `{"uri": "at://\na"}` — the raw newline
        // inside the string literal is not allowed in JSON.
        assert!(
            result.is_err(),
            "expected parse error from newline-joined JSON string, got {:?}",
            result
        );
    }

    #[test]
    fn multi_line_data_with_whitespace_safe_join_succeeds() {
        // Sanity check: splitting between JSON tokens (where newline is
        // whitespace) parses fine.
        let mut buf = SseFrameBuffer::new();
        assert!(buf.feed_line(b"event: at.opake.keyring:delete").is_none());
        assert!(buf.feed_line(b"data: {\"uri\":").is_none());
        assert!(buf.feed_line(b"data: \"at://a\"}").is_none());
        let result = buf.feed_line(b"").unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn ignores_id_and_retry_fields() {
        let mut acc = SseLineAccumulator::new();
        let data = concat!(
            "id: 42\n",
            "retry: 5000\n",
            "event: at.opake.directory:delete\n",
            "data: {\"uri\":\"at://x\"}\n",
            "\n",
        );
        let events = feed_string(&mut acc, data);
        assert_eq!(events.len(), 1);
        assert!(events[0].is_ok());
    }

    #[test]
    fn tolerates_optional_space_after_colon() {
        // "data:X" is legal per the spec (space optional).
        let mut acc = SseLineAccumulator::new();
        let events = feed_string(
            &mut acc,
            "event:at.opake.directory:delete\ndata:{\"uri\":\"at://y\"}\n\n",
        );
        assert_eq!(events.len(), 1);
        assert!(events[0].is_ok());
    }

    #[test]
    fn empty_frame_between_events_is_noop() {
        let mut acc = SseLineAccumulator::new();
        let events = feed_string(&mut acc, "\n\n\n");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn frame_with_data_but_no_event_errors() {
        let mut acc = SseLineAccumulator::new();
        let events = feed_string(&mut acc, "data: {\"uri\":\"at://x\"}\n\n");
        assert_eq!(events.len(), 1);
        assert!(events[0].is_err());
    }
}
