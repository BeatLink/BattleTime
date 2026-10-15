extern crate std;

use alloc::{string::String, vec, vec::Vec};

use battletime_flash_store::{ram_flash::RamFlash, FlashStore, Layout};
use embassy_futures::block_on;

use super::*;

// ============================================================================
// A small store in memory
// ============================================================================

type TestStore = FlashStore<RamFlash, 16, 32, 8>;

const LAYOUT: Layout = Layout {
    start_address: 0,
    file_sectors: 16,
    record_sectors: 8,
    log_sectors: 4,
};
/// Room for a notification at a 64-byte MTU.
const ROOM: usize = 61;

fn store() -> TestStore {
    block_on(TestStore::mount(RamFlash::blank(LAYOUT.total_sectors()), LAYOUT)).unwrap()
}

fn put(files: &mut TestStore, path: &str, bytes: &[u8]) {
    block_on(FileAccess::write_file(files, path, bytes)).unwrap();
}

fn get(files: &mut TestStore, path: &str) -> Option<Vec<u8>> {
    let length = block_on(FileAccess::file_length(files, path)).unwrap()?;
    let mut bytes = vec![0; length];
    block_on(FileAccess::read_file(files, path, 0, &mut bytes)).unwrap();
    Some(bytes)
}

fn answers(transfer: &mut FileTransfer, files: &mut TestStore, request: &[u8]) -> Vec<Vec<u8>> {
    block_on(transfer.answer(Some(files), request, ROOM))
}

fn one_answer(transfer: &mut FileTransfer, files: &mut TestStore, request: &[u8]) -> Vec<u8> {
    let mut answers = answers(transfer, files, request);
    assert_eq!(answers.len(), 1);
    answers.remove(0)
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    read_u32(bytes, at)
}

// ============================================================================
// Building commands, laid out as the protocol describes
// ============================================================================

fn read_request(path: &str, chunk_offset: u32, chunk_size: u32) -> Vec<u8> {
    let mut request = vec![command::READ, 0];
    request.extend_from_slice(&(path.len() as u16).to_le_bytes());
    request.extend_from_slice(&chunk_offset.to_le_bytes());
    request.extend_from_slice(&chunk_size.to_le_bytes());
    request.extend_from_slice(path.as_bytes());
    request
}

fn read_pacing_request(chunk_offset: u32, chunk_size: u32) -> Vec<u8> {
    let mut request = vec![command::READ_PACING, status::OK, 0, 0];
    request.extend_from_slice(&chunk_offset.to_le_bytes());
    request.extend_from_slice(&chunk_size.to_le_bytes());
    request
}

fn write_request(path: &str, offset: u32, total_size: u32) -> Vec<u8> {
    let mut request = vec![command::WRITE, 0];
    request.extend_from_slice(&(path.len() as u16).to_le_bytes());
    request.extend_from_slice(&offset.to_le_bytes());
    request.extend_from_slice(&0u64.to_le_bytes());
    request.extend_from_slice(&total_size.to_le_bytes());
    request.extend_from_slice(path.as_bytes());
    request
}

fn write_data_request(offset: u32, data: &[u8]) -> Vec<u8> {
    let mut request = vec![command::WRITE_DATA, status::OK, 0, 0];
    request.extend_from_slice(&offset.to_le_bytes());
    request.extend_from_slice(&(data.len() as u32).to_le_bytes());
    request.extend_from_slice(data);
    request
}

fn path_request(command_byte: u8, path: &str) -> Vec<u8> {
    let mut request = vec![command_byte, 0];
    request.extend_from_slice(&(path.len() as u16).to_le_bytes());
    request.extend_from_slice(path.as_bytes());
    request
}

fn make_directory_request(path: &str) -> Vec<u8> {
    let mut request = vec![command::MAKE_DIRECTORY, 0];
    request.extend_from_slice(&(path.len() as u16).to_le_bytes());
    request.extend_from_slice(&[0; 4]);
    request.extend_from_slice(&0u64.to_le_bytes());
    request.extend_from_slice(path.as_bytes());
    request
}

fn move_request(old_path: &str, new_path: &str) -> Vec<u8> {
    let mut request = vec![command::MOVE, 0];
    request.extend_from_slice(&(old_path.len() as u16).to_le_bytes());
    request.extend_from_slice(&(new_path.len() as u16).to_le_bytes());
    request.extend_from_slice(old_path.as_bytes());
    request.push(0);
    request.extend_from_slice(new_path.as_bytes());
    request
}

