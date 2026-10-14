// Turns the watch's UART bytes into text without breaking a character split across two notifications.

// ============================================================================
// UTF-8 across notifications
// ============================================================================

#[derive(Default)]
pub struct Utf8Joiner {
    carried_bytes: Vec<u8>,
}

impl Utf8Joiner {
    // Gives the text in `bytes`, holding back an unfinished character until the next call.
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.carried_bytes.extend_from_slice(bytes);
        let complete_length = complete_prefix_length(&self.carried_bytes);
        let complete_bytes: Vec<u8> = self.carried_bytes.drain(..complete_length).collect();
        String::from_utf8_lossy(&complete_bytes).into_owned()
    }

    pub fn clear(&mut self) {
        self.carried_bytes.clear();
    }
}

// How many leading bytes end on a character boundary; at most three trailing bytes are held back.
fn complete_prefix_length(bytes: &[u8]) -> usize {
    let length = bytes.len();
    for back in 1..=3.min(length) {
        let byte = bytes[length - back];
        if byte & 0b1100_0000 == 0b1000_0000 {
            continue;
        }
        let needed = match byte {
            byte if byte & 0b1110_0000 == 0b1100_0000 => 2,
            byte if byte & 0b1111_0000 == 0b1110_0000 => 3,
            byte if byte & 0b1111_1000 == 0b1111_0000 => 4,
            _ => 1,
        };
        return if needed > back { length - back } else { length };
    }
    length
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_ascii_passes_straight_through() {
        let mut joiner = Utf8Joiner::default();
        assert_eq!(joiner.push(b"GB({})\n"), "GB({})\n");
    }

    #[test]
    fn a_character_split_across_two_pushes_comes_out_whole() {
        let euro = "€".as_bytes();
        let mut joiner = Utf8Joiner::default();
        assert_eq!(joiner.push(&[b'a', euro[0], euro[1]]), "a");
        assert_eq!(joiner.push(&[euro[2], b'b']), "€b");
    }

    #[test]
    fn a_four_byte_character_split_after_one_byte_comes_out_whole() {
        let emoji = "😀".as_bytes();
        let mut joiner = Utf8Joiner::default();
        assert_eq!(joiner.push(&emoji[..1]), "");
        assert_eq!(joiner.push(&emoji[1..]), "😀");
    }

    #[test]
    fn invalid_bytes_become_replacement_characters_instead_of_stalling() {
        let mut joiner = Utf8Joiner::default();
        assert_eq!(joiner.push(&[0xff, b'x']), "\u{fffd}x");
    }

    #[test]
    fn clearing_drops_a_held_back_partial_character() {
        let mut joiner = Utf8Joiner::default();
        joiner.push(&"€".as_bytes()[..2]);
        joiner.clear();
        assert_eq!(joiner.push(b"ok"), "ok");
    }
}
