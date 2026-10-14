// The watch's message lines, as docs/companion-protocol.md and src/protocol/gadgetbridge.ts describe them.

use serde_json::Value;

// ============================================================================
// Phone to watch
// ============================================================================

// JSON with every character outside printable ASCII escaped, since the watch's line editor drops those bytes.
pub fn ascii_json(value: &Value) -> String {
    let mut out = String::new();
    for character in value.to_string().chars() {
        if (character as u32) < 0x7f {
            out.push(character);
            continue;
        }
        let mut units = [0u16; 2];
        for unit in character.encode_utf16(&mut units) {
            out.push_str(&format!("\\u{unit:04x}"));
        }
    }
    out
}

// The line GB({...}) the watch reads, as Gadgetbridge's Bangle.js driver sends it.
pub fn encode_for_watch(message: &Value) -> String {
    format!("GB({})\n", ascii_json(message))
}

// ============================================================================
// Watch to phone
// ============================================================================

// One line of watch output as a message, or None when it is console text rather than a JSON object with a type.
pub fn decode_watch_line(line: &str) -> Option<Value> {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let parsed: Value = serde_json::from_str(trimmed).ok()?;
    parsed.get("t")?.as_str()?;
    Some(parsed)
}

// Collects raw UART text and hands back complete lines as they arrive.
#[derive(Default)]
pub struct LineBuffer {
    pending: String,
}

impl LineBuffer {
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.pending.push_str(chunk);
        let mut lines: Vec<String> = self.pending.split('\n').map(str::to_string).collect();
        self.pending = lines.pop().unwrap_or_default();
        lines
            .into_iter()
            .map(|line| line.trim_end_matches('\r').to_string())
            .filter(|line| !line.is_empty())
            .collect()
    }

    pub fn reset(&mut self) {
        self.pending.clear();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn non_ascii_text_is_escaped_as_utf16_units() {
        assert_eq!(
            ascii_json(&json!({"loc": "Z\u{fc}rich"})),
            "{\"loc\":\"Z\\u00fcrich\"}"
        );
        assert_eq!(ascii_json(&json!("\u{1f600}")), "\"\\ud83d\\ude00\"");
    }

    #[test]
    fn a_message_goes_out_as_a_gb_line() {
        assert_eq!(
            encode_for_watch(&json!({"t": "vibrate", "n": 1})),
            "GB({\"n\":1,\"t\":\"vibrate\"})\n"
        );
    }

    #[test]
    fn only_json_objects_with_a_type_are_messages() {
        assert_eq!(
            decode_watch_line(r#" {"t":"app","ch":"x","d":{}} "#).unwrap()["ch"],
            "x"
        );
        assert_eq!(decode_watch_line("console text"), None);
        assert_eq!(decode_watch_line(r#"{"no":"type"}"#), None);
        assert_eq!(decode_watch_line("{broken"), None);
    }

    #[test]
    fn lines_are_split_across_chunks_and_blank_ones_dropped() {
        let mut buffer = LineBuffer::default();
        assert!(buffer.push("{\"t\":").is_empty());
        assert_eq!(buffer.push("\"a\"}\r\n\n{\"t\""), vec!["{\"t\":\"a\"}"]);
        buffer.reset();
        assert_eq!(buffer.push("x\n"), vec!["x"]);
    }
}