const PIECE: usize = ROOM - WRITE_DATA_HEADER_SIZE;

/// Uploads `bytes` the way a phone does: a write, then pieces that fit the room.
fn upload(transfer: &mut FileTransfer, files: &mut TestStore, path: &str, bytes: &[u8]) {
    let answer = one_answer(transfer, files, &write_request(path, 0, bytes.len() as u32));
    assert_eq!(answer[1], status::OK);
    for (index, piece) in bytes.chunks(PIECE).enumerate() {
        let answer = one_answer(
            transfer,
            files,
            &write_data_request((index * PIECE) as u32, piece),
        );
        assert_eq!(answer[1], status::OK);
    }
}

/// Downloads a file the way a phone does: a read, then pacing until the total arrives.
fn download(transfer: &mut FileTransfer, files: &mut TestStore, path: &str) -> Vec<u8> {
    let mut answer = one_answer(transfer, files, &read_request(path, 0, 1000));
    let mut received = Vec::new();
    loop {
        assert_eq!(answer[0], command::READ_DATA);
        assert_eq!(answer[1], status::OK);
        let total = u32_at(&answer, 8) as usize;
        let length = u32_at(&answer, 12) as usize;
        received.extend_from_slice(&answer[READ_DATA_HEADER_SIZE..READ_DATA_HEADER_SIZE + length]);
        if received.len() >= total {
            return received;
        }
        answer = one_answer(transfer, files, &read_pacing_request(received.len() as u32, 1000));
    }
}

fn sample_bytes(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index * 7 % 251) as u8).collect()
}

// ============================================================================
// Reading
// ============================================================================

#[test]
fn reads_a_file_in_chunks_that_fit_the_notification() {
    let mut files = store();
    let bytes = sample_bytes(3000);
    put(&mut files, "/notes.txt", &bytes);
    let mut transfer = FileTransfer::new();

    let first = one_answer(&mut transfer, &mut files, &read_request("/notes.txt", 0, 1000));

    assert_eq!(first.len(), ROOM);
    assert_eq!(u32_at(&first, 4), 0);
    assert_eq!(u32_at(&first, 8), 3000);
    assert_eq!(u32_at(&first, 12) as usize, ROOM - READ_DATA_HEADER_SIZE);
    assert_eq!(download(&mut transfer, &mut files, "/notes.txt"), bytes);
}

#[test]
fn a_read_never_sends_more_than_the_phone_asked_for() {
    let mut files = store();
    put(&mut files, "/notes.txt", &sample_bytes(100));
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &read_request("/notes.txt", 10, 5));

    assert_eq!(u32_at(&answer, 4), 10);
    assert_eq!(u32_at(&answer, 12), 5);
    assert_eq!(&answer[READ_DATA_HEADER_SIZE..], &sample_bytes(100)[10..15]);
}

#[test]
fn reading_an_empty_file_sends_one_empty_chunk() {
    let mut files = store();
    put(&mut files, "/empty", b"");
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &read_request("/empty", 0, 100));
    let pacing = one_answer(&mut transfer, &mut files, &read_pacing_request(0, 100));

    assert_eq!(answer[1], status::OK);
    assert_eq!(u32_at(&answer, 8), 0);
    assert_eq!(u32_at(&answer, 12), 0);
    assert_eq!(pacing[1], status::PROTOCOL_ERROR);
}

#[test]
fn reading_a_missing_file_reports_no_file() {
    let mut files = store();
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &read_request("/missing", 0, 100));

    assert_eq!(answer[0], command::READ_DATA);
    assert_eq!(answer[1], status::NO_FILE);
    assert_eq!(answer.len(), READ_DATA_HEADER_SIZE);
}

#[test]
fn reading_a_folder_is_an_error() {
    let mut files = store();
    put(&mut files, "/apps/clock", b"x");
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &read_request("/apps", 0, 100));

    assert_eq!(answer[1], status::ERROR);
}

#[test]
fn pacing_without_a_read_is_a_protocol_error() {
    let mut files = store();
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &read_pacing_request(0, 100));

    assert_eq!(answer[0], command::READ_DATA);
    assert_eq!(answer[1], status::PROTOCOL_ERROR);
}

