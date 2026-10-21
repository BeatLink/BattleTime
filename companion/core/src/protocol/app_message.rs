// App messages: a few values for one watch app, sent on its topic, as docs/companion-protocol.md's "App messages" describes them.
//
// This is the only Rust code that writes or reads the `{"t":"app",...}` line; everything else deals in
// `AppMessage`, so the line can change without touching its users. src/protocol/app-message.ts mirrors it.

use serde_json::{json, Map, Value};

use crate::gadgetbridge::{ascii_json, decode_watch_line};

// ============================================================================
// Limits
// ============================================================================

// The longest topic or key, in characters.
pub const MOST_NAME_CHARACTERS: usize = 32;

// The most numbers one message may carry.
pub const MOST_NUMBERS: usize = 4;

// The most texts one message may carry.
pub const MOST_TEXTS: usize = 1;

// How much of a text the watch keeps, in bytes.
pub const TEXT_BYTES_KEPT: usize = 120;

// The longest line the watch reads, `GB(` and `)` included but not the newline.
pub const LONGEST_LINE_BYTES: usize = 640;

// ============================================================================
// The message
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppMessage {
    // The watch app's id.
    pub topic: String,
    // Named whole numbers, in the order they were given.
    pub numbers: Vec<(String, i64)>,
    // One named text, which the watch cuts to `TEXT_BYTES_KEPT` bytes.
    pub text: Option<(String, String)>,
}

impl AppMessage {
    pub fn new(topic: impl Into<String>) -> AppMessage {
        AppMessage {
            topic: topic.into(),
            ..AppMessage::default()
        }
    }

    pub fn with_number(mut self, key: impl Into<String>, value: i64) -> AppMessage {
        self.numbers.push((key.into(), value));
        self
    }

    pub fn with_text(mut self, key: impl Into<String>, value: impl Into<String>) -> AppMessage {
        self.text = Some((key.into(), value.into()));
        self
    }

    // A message from a JSON object of numbers, booleans and at most one string, checked against the limits.
    pub fn from_data(topic: &str, data: &Value) -> Result<AppMessage, String> {
        let Some(fields) = data.as_object() else {
            return Err("An app message's data must be an object.".into());
        };
        let mut message = AppMessage::new(topic);
        for (key, value) in fields {
            match value {
                Value::Bool(flag) => message.numbers.push((key.clone(), i64::from(*flag))),
                Value::Number(number) => {
                    let Some(whole) = number.as_i64().or_else(|| whole_number(number.as_f64()?)) else {
                        return Err(format!("\"{key}\" must be a whole number."));
                    };
                    message.numbers.push((key.clone(), whole));
                }
                Value::String(text) => {
                    if message.text.is_some() {
                        return Err(format!("An app message carries at most {MOST_TEXTS} text."));
                    }
                    message.text = Some((key.clone(), text.clone()));
                }
                _ => return Err(format!("\"{key}\" must be a number, true or false, or a text.")),
            }
        }
        message.check()?;
        Ok(message)
    }

    // Why the watch could not take this message, or Ok when it can.
    pub fn check(&self) -> Result<(), String> {
        if !is_name(&self.topic) {
            return Err(format!(
                "\"{}\" is not a topic: use 1 to {MOST_NAME_CHARACTERS} letters, digits, _ and -.",
                self.topic
            ));
        }
        if self.numbers.len() > MOST_NUMBERS {
            return Err(format!("An app message carries at most {MOST_NUMBERS} numbers."));
        }
        let keys = self
            .numbers
            .iter()
            .map(|(key, _)| key)
            .chain(self.text.iter().map(|(key, _)| key));
        let mut seen = Vec::new();
        for key in keys {
            if !is_name(key) {
                return Err(format!(
                    "\"{key}\" is not a key: use 1 to {MOST_NAME_CHARACTERS} letters, digits, _ and -."
                ));
            }
            if seen.contains(&key) {
                return Err(format!("\"{key}\" is given twice."));
            }
            seen.push(key);
        }
        if let Some((key, _)) = self
            .numbers
            .iter()
            .find(|(_, value)| i32::try_from(*value).is_err())
        {
            return Err(format!("\"{key}\" does not fit in a 32-bit whole number."));
        }
        Ok(())
    }

    // The values as one JSON object, numbers first, as plugins and the window see them.
    pub fn data(&self) -> Value {
        let mut fields = Map::new();
        for (key, value) in &self.numbers {
            fields.insert(key.clone(), json!(value));
        }
        if let Some((key, text)) = &self.text {
            fields.insert(key.clone(), json!(text));
        }
        Value::Object(fields)
    }

    // A number by its key.
    pub fn number(&self, key: &str) -> Option<i64> {
        self.numbers
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| *value)
    }
}

