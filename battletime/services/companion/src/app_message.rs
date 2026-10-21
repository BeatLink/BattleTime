//! The phone's `app` lines, each a message for one watch app, read into a plain `AppMessage`.
//!
//! This is the only code that knows how such a message is written on the wire. The shell, the app
//! host and the runtime see only the topic, up to four named whole numbers and one named text.
//! The line is read by hand, byte by byte, since a JSON parser for an object of any keys would cost
//! far more flash.

extern crate alloc;

use alloc::boxed::Box;

use battletime_runtime::Value;
use serde_json_core::str::EscapedStr;

use crate::{unescaped, Message, MessageError, MOST_APP_ID_BYTES};

// ============================================================================
// Limits
// ============================================================================

/// The most whole numbers one message carries.
pub const MOST_APP_MESSAGE_NUMBERS: usize = 4;

/// The longest key, as the compiler allows for an app's message fields.
pub const MOST_APP_MESSAGE_KEY_BYTES: usize = 32;

/// The longest text kept; a longer one is cut at the last whole character that fits.
pub const APP_MESSAGE_TEXT_BYTES: usize = 120;

pub type AppMessageKey = heapless::String<MOST_APP_MESSAGE_KEY_BYTES>;

// ============================================================================
// Messages for apps
// ============================================================================

/// A message from the phone for the watch app whose id is `topic`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppMessage {
    pub topic: heapless::String<MOST_APP_ID_BYTES>,
    pub numbers: heapless::Vec<(AppMessageKey, Value), MOST_APP_MESSAGE_NUMBERS>,
    pub text: Option<(AppMessageKey, heapless::String<APP_MESSAGE_TEXT_BYTES>)>,
}

impl AppMessage {
    /// The numbers as pairs of key and value, for the app host.
    pub fn number_fields(&self) -> heapless::Vec<(&str, Value), MOST_APP_MESSAGE_NUMBERS> {
        self.numbers
            .iter()
            .map(|(key, value)| (key.as_str(), *value))
            .collect()
    }

    /// The text as a pair of key and text, for the app host.
    pub fn text_field(&self) -> Option<(&str, &str)> {
        self.text
            .as_ref()
            .map(|(key, text)| (key.as_str(), text.as_str()))
    }
}

/// Whether a topic or key is 1 to 32 letters, digits, `_` or `-`.
fn is_name(text: &str) -> bool {
    (1..=MOST_APP_MESSAGE_KEY_BYTES).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

// ============================================================================
// Reading the line
// ============================================================================

/// Reads `{"t":"app","ch":"<topic>","d":{...}}`, already known to be JSON with that `t`.
#[inline(never)]
pub(crate) fn parse_app_message(json: &str) -> Result<Message<'_>, MessageError> {
    let mut reader = Reader {
        bytes: json.as_bytes(),
        at: 0,
    };
    let mut message = Box::<AppMessage>::default();
    let mut has_data = false;
    reader.expect(b'{')?;
    loop {
        let key = reader.string()?;
        reader.expect(b':')?;
        match key {
            "ch" => {
                let topic = reader.string()?;
                if !is_name(topic) {
                    return Err(MessageError::NotJson);
                }
                message.topic = topic.try_into().map_err(|_| MessageError::NotJson)?;
            }
            "d" => {
                reader.data(&mut message)?;
                has_data = true;
            }
            _ => reader.skip_value()?,
        }
        if !reader.next_in_object()? {
            break;
        }
    }
    if message.topic.is_empty() || !has_data {
        return Err(MessageError::NotJson);
    }
    Ok(Message::App(message))
}

/// A cursor over the line's bytes.
struct Reader<'line> {
    bytes: &'line [u8],
    at: usize,
}

impl<'line> Reader<'line> {
    /// The next byte that is not white space, without taking it.
    fn peek(&mut self) -> Option<u8> {
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
        self.bytes.get(self.at).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), MessageError> {
        if self.peek() != Some(byte) {
            return Err(MessageError::NotJson);
        }
        self.at += 1;
        Ok(())
    }