#[test]
fn short_or_garbled_reads_are_protocol_errors() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    let mut cut_path = read_request("/notes.txt", 0, 10);
    cut_path.truncate(cut_path.len() - 2);
    let mut not_text = read_request("/ab", 0, 10);
    let last = not_text.len() - 1;
    not_text[last] = 0xFF;

    assert_eq!(
        one_answer(&mut transfer, &mut files, &[command::READ, 0, 0])[1],
        status::PROTOCOL_ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &cut_path)[1],
        status::PROTOCOL_ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &not_text)[1],
        status::PROTOCOL_ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &[command::READ_PACING])[1],
        status::PROTOCOL_ERROR
    );
}

#[test]
fn a_read_past_the_end_sends_nothing_and_finishes() {
    let mut files = store();
    put(&mut files, "/notes.txt", b"hello");
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &read_request("/notes.txt", 50, 10));

    assert_eq!(answer[1], status::OK);
    assert_eq!(u32_at(&answer, 12), 0);
}

#[test]
fn a_file_deleted_between_chunks_reports_no_file() {
    let mut files = store();
    put(&mut files, "/notes.txt", &sample_bytes(200));
    let mut transfer = FileTransfer::new();
    one_answer(&mut transfer, &mut files, &read_request("/notes.txt", 0, 1000));

    block_on(FileAccess::delete_file(&mut files, "/notes.txt")).unwrap();
    let answer = one_answer(&mut transfer, &mut files, &read_pacing_request(45, 1000));

    assert_eq!(answer[1], status::NO_FILE);
}

// ============================================================================
// Writing
// ============================================================================

#[test]
fn writes_a_small_file_in_pieces_and_reads_it_back() {
    let mut files = store();
    let bytes = sample_bytes(1000);
    let mut transfer = FileTransfer::new();

    upload(&mut transfer, &mut files, "/apps/clock/state", &bytes);

    assert!(!transfer.is_writing());
    assert_eq!(get(&mut files, "/apps/clock/state").unwrap(), bytes);
    assert_eq!(download(&mut transfer, &mut files, "/apps/clock/state"), bytes);
}

#[test]
fn writes_a_large_file_in_pieces_and_reads_it_back() {
    let mut files = store();
    let bytes = sample_bytes(9000);
    let mut transfer = FileTransfer::new();

    upload(&mut transfer, &mut files, "/apps/clock/app.ntapp", &bytes);

    assert!(!transfer.is_writing());
    assert_eq!(get(&mut files, "/apps/clock/app.ntapp").unwrap(), bytes);
}

#[test]
fn the_old_file_stays_until_the_last_piece_arrives() {
    let mut files = store();
    put(&mut files, "/a", &sample_bytes(2000));
    let mut transfer = FileTransfer::new();

    one_answer(&mut transfer, &mut files, &write_request("/a", 0, 5000));
    one_answer(&mut transfer, &mut files, &write_data_request(0, &[1; 40]));

    assert_eq!(get(&mut files, "/a").unwrap(), sample_bytes(2000));
}

#[test]
fn write_answers_give_the_next_offset_and_what_may_follow() {
    let mut files = store();
    let mut transfer = FileTransfer::new();

    let start = one_answer(&mut transfer, &mut files, &write_request("/a", 0, 10));
    let middle = one_answer(&mut transfer, &mut files, &write_data_request(0, b"abcd"));
    let still_writing = transfer.is_writing();
    let end = one_answer(&mut transfer, &mut files, &write_data_request(4, b"efghij"));

    assert_eq!(start[0], command::WRITE_PACING);
    assert_eq!(start.len(), WRITE_PACING_SIZE);
    assert_eq!(u32_at(&start, 4), 0);
    assert_eq!(u32_at(&start, 16), 10);
    assert_eq!(u32_at(&middle, 4), 4);
    assert_eq!(u32_at(&middle, 16), 6);
    assert!(still_writing);
    assert_eq!(u32_at(&end, 4), 10);
    assert_eq!(u32_at(&end, 16), 0);
    assert!(!transfer.is_writing());
}