// Whether a topic or key is 1 to 32 letters, digits, `_` or `-`.
pub fn is_name(name: &str) -> bool {
    (1..=MOST_NAME_CHARACTERS).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn whole_number(value: f64) -> Option<i64> {
    (value.is_finite() && value.fract() == 0.0 && value.abs() < 9.0e15).then_some(value as i64)
}

// ============================================================================
// Phone to watch
// ============================================================================

// The line GB({"t":"app","ch":...,"d":{...}}) for the watch, or why it cannot be sent.
pub fn encode_app_message(message: &AppMessage) -> Result<String, String> {
    message.check()?;
    let line = format!(
        "GB({})",
        ascii_json(&json!({ "t": "app", "ch": message.topic, "d": message.data() }))
    );
    if line.len() > LONGEST_LINE_BYTES {
        return Err(format!(
            "The message is {} bytes as a line; the watch reads at most {LONGEST_LINE_BYTES}.",
            line.len()
        ));
    }
    Ok(format!("{line}\n"))
}

// ============================================================================
// Watch to phone
// ============================================================================

// The app message in a decoded watch message, or None when it is another kind or has no valid topic.
pub fn read_app_message(message: &Value) -> Option<AppMessage> {
    if message["t"] != "app" {
        return None;
    }
    let topic = message["ch"].as_str().filter(|topic| is_name(topic))?;
    let mut read = AppMessage::new(topic);
    for (key, value) in message["d"].as_object().into_iter().flatten() {
        match value {
            Value::Bool(flag) => read.numbers.push((key.clone(), i64::from(*flag))),
            Value::Number(number) => {
                if let Some(whole) = number.as_i64().or_else(|| whole_number(number.as_f64()?)) {
                    read.numbers.push((key.clone(), whole));
                }
            }
            Value::String(text) if read.text.is_none() => read.text = Some((key.clone(), text.clone())),
            _ => {}
        }
    }
    Some(read)
}

// The app message on one line of watch output, or None for any other line.
pub fn decode_app_line(line: &str) -> Option<AppMessage> {
    read_app_message(&decode_watch_line(line)?)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_goes_out_as_one_gb_line() {
        let message = AppMessage::new("sos").with_number("on", 0);
        assert_eq!(
            encode_app_message(&message).unwrap(),
            "GB({\"ch\":\"sos\",\"d\":{\"on\":0},\"t\":\"app\"})\n"
        );
    }

    #[test]
    fn data_takes_numbers_booleans_and_one_text() {
        let message = AppMessage::from_data(
            "buzz-back",
            &json!({ "count": 3, "loud": true, "half": 2.0, "note": "Hi" }),
        )
        .unwrap();
        assert_eq!(message.number("count"), Some(3));
        assert_eq!(message.number("loud"), Some(1));
        assert_eq!(message.number("half"), Some(2));
        assert_eq!(message.text, Some(("note".into(), "Hi".into())));
        assert_eq!(
            message.data(),
            json!({ "count": 3, "loud": 1, "half": 2, "note": "Hi" })
        );
    }

    #[test]
    fn data_outside_the_limits_is_refused() {
        let refusal = |data: Value| AppMessage::from_data("app", &data).unwrap_err();
        assert!(refusal(json!([1])).contains("must be an object"));
        assert!(refusal(json!({ "a": 1.5 })).contains("whole number"));
        assert!(refusal(json!({ "a": null })).contains("must be a number"));
        assert!(refusal(json!({ "a": "x", "b": "y" })).contains("at most 1 text"));
        assert!(refusal(json!({ "a": 1, "b": 2, "c": 3, "d": 4, "e": 5 })).contains("at most 4 numbers"));
        assert!(refusal(json!({ "a b": 1 })).contains("is not a key"));
        assert!(refusal(json!({ "a": 3_000_000_000_i64 })).contains("32-bit"));
    }

    #[test]
    fn topics_and_keys_are_short_plain_names() {
        assert!(is_name("buzz-back") && is_name("Remote_2") && is_name(&"a".repeat(32)));
        assert!(!is_name("") && !is_name(&"a".repeat(33)) && !is_name("a.b") && !is_name("é"));
        assert!(AppMessage::from_data("no topic", &json!({}))
            .unwrap_err()
            .contains("is not a topic"));
        let twice = AppMessage::new("app").with_number("a", 1).with_text("a", "x");
        assert!(twice.check().unwrap_err().contains("given twice"));
    }

    #[test]
    fn other_characters_are_escaped_and_a_long_line_is_refused() {
        let message = AppMessage::new("note").with_text("text", "Z\u{fc}rich");
        assert!(encode_app_message(&message)
            .unwrap()
            .contains("\"text\":\"Z\\u00fcrich\""));
        let long = AppMessage::new("note").with_text("text", "\u{1f600}".repeat(60));
        assert!(encode_app_message(&long).unwrap_err().contains("at most 640"));
        let fits = AppMessage::new("note").with_text("text", "a".repeat(590));
        assert!(encode_app_message(&fits).unwrap().len() <= LONGEST_LINE_BYTES + 1);
    }

    #[test]
    fn watch_lines_read_back_as_messages() {
        let read = decode_app_line(r#"{"t":"app","ch":"remote","d":{"button":2,"held":false}}"#).unwrap();
        assert_eq!(read.topic, "remote");
        assert_eq!(read.numbers, vec![("button".into(), 2), ("held".into(), 0)]);
        assert_eq!(
            decode_app_line(r#"{"t":"app","ch":"remote"}"#).unwrap().numbers,
            vec![]
        );
        assert_eq!(
            decode_app_line(r#"{"t":"app","ch":"r","d":{"a":1.5,"b":"x"}}"#),
            Some(AppMessage::new("r").with_text("b", "x"))
        );
        assert_eq!(decode_app_line(r#"{"t":"app","ch":"bad topic","d":{}}"#), None);
        assert_eq!(decode_app_line(r#"{"t":"findPhone","n":true}"#), None);
        assert_eq!(decode_app_line("console text"), None);
    }
}
