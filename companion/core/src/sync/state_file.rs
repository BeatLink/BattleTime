// An app's saved state, `/apps/<id>/state`, as the watch's runtime writes it (`neotime/runtime/src/state.rs`).
//
// The file names every value, so it reads without the app's image: "NTST", version 1, a count, then for
// each value its key, type, array length, element width and bytes.

use serde_json::{json, Map, Value};

// ============================================================================
// The format
// ============================================================================

const MAGIC: &[u8; 4] = b"NTST";
const VERSION: u8 = 1;
const HEADER_BYTES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Int,
    Bool,
    // Milliseconds since 1970 in the watch's local time.
    Instant,
    // Milliseconds.
    Duration,
    Color,
    // A string's number in the app's image.
    Text,
}

impl ValueKind {
    fn from_byte(byte: u8) -> Option<ValueKind> {
        Some(match byte {
            0 => ValueKind::Int,
            1 => ValueKind::Bool,
            2 => ValueKind::Instant,
            3 => ValueKind::Duration,
            4 => ValueKind::Color,
            5 => ValueKind::Text,
            _ => return None,
        })
    }

    fn byte(self) -> u8 {
        match self {
            ValueKind::Int => 0,
            ValueKind::Bool => 1,
            ValueKind::Instant => 2,
            ValueKind::Duration => 3,
            ValueKind::Color => 4,
            ValueKind::Text => 5,
        }
    }
}

// One saved variable; `length` is 0 for a single value and the array's length otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateEntry {
    pub key: String,
    pub kind: ValueKind,
    pub length: u16,
    pub width: u8,
    pub values: Vec<i64>,
}

// ============================================================================
// Reading and writing the bytes
// ============================================================================

pub fn decode(bytes: &[u8]) -> Result<Vec<StateEntry>, String> {
    if bytes.len() < HEADER_BYTES || &bytes[..4] != MAGIC || bytes[4] != VERSION {
        return Err("not an app's saved state".into());
    }
    let mut entries = Vec::new();
    let mut at = HEADER_BYTES;
    for _ in 0..bytes[5] {
        let short = || "the saved state stops part way".to_string();
        let key_length = *bytes.get(at).ok_or_else(short)? as usize;
        let key = bytes.get(at + 1..at + 1 + key_length).ok_or_else(short)?;
        let header = bytes
            .get(at + 1 + key_length..at + 5 + key_length)
            .ok_or_else(short)?;
        let kind = ValueKind::from_byte(header[0]).ok_or("the saved state has a value of no known type")?;
        let length = u16::from_le_bytes([header[1], header[2]]);
        let width = header[3];
        if !matches!(width, 1 | 2 | 4 | 8) {
            return Err(format!("the saved state stores a value in {width} bytes"));
        }
        let start = at + 5 + key_length;
        let end = start + length.max(1) as usize * width as usize;
        let data = bytes.get(start..end).ok_or_else(short)?;
        entries.push(StateEntry {
            key: String::from_utf8_lossy(key).into_owned(),
            kind,
            length,
            width,
            values: data.chunks(width as usize).map(read_packed).collect(),
        });
        at = end;
    }
    Ok(entries)
}

pub fn encode(entries: &[StateEntry]) -> Result<Vec<u8>, String> {
    let count = u8::try_from(entries.len()).map_err(|_| "too many values for one saved state")?;
    let mut out = Vec::from(&MAGIC[..]);
    out.extend([VERSION, count]);
    for entry in entries {
        let key = entry.key.as_bytes();
        out.push(u8::try_from(key.len()).map_err(|_| format!("the key {} is too long", entry.key))?);
        out.extend(key);
        out.push(entry.kind.byte());
        out.extend(entry.length.to_le_bytes());
        out.push(entry.width);
        for &value in &entry.values {
            write_packed(&mut out, value, entry.width)
                .ok_or_else(|| format!("{} cannot hold {value}", entry.key))?;
        }
    }
    Ok(out)
}

fn read_packed(bytes: &[u8]) -> i64 {
    match bytes.len() {
        1 => bytes[0] as i8 as i64,
        2 => i16::from_le_bytes([bytes[0], bytes[1]]) as i64,
        4 => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64,
        _ => i64::from_le_bytes(bytes.try_into().unwrap_or([0; 8])),
    }
}

fn write_packed(out: &mut Vec<u8>, value: i64, width: u8) -> Option<()> {
    match width {
        1 => out.extend(i8::try_from(value).ok()?.to_le_bytes()),
        2 => out.extend(i16::try_from(value).ok()?.to_le_bytes()),
        4 => out.extend(i32::try_from(value).ok()?.to_le_bytes()),
        8 => out.extend(value.to_le_bytes()),
        _ => return None,
    }
    Some(())
}

// ============================================================================
// As JSON
// ============================================================================

// A record's fields are saved as `name.field`, so they come back together as one object of arrays or values.
pub fn to_json(entries: &[StateEntry]) -> Value {
    let mut out = Map::new();
    for entry in entries {
        let value = entry_json(entry);
        match entry.key.split_once('.') {
            Some((record, field)) => {
                let slot = out.entry(record).or_insert_with(|| json!({}));
                if let Some(fields) = slot.as_object_mut() {
                    fields.insert(field.to_string(), value);
                }
            }
            None => {
                out.insert(entry.key.clone(), value);
            }
        }
    }
    Value::Object(out)
}