#[test]
fn writing_replaces_a_longer_file() {
    let mut files = store();
    put(&mut files, "/a", &sample_bytes(4000));
    let mut transfer = FileTransfer::new();

    upload(&mut transfer, &mut files, "/a", b"short");

    assert_eq!(get(&mut files, "/a").unwrap(), b"short");
}

#[test]
fn an_empty_write_leaves_an_empty_file() {
    let mut files = store();
    put(&mut files, "/a", b"old");
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &write_request("/a", 0, 0));

    assert_eq!(answer[1], status::OK);
    assert_eq!(u32_at(&answer, 16), 0);
    assert!(!transfer.is_writing());
    assert_eq!(get(&mut files, "/a").unwrap(), b"");
}

#[test]
fn an_interrupted_large_upload_resumes_at_its_offset() {
    let mut files = store();
    let bytes = sample_bytes(3000);
    let mut transfer = FileTransfer::new();
    one_answer(&mut transfer, &mut files, &write_request("/a", 0, 3000));
    one_answer(&mut transfer, &mut files, &write_data_request(0, &bytes[..40]));

    transfer.abort();
    let resumed = one_answer(&mut transfer, &mut files, &write_request("/a", 40, 3000));
    for (index, piece) in bytes[40..].chunks(PIECE).enumerate() {
        one_answer(
            &mut transfer,
            &mut files,
            &write_data_request((40 + index * PIECE) as u32, piece),
        );
    }

    assert_eq!(resumed[1], status::OK);
    assert_eq!(u32_at(&resumed, 4), 40);
    assert_eq!(u32_at(&resumed, 16), 2960);
    assert_eq!(get(&mut files, "/a").unwrap(), bytes);
}

#[test]
fn resuming_what_the_store_does_not_hold_open_is_refused() {
    let mut files = store();
    put(&mut files, "/a", b"abc");
    let mut transfer = FileTransfer::new();
    one_answer(&mut transfer, &mut files, &write_request("/b", 0, 3000));
    one_answer(&mut transfer, &mut files, &write_data_request(0, &[7; 40]));

    let wrong_offset = one_answer(&mut transfer, &mut files, &write_request("/b", 10, 3000));
    let other_file = one_answer(&mut transfer, &mut files, &write_request("/a", 2, 20));

    assert_eq!(wrong_offset[1], status::ERROR);
    assert_eq!(other_file[1], status::ERROR);
    assert!(!transfer.is_writing());
    assert_eq!(get(&mut files, "/a").unwrap(), b"abc");
}

#[test]
fn an_offset_beyond_the_total_is_a_protocol_error() {
    let mut files = store();
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &write_request("/a", 11, 10));

    assert_eq!(answer[1], status::PROTOCOL_ERROR);
}

#[test]
fn data_without_a_write_is_a_protocol_error() {
    let mut files = store();
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &write_data_request(0, b"abc"));

    assert_eq!(answer[0], command::WRITE_PACING);
    assert_eq!(answer[1], status::PROTOCOL_ERROR);
}

#[test]
fn data_past_the_total_or_after_a_gap_is_a_protocol_error() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    one_answer(&mut transfer, &mut files, &write_request("/a", 0, 4));

    let too_long = one_answer(&mut transfer, &mut files, &write_data_request(0, b"abcdef"));
    let gap = one_answer(&mut transfer, &mut files, &write_data_request(2, b"cd"));

    assert_eq!(too_long[1], status::PROTOCOL_ERROR);
    assert_eq!(gap[1], status::PROTOCOL_ERROR);
    assert!(transfer.is_writing());
}

#[test]
fn a_small_piece_sent_again_is_written_again() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    one_answer(&mut transfer, &mut files, &write_request("/a", 0, 6));

    one_answer(&mut transfer, &mut files, &write_data_request(0, b"abc"));
    one_answer(&mut transfer, &mut files, &write_data_request(0, b"xyz"));
    one_answer(&mut transfer, &mut files, &write_data_request(3, b"def"));

    assert_eq!(get(&mut files, "/a").unwrap(), b"xyzdef");
}