    /// Takes a `,` and says true, or a `}` and says false.
    fn next_in_object(&mut self) -> Result<bool, MessageError> {
        let more = self.peek() == Some(b',');
        self.expect(if more { b',' } else { b'}' })?;
        Ok(more)
    }

    /// A string's text between its quotes, its escapes left as written.
    fn string(&mut self) -> Result<&'line str, MessageError> {
        self.expect(b'"')?;
        let start = self.at;
        let mut after_backslash = false;
        while let Some(&byte) = self.bytes.get(self.at) {
            self.at += 1;
            if byte == b'"' && !after_backslash {
                let text = &self.bytes[start..self.at - 1];
                return core::str::from_utf8(text).map_err(|_| MessageError::NotText);
            }
            after_backslash = !after_backslash && byte == b'\\';
        }
        Err(MessageError::NotJson)
    }

    /// The bare word or number that comes next, such as `true` or `-12`.
    fn word(&mut self) -> &'line [u8] {
        self.peek();
        let start = self.at;
        while self.bytes.get(self.at).is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'+' || *byte == b'.'
        }) {
            self.at += 1;
        }
        &self.bytes[start..self.at]
    }

    /// Passes over a value of a key the watch does not read, counting brackets outside strings.
    fn skip_value(&mut self) -> Result<(), MessageError> {
        let mut depth = 0usize;
        loop {
            match self.peek().ok_or(MessageError::NotJson)? {
                b'"' => {
                    self.string()?;
                }
                b'{' | b'[' => {
                    depth += 1;
                    self.at += 1;
                }
                b'}' | b']' if depth > 0 => {
                    depth -= 1;
                    self.at += 1;
                }
                b',' | b':' if depth > 0 => self.at += 1,
                _ => {
                    if self.word().is_empty() {
                        return Err(MessageError::NotJson);
                    }
                }
            }
            if depth == 0 {
                return Ok(());
            }
        }
    }

    /// The `d` object: up to four whole numbers or booleans and one text, each under a plain key.
    fn data(&mut self, message: &mut AppMessage) -> Result<(), MessageError> {
        self.expect(b'{')?;
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(());
        }
        loop {
            let key = self.string()?;
            if !is_name(key) {
                return Err(MessageError::NotJson);
            }
            let is_repeated = message.numbers.iter().any(|(number_key, _)| number_key == key)
                || message.text.as_ref().is_some_and(|(text_key, _)| text_key == key);
            if is_repeated {
                return Err(MessageError::NotJson);
            }
            let key = AppMessageKey::try_from(key).map_err(|_| MessageError::NotJson)?;
            self.expect(b':')?;
            if self.peek() == Some(b'"') {
                let text = self.string()?;
                if message.text.is_some() {
                    return Err(MessageError::NotJson);
                }
                message.text = Some((key, unescaped(Some(EscapedStr(text)))));
            } else {
                let value = whole_number(self.word()).ok_or(MessageError::NotJson)?;
                message
                    .numbers
                    .push((key, value))
                    .map_err(|_| MessageError::NotJson)?;
            }
            if !self.next_in_object()? {
                return Ok(());
            }
        }
    }
}

/// `true` as 1, `false` as 0, or a whole number that fits in 32 bits, such as `-12`.
fn whole_number(word: &[u8]) -> Option<Value> {
    match word {
        b"true" => return Some(1),
        b"false" => return Some(0),
        _ => {}
    }
    let (is_negative, digits) = match word.split_first()? {
        (b'-', rest) => (true, rest),
        _ => (false, word),
    };
    if digits.is_empty() {
        return None;
    }
    let mut value: i32 = 0;
    for &digit in digits {
        if !digit.is_ascii_digit() {
            return None;
        }
        let digit = i32::from(digit - b'0');
        value = value
            .checked_mul(10)?
            .checked_add(if is_negative { -digit } else { digit })?;
    }
    Some(Value::from(value))
}

#[cfg(test)]
#[path = "app_message_tests.rs"]
mod app_message_tests;