fn entry_json(entry: &StateEntry) -> Value {
    let one = |value: i64| match entry.kind {
        ValueKind::Bool => json!(value != 0),
        _ => json!(value),
    };
    if entry.length == 0 {
        entry.values.first().map_or(Value::Null, |&value| one(value))
    } else {
        Value::Array(entry.values.iter().map(|&value| one(value)).collect())
    }
}

// Takes new values from JSON shaped as `to_json` gives it; keys it leaves out keep their values.
//
// The types, lengths and widths stay as the app saved them, so the watch still loads the result.
pub fn apply_json(entries: &[StateEntry], changes: &Value) -> Result<Vec<StateEntry>, String> {
    let mut out = entries.to_vec();
    for entry in &mut out {
        let change = match entry.key.split_once('.') {
            Some((record, field)) => changes.get(record).and_then(|fields| fields.get(field)),
            None => changes.get(&entry.key),
        };
        let Some(change) = change else { continue };
        let values: Vec<i64> = if entry.length == 0 {
            vec![number(change, &entry.key)?]
        } else {
            let items = change
                .as_array()
                .ok_or_else(|| format!("{} is a list of {}", entry.key, entry.length))?;
            if items.len() != entry.length as usize {
                return Err(format!("{} is a list of {}", entry.key, entry.length));
            }
            items
                .iter()
                .map(|item| number(item, &entry.key))
                .collect::<Result<_, _>>()?
        };
        for &value in &values {
            write_packed(&mut Vec::new(), value, entry.width)
                .ok_or_else(|| format!("{} cannot hold {value}", entry.key))?;
        }
        entry.values = values;
    }
    Ok(out)
}

fn number(value: &Value, key: &str) -> Result<i64, String> {
    match value {
        Value::Bool(on) => Ok(*on as i64),
        _ => value
            .as_i64()
            .ok_or_else(|| format!("{key} takes whole numbers, not {value}")),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // The alarm app's state, cut down: a count and one record array of two.
    pub(crate) fn alarm_state() -> Vec<StateEntry> {
        vec![
            StateEntry {
                key: "alarm_count".into(),
                kind: ValueKind::Int,
                length: 0,
                width: 1,
                values: vec![2],
            },
            StateEntry {
                key: "alarms.hour".into(),
                kind: ValueKind::Int,
                length: 2,
                width: 1,
                values: vec![7, 9],
            },
            StateEntry {
                key: "alarms.enabled".into(),
                kind: ValueKind::Bool,
                length: 2,
                width: 1,
                values: vec![1, 0],
            },
            StateEntry {
                key: "next_ring_at".into(),
                kind: ValueKind::Instant,
                length: 0,
                width: 8,
                values: vec![1_760_000_000_000],
            },
        ]
    }

    #[test]
    fn saved_state_reads_back_as_it_was_written() {
        let bytes = encode(&alarm_state()).unwrap();
        assert_eq!(&bytes[..6], b"NTST\x01\x04");
        assert_eq!(decode(&bytes).unwrap(), alarm_state());
    }

    #[test]
    fn it_reads_the_bytes_the_runtime_writes() {
        // One i16 under "n": key length, key, type 0, length 0, width 2, then -2.
        let bytes = b"NTST\x01\x01\x01n\x00\x00\x00\x02\xfe\xff";
        let entries = decode(bytes).unwrap();
        assert_eq!(entries[0].values, vec![-2]);
        assert_eq!(to_json(&entries), json!({ "n": -2 }));
    }

    #[test]
    fn records_come_back_together_and_switches_as_booleans() {
        assert_eq!(
            to_json(&alarm_state()),
            json!({
                "alarm_count": 2,
                "alarms": { "hour": [7, 9], "enabled": [true, false] },
                "next_ring_at": 1_760_000_000_000_i64,
            })
        );
    }

    #[test]
    fn anything_else_is_refused() {
        assert!(decode(b"").is_err());
        assert!(decode(b"NTST\x02\x00").is_err());
        assert!(decode(b"NTST\x01\x01\x01n\x00\x00\x00\x02\xfe").is_err());
        assert!(decode(b"NTST\x01\x01\x01n\x09\x00\x00\x02\xfe\xff").is_err());
        assert!(decode(b"NTST\x01\x01\x01n\x00\x00\x00\x03\xfe\xff\x00").is_err());
    }

    #[test]
    fn changes_keep_the_layout_and_leave_the_rest() {
        let changed = apply_json(
            &alarm_state(),
            &json!({ "alarms": { "hour": [6, 9], "enabled": [true, true] } }),
        )
        .unwrap();
        assert_eq!(changed[1].values, vec![6, 9]);
        assert_eq!(changed[2].values, vec![1, 1]);
        assert_eq!(changed[0], alarm_state()[0]);
        assert_eq!(changed[3], alarm_state()[3]);
    }

    #[test]
    fn a_change_that_would_not_load_is_refused() {
        let state = alarm_state();
        assert!(apply_json(&state, &json!({ "alarm_count": 300 })).is_err());
        assert!(apply_json(&state, &json!({ "alarm_count": "two" })).is_err());
        assert!(apply_json(&state, &json!({ "alarms": { "hour": [6] } })).is_err());
        assert!(apply_json(&state, &json!({ "alarms": { "hour": 6 } })).is_err());
    }
}