#[test]
fn a_large_piece_sent_again_adds_only_what_is_new() {
    let mut files = store();
    let bytes = sample_bytes(2000);
    let mut transfer = FileTransfer::new();
    one_answer(&mut transfer, &mut files, &write_request("/a", 0, 2000));

    one_answer(&mut transfer, &mut files, &write_data_request(0, &bytes[..40]));
    let again = one_answer(&mut transfer, &mut files, &write_data_request(20, &bytes[20..48]));
    for (index, piece) in bytes[48..].chunks(PIECE).enumerate() {
        one_answer(
            &mut transfer,
            &mut files,
            &write_data_request((48 + index * PIECE) as u32, piece),
        );
    }

    assert_eq!(u32_at(&again, 4), 48);
    assert_eq!(get(&mut files, "/a").unwrap(), bytes);
}

#[test]
fn a_stated_size_longer_than_the_data_uses_what_arrived() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    one_answer(&mut transfer, &mut files, &write_request("/a", 0, 6));
    let mut request = write_data_request(0, b"abc");
    request[8] = 50;

    let answer = one_answer(&mut transfer, &mut files, &request);

    assert_eq!(answer[1], status::OK);
    assert_eq!(u32_at(&answer, 4), 3);
}

#[test]
fn short_or_garbled_writes_are_protocol_errors() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    let mut cut_path = write_request("/abc", 0, 3);
    cut_path.pop();

    assert_eq!(
        one_answer(&mut transfer, &mut files, &[command::WRITE, 0])[1],
        status::PROTOCOL_ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &[command::WRITE_DATA, 0])[1],
        status::PROTOCOL_ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &cut_path)[1],
        status::PROTOCOL_ERROR
    );
}

#[test]
fn writing_where_a_folder_stands_is_an_error() {
    let mut files = store();
    put(&mut files, "/apps/clock", b"x");
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &write_request("/apps", 0, 3));

    assert_eq!(answer[1], status::ERROR);
}

#[test]
fn free_space_limits_what_may_follow() {
    let mut files = store();
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &write_request("/big", 0, 10_000_000));

    let free = u32_at(&answer, 16) as usize;
    assert!(free < LAYOUT.file_sectors * 4096);
    assert!(free > 0);
}

#[test]
fn a_full_store_stops_the_upload_and_keeps_the_old_file() {
    let mut files = store();
    put(&mut files, "/big", b"old");
    let mut transfer = FileTransfer::new();
    one_answer(&mut transfer, &mut files, &write_request("/big", 0, 200_000));
    let piece = vec![0x5A; 4000];
    let mut offset = 0;

    let failed = loop {
        let answer = one_answer(&mut transfer, &mut files, &write_data_request(offset, &piece));
        if answer[1] != status::OK {
            break answer;
        }
        offset += piece.len() as u32;
    };

    assert_eq!(failed[1], status::ERROR);
    assert!(!transfer.is_writing());
    assert_eq!(get(&mut files, "/big").unwrap(), b"old");
}

// ============================================================================
// Folders
// ============================================================================

#[test]
fn making_a_folder_succeeds_since_folders_live_in_paths() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    let mut cut_path = make_directory_request("/abc");
    cut_path.pop();

    let answer = one_answer(
        &mut transfer,
        &mut files,
        &make_directory_request("/resources/fonts/"),
    );

    assert_eq!(answer[0], command::MAKE_DIRECTORY_STATUS);
    assert_eq!(answer[1], status::OK);
    assert_eq!(answer.len(), MAKE_DIRECTORY_STATUS_SIZE);
    assert_eq!(
        one_answer(&mut transfer, &mut files, &make_directory_request("/"))[1],
        status::PROTOCOL_ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &[command::MAKE_DIRECTORY, 0, 0])[1],
        status::PROTOCOL_ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &cut_path)[1],
        status::PROTOCOL_ERROR
    );
}

