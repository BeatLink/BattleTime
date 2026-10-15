use super::*;
use crate::text::WrappedLines;

// ============================================================================
// Helpers
// ============================================================================

const WIDTH: i32 = 224;

/// A reader over bytes held in memory, as the watch's storage would give them.
fn reader(file: &[u8]) -> impl FnMut(u32, &mut [u8]) -> Option<usize> + '_ {
    move |offset, out| {
        let rest = file.get(offset as usize..).unwrap_or(&[]);
        let length = rest.len().min(out.len());
        out[..length].copy_from_slice(&rest[..length]);
        Some(length)
    }
}

/// Every line `for_each_line` gives from the start of the file.
fn lines_of(file: &[u8], font: FontName) -> Vec<String> {
    let mut read = reader(file);
    let mut lines = Vec::new();
    for_each_line(&mut read, 0, font, WIDTH, &mut |line, _| {
        lines.push(String::from(line));
        true
    });
    lines
}

/// A long file of numbered paragraphs, several chunks long.
fn long_file() -> String {
    let mut text = String::new();
    for paragraph in 0..60 {
        text.push_str(&alloc::format!(
            "Paragraph {paragraph} holds some words that wrap across more than one line of the screen.\n"
        ));
        if paragraph % 7 == 0 {
            text.push('\n');
        }
    }
    text
}

// ============================================================================
// Wrapping a file in chunks
// ============================================================================

#[test]
fn a_file_read_in_chunks_wraps_as_the_whole_text_would() {
    let file = long_file();
    assert!(file.len() > 4 * CHUNK_BYTES);
    for font in [FontName::Small, FontName::Normal, FontName::Large] {
        let whole: Vec<String> = WrappedLines::new(&file, font, WIDTH).map(String::from).collect();
        assert_eq!(lines_of(file.as_bytes(), font), whole);
    }
}

#[test]
fn each_line_says_where_the_next_one_starts() {
    let file = long_file();
    let mut read = reader(file.as_bytes());
    let mut starts = Vec::new();
    for_each_line(&mut read, 0, FontName::Normal, WIDTH, &mut |_, next_start| {
        starts.push(next_start);
        true
    });
    let expected: Vec<String> = WrappedLines::new(&file, FontName::Normal, WIDTH)
        .map(String::from)
        .collect();
    for (line, start) in starts.iter().enumerate().take(expected.len() - 1) {
        let mut read = reader(file.as_bytes());
        let mut first = None;
        for_each_line(&mut read, *start, FontName::Normal, WIDTH, &mut |text, _| {
            first = Some(String::from(text));
            false
        });
        assert_eq!(
            first.as_deref(),
            Some(expected[line + 1].as_str()),
            "line {}",
            line + 1
        );
    }
}

#[test]
fn an_empty_or_unreadable_file_has_no_lines() {
    assert!(lines_of(b"", FontName::Normal).is_empty());
    let mut missing = |_: u32, _: &mut [u8]| None;
    let index = LineIndex::build(&mut missing, FontName::Normal, WIDTH, 100);
    assert_eq!(index.line_count, 0);
    assert_eq!(index.read_lines(&mut missing, 0, 10), (0, String::new()));
}

#[test]
fn tabs_and_carriage_returns_become_spaces_and_bad_bytes_become_question_marks() {
    let lines = lines_of(b"one\ttwo\r\nthree \xff four", FontName::Normal);
    assert_eq!(lines, ["one two", "three ? four"]);
}

#[test]
fn a_character_cut_by_a_chunk_is_read_whole_with_the_next() {
    let mut file = alloc::vec![b'a'; CHUNK_BYTES - 1];
    for chunk_end in [10, 60, 120, 200, 300, 400, 500] {
        file[chunk_end] = b'\n';
    }
    file.extend_from_slice("é and more\n".as_bytes());
    let lines = lines_of(&file, FontName::Small);
    let text = core::str::from_utf8(&file).unwrap();
    let whole: Vec<String> = WrappedLines::new(text, FontName::Small, WIDTH)
        .map(String::from)
        .collect();
    assert_eq!(lines, whole);
    assert!(lines.last().unwrap().ends_with("é and more"));
}

#[test]
fn a_cut_character_at_the_very_end_becomes_a_question_mark() {
    assert_eq!(lines_of(b"end \xc3", FontName::Normal), ["end ?"]);
}

// ============================================================================
// The index
// ============================================================================

#[test]
fn the_index_counts_lines_and_reads_back_any_stretch_of_them() {
    let file = long_file();
    let whole: Vec<String> = WrappedLines::new(&file, FontName::Normal, WIDTH)
        .map(String::from)
        .collect();
    let mut read = reader(file.as_bytes());
    let index = LineIndex::build(&mut read, FontName::Normal, WIDTH, 10_000);
    assert_eq!(index.line_count, whole.len() as i32);

    for first in [0, 5, 37, 100, whole.len() as i32 - 3] {
        let (window_first, text) = index.read_lines(&mut read, first, first + 12);
        assert!(window_first <= first);
        let held: Vec<&str> = text.split('\n').collect();
        assert!(covers(
            window_first,
            held.len() as i32,
            first,
            first + 12,
            index.line_count
        ));
        for (place, line) in held.iter().enumerate() {
            assert_eq!(*line, whole[window_first as usize + place]);
        }
    }
}

#[test]
fn the_index_stops_at_the_most_lines_allowed() {
    let file = long_file();
    let mut read = reader(file.as_bytes());
    let index = LineIndex::build(&mut read, FontName::Normal, WIDTH, 20);
    assert_eq!(index.line_count, 20);
    let (_, text) = index.read_lines(&mut read, 15, 30);
    assert!(text.split('\n').count() <= 20);
    assert_eq!(
        LineIndex::build(&mut read, FontName::Normal, WIDTH, 0).line_count,
        0
    );
}

#[test]
fn held_lines_cover_a_view_that_runs_past_the_last_line() {
    assert!(covers(0, 10, 2, 30, 10));
    assert!(!covers(4, 10, 2, 8, 100));
    assert!(!covers(0, 10, 2, 12, 100));
}

// ============================================================================
// The font file
// ============================================================================

#[test]
fn a_font_file_names_one_of_the_text_fonts() {
    let font = |bytes: &[u8]| font_from_file(&mut reader(bytes));
    assert_eq!(font(b"small"), Some(FontName::Small));
    assert_eq!(font(b"normal\n"), Some(FontName::Normal));
    assert_eq!(font(b" medium "), Some(FontName::Medium));
    assert_eq!(font(b"large"), Some(FontName::Large));
    assert_eq!(font(b"giant"), None);
    assert_eq!(font(b"\xff"), None);
    assert_eq!(font_from_file(&mut |_: u32, _: &mut [u8]| None), None);
}

#[test]
fn the_lines_held_for_a_full_screen_of_dense_small_text_stay_under_two_kilobytes() {
    let file = "iiii lll jjjj ".repeat(2000);
    let mut read = reader(file.as_bytes());
    let index = LineIndex::build(&mut read, FontName::Small, WIDTH, 10_000);
    let visible = 240 / crate::fonts::line_height(FontName::Small.metrics()) + 1;

    for first in [0, 7, 100, 333] {
        let (_, text) = index.read_lines(&mut read, first, first + visible);
        assert!(text.len() < 2048, "{} bytes from line {first}", text.len());
    }
}