#[test]
fn lists_a_folder_then_closes_the_list() {
    let mut files = store();
    put(&mut files, "/apps/clock/app.ntapp", b"code");
    put(&mut files, "/apps/clock/state", b"state");
    put(&mut files, "/apps/readme", b"hello");
    let mut transfer = FileTransfer::new();

    let answers = answers(
        &mut transfer,
        &mut files,
        &path_request(command::LIST_DIRECTORY, "/apps"),
    );

    assert_eq!(answers.len(), 3);
    let mut seen = Vec::new();
    for (index, answer) in answers[..2].iter().enumerate() {
        assert_eq!(answer[0], command::LIST_DIRECTORY_ENTRY);
        assert_eq!(answer[1], status::OK);
        assert_eq!(u32_at(answer, 4), index as u32);
        assert_eq!(u32_at(answer, 8), 2);
        let name_length = read_u16(answer, 2) as usize;
        let name =
            String::from_utf8(answer[LIST_DIRECTORY_ENTRY_HEADER_SIZE..][..name_length].to_vec()).unwrap();
        seen.push((name, u32_at(answer, 12), u32_at(answer, 24)));
    }
    seen.sort();
    assert_eq!(
        seen,
        vec![
            (String::from("clock"), IS_DIRECTORY, 0),
            (String::from("readme"), 0, 5)
        ]
    );
    let last = &answers[2];
    assert_eq!(u32_at(last, 4), 2);
    assert_eq!(u32_at(last, 8), 2);
    assert_eq!(read_u16(last, 2), 0);
    assert_eq!(last.len(), LIST_DIRECTORY_ENTRY_HEADER_SIZE);
}

#[test]
fn an_empty_path_lists_the_top_folder() {
    let mut files = store();
    put(&mut files, "/apps/clock", b"x");
    let mut transfer = FileTransfer::new();

    let answers = answers(
        &mut transfer,
        &mut files,
        &path_request(command::LIST_DIRECTORY, ""),
    );

    assert_eq!(answers.len(), 2);
    assert_eq!(&answers[0][LIST_DIRECTORY_ENTRY_HEADER_SIZE..], b"apps");
}

#[test]
fn a_long_name_is_cut_to_fit_the_notification() {
    let mut files = store();
    put(&mut files, &std::format!("/{}", "a".repeat(60)), b"");
    let mut transfer = FileTransfer::new();

    let answers = answers(
        &mut transfer,
        &mut files,
        &path_request(command::LIST_DIRECTORY, "/"),
    );

    assert_eq!(answers[0].len(), ROOM);
    assert_eq!(
        read_u16(&answers[0], 2) as usize,
        ROOM - LIST_DIRECTORY_ENTRY_HEADER_SIZE
    );
}

#[test]
fn listing_something_missing_or_garbled_is_one_failure() {
    let mut files = store();
    put(&mut files, "/file", b"x");
    let mut transfer = FileTransfer::new();
    let mut cut_path = path_request(command::LIST_DIRECTORY, "/abc");
    cut_path.pop();

    let missing = answers(
        &mut transfer,
        &mut files,
        &path_request(command::LIST_DIRECTORY, "/nope"),
    );
    let file = answers(
        &mut transfer,
        &mut files,
        &path_request(command::LIST_DIRECTORY, "/file"),
    );
    let short = answers(&mut transfer, &mut files, &[command::LIST_DIRECTORY, 0]);
    let garbled = answers(&mut transfer, &mut files, &cut_path);

    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0][1], status::NO_FILE);
    assert_eq!(file[0][1], status::ERROR);
    assert_eq!(short[0][1], status::PROTOCOL_ERROR);
    assert_eq!(garbled[0][1], status::PROTOCOL_ERROR);
}

// ============================================================================
// Deleting and moving
// ============================================================================

#[test]
fn deletes_a_file() {
    let mut files = store();
    put(&mut files, "/a", b"x");
    let mut transfer = FileTransfer::new();

    let answer = one_answer(&mut transfer, &mut files, &path_request(command::DELETE, "/a"));

    assert_eq!(answer, vec![command::DELETE_STATUS, status::OK]);
    assert_eq!(get(&mut files, "/a"), None);
}

#[test]
fn deletes_a_folder_with_everything_in_it() {
    let mut files = store();
    put(&mut files, "/apps/clock/app.ntapp", &sample_bytes(5000));
    put(&mut files, "/apps/clock/data/state", b"state");
    put(&mut files, "/apps/other", b"kept");
    let mut transfer = FileTransfer::new();

    let answer = one_answer(
        &mut transfer,
        &mut files,
        &path_request(command::DELETE, "/apps/clock/"),
    );

    assert_eq!(answer[1], status::OK);
    assert_eq!(get(&mut files, "/apps/clock/app.ntapp"), None);
    assert_eq!(get(&mut files, "/apps/clock/data/state"), None);
    assert_eq!(get(&mut files, "/apps/other").unwrap(), b"kept");
}

#[test]
fn deleting_the_top_folder_or_something_missing_fails() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    let mut cut_path = path_request(command::DELETE, "/abc");
    cut_path.pop();

    assert_eq!(
        one_answer(&mut transfer, &mut files, &path_request(command::DELETE, "/"))[1],
        status::ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &path_request(command::DELETE, "/nope"))[1],
        status::NO_FILE
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &[command::DELETE, 0])[1],
        status::PROTOCOL_ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &cut_path)[1],
        status::PROTOCOL_ERROR
    );
}

#[test]
fn moves_a_small_and_a_large_file() {
    let mut files = store();
    put(&mut files, "/a", b"hello");
    put(&mut files, "/big", &sample_bytes(6000));
    let mut transfer = FileTransfer::new();

    let small = one_answer(&mut transfer, &mut files, &move_request("/a", "/b"));
    let large = one_answer(&mut transfer, &mut files, &move_request("/big", "/moved"));

    assert_eq!(small, vec![command::MOVE_STATUS, status::OK]);
    assert_eq!(large, vec![command::MOVE_STATUS, status::OK]);
    assert_eq!(get(&mut files, "/b").unwrap(), b"hello");
    assert_eq!(get(&mut files, "/a"), None);
    assert_eq!(get(&mut files, "/moved").unwrap(), sample_bytes(6000));
    assert_eq!(get(&mut files, "/big"), None);
}

#[test]
fn moves_a_folder_with_everything_in_it() {
    let mut files = store();
    put(&mut files, "/apps/clock/app.ntapp", b"code");
    put(&mut files, "/apps/clock/state", b"state");
    let mut transfer = FileTransfer::new();

    let answer = one_answer(
        &mut transfer,
        &mut files,
        &move_request("/apps/clock", "/apps/watch"),
    );

    assert_eq!(answer[1], status::OK);
    assert_eq!(get(&mut files, "/apps/watch/app.ntapp").unwrap(), b"code");
    assert_eq!(get(&mut files, "/apps/watch/state").unwrap(), b"state");
    assert_eq!(get(&mut files, "/apps/clock/state"), None);
}

#[test]
fn moving_something_missing_or_garbled_fails() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    let mut cut_path = move_request("/a", "/bcd");
    cut_path.pop();

    assert_eq!(
        one_answer(&mut transfer, &mut files, &move_request("/a", "/b"))[1],
        status::NO_FILE
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &[command::MOVE, 0, 0])[1],
        status::PROTOCOL_ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &mut files, &cut_path)[1],
        status::PROTOCOL_ERROR
    );
}

// ============================================================================
// Locked folders
// ============================================================================

const LOCKED: &[&str] = &["/apps", "/temp"];

#[test]
fn a_locked_folder_refuses_every_change_but_still_reads() {
    let mut files = store();
    put(&mut files, "/apps/timer/app.ntapp", b"image");
    let mut transfer = FileTransfer::new();
    transfer.lock_folders(LOCKED);

    let write = one_answer(
        &mut transfer,
        &mut files,
        &write_request("/apps/timer/app.ntapp", 0, 3),
    );
    let delete = one_answer(
        &mut transfer,
        &mut files,
        &path_request(command::DELETE, "/apps/timer"),
    );
    let make = one_answer(&mut transfer, &mut files, &make_directory_request("/temp/new"));
    let move_out = one_answer(&mut transfer, &mut files, &move_request("/apps/timer", "/old"));
    let move_in = one_answer(&mut transfer, &mut files, &move_request("/notes/a", "/apps/a"));
    let read = one_answer(
        &mut transfer,
        &mut files,
        &read_request("/apps/timer/app.ntapp", 0, 5),
    );

    assert_eq!(write[1], status::ERROR);
    assert_eq!(delete[1], status::ERROR);
    assert_eq!(make[1], status::ERROR);
    assert_eq!(move_out[1], status::ERROR);
    assert_eq!(move_in[1], status::ERROR);
    assert_eq!(read[1], status::OK);
    assert_eq!(get(&mut files, "/apps/timer/app.ntapp").unwrap(), b"image");
}

#[test]
fn a_folder_holding_a_locked_one_cannot_be_deleted_or_moved_either() {
    let mut files = store();
    put(&mut files, "/apps/timer/app.ntapp", b"image");
    let mut transfer = FileTransfer::new();
    transfer.lock_folders(LOCKED);

    let delete_root = one_answer(&mut transfer, &mut files, &path_request(command::DELETE, "/"));
    let move_root = one_answer(&mut transfer, &mut files, &move_request("/", "/backup"));

    assert_eq!(delete_root[1], status::ERROR);
    assert_eq!(move_root[1], status::ERROR);
    assert!(get(&mut files, "/apps/timer/app.ntapp").is_some());
}

#[test]
fn files_beside_a_locked_folder_still_change() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    transfer.lock_folders(LOCKED);

    upload(&mut transfer, &mut files, "/notes/shopping", b"milk");
    upload(&mut transfer, &mut files, "/apps-list", b"x");

    assert_eq!(get(&mut files, "/notes/shopping").unwrap(), b"milk");
    assert_eq!(get(&mut files, "/apps-list").unwrap(), b"x");
}

#[test]
fn locking_part_way_through_an_upload_stops_it() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    let started = one_answer(
        &mut transfer,
        &mut files,
        &write_request("/apps/new/app.ntapp", 0, 4),
    );

    transfer.lock_folders(LOCKED);
    let piece = one_answer(&mut transfer, &mut files, &write_data_request(0, b"data"));

    assert_eq!(started[1], status::OK);
    assert_eq!(piece[1], status::ERROR);
    assert!(get(&mut files, "/apps/new/app.ntapp").is_none());
}

#[test]
fn data_without_a_write_is_still_a_protocol_error_while_locked() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    transfer.lock_folders(LOCKED);

    let answer = one_answer(&mut transfer, &mut files, &write_data_request(0, b"abc"));

    assert_eq!(answer[1], status::PROTOCOL_ERROR);
}

#[test]
fn a_move_names_its_new_path_by_its_length() {
    let mut files = store();
    put(&mut files, "/notes/a", b"note");
    let mut transfer = FileTransfer::new();
    transfer.lock_folders(LOCKED);
    let mut request = move_request("/notes/a", "/notes/b");
    request.extend_from_slice(b"/apps");

    let answer = one_answer(&mut transfer, &mut files, &request);

    assert_eq!(answer[1], status::OK);
    assert_eq!(get(&mut files, "/notes/b").unwrap(), b"note");
}

#[test]
fn unlocking_lets_changes_through_again() {
    let mut files = store();
    let mut transfer = FileTransfer::new();
    transfer.lock_folders(LOCKED);
    transfer.lock_folders(&[]);

    upload(&mut transfer, &mut files, "/apps/timer/app.ntapp", b"image");

    assert_eq!(get(&mut files, "/apps/timer/app.ntapp").unwrap(), b"image");
}

// ============================================================================
// Without a store, and unknown commands
// ============================================================================

#[test]
fn every_command_fails_with_its_own_answer_when_there_is_no_store() {
    let mut transfer = FileTransfer::default();
    let cases = [
        (command::READ, command::READ_DATA),
        (command::READ_PACING, command::READ_DATA),
        (command::WRITE, command::WRITE_PACING),
        (command::WRITE_DATA, command::WRITE_PACING),
        (command::DELETE, command::DELETE_STATUS),
        (command::MAKE_DIRECTORY, command::MAKE_DIRECTORY_STATUS),
        (command::LIST_DIRECTORY, command::LIST_DIRECTORY_ENTRY),
        (command::MOVE, command::MOVE_STATUS),
    ];

    for (request, reply) in cases {
        let answers = block_on(transfer.answer(None::<&mut TestStore>, &[request], ROOM));
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0][0], reply);
        assert_eq!(answers[0][1], status::ERROR);
    }
}

#[test]
fn a_build_without_a_store_refuses_each_command() {
    let refused = FileTransfer::refuse(&[command::DELETE]);

    assert_eq!(refused, vec![vec![command::DELETE_STATUS, status::ERROR]]);
    assert!(FileTransfer::refuse(&[]).is_empty());
}

#[test]
fn unknown_and_empty_commands_get_no_answer() {
    let mut files = store();
    let mut transfer = FileTransfer::new();

    assert!(answers(&mut transfer, &mut files, &[0x99]).is_empty());
    assert!(answers(&mut transfer, &mut files, &[]).is_empty());
    assert!(block_on(transfer.answer(None::<&mut TestStore>, &[0x99], ROOM)).is_empty());
}
