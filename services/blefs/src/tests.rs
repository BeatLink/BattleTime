extern crate std;

use alloc::{string::String, vec, vec::Vec};

use littlefs_rust::{Config, RamStorage};

use super::*;

// ============================================================================
// A small file system in memory
// ============================================================================

const BLOCK_SIZE: u32 = 512;
const BLOCK_COUNT: u32 = 64;
/// Room for a notification at a 64-byte MTU.
const ROOM: usize = 61;

fn file_system() -> Filesystem<RamStorage> {
    let config = Config::new(BLOCK_SIZE, BLOCK_COUNT);
    let mut storage = RamStorage::new(BLOCK_SIZE, BLOCK_COUNT);
    Filesystem::format(&mut storage, &config).unwrap();
    Filesystem::mount(storage, config)
        .map_err(|(error, _)| error)
        .unwrap()
}

fn transfer() -> FileTransfer {
    FileTransfer::new(BLOCK_SIZE, BLOCK_COUNT)
}

fn one_answer(transfer: &mut FileTransfer, file_system: &Filesystem<RamStorage>, request: &[u8]) -> Vec<u8> {
    let mut answers = transfer.answer(Some(file_system), request, ROOM);
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

/// Uploads `bytes` the way a phone does: a write, then pieces that fit the room.
fn upload(transfer: &mut FileTransfer, file_system: &Filesystem<RamStorage>, path: &str, bytes: &[u8]) {
    let answer = one_answer(transfer, file_system, &write_request(path, 0, bytes.len() as u32));
    assert_eq!(answer[1], status::OK);
    for (index, piece) in bytes.chunks(ROOM - WRITE_DATA_HEADER_SIZE).enumerate() {
        let offset = (index * (ROOM - WRITE_DATA_HEADER_SIZE)) as u32;
        let answer = one_answer(transfer, file_system, &write_data_request(offset, piece));
        assert_eq!(answer[1], status::OK);
    }
}

/// Downloads a file the way a phone does: a read, then pacing until the total arrives.
fn download(transfer: &mut FileTransfer, file_system: &Filesystem<RamStorage>, path: &str) -> Vec<u8> {
    let mut answer = one_answer(transfer, file_system, &read_request(path, 0, 1000));
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
        answer = one_answer(
            transfer,
            file_system,
            &read_pacing_request(received.len() as u32, 1000),
        );
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
    let file_system = file_system();
    let bytes = sample_bytes(300);
    file_system.write_file("/notes.txt", &bytes).unwrap();
    let mut transfer = transfer();

    let first = one_answer(&mut transfer, &file_system, &read_request("/notes.txt", 0, 1000));
    assert_eq!(first.len(), ROOM);
    assert_eq!(u32_at(&first, 4), 0);
    assert_eq!(u32_at(&first, 8), 300);
    assert_eq!(u32_at(&first, 12) as usize, ROOM - READ_DATA_HEADER_SIZE);

    assert_eq!(download(&mut transfer, &file_system, "/notes.txt"), bytes);
}

#[test]
fn a_read_never_sends_more_than_the_phone_asked_for() {
    let file_system = file_system();
    file_system.write_file("/notes.txt", &sample_bytes(100)).unwrap();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &read_request("/notes.txt", 10, 5));
    assert_eq!(u32_at(&answer, 4), 10);
    assert_eq!(u32_at(&answer, 12), 5);
    assert_eq!(&answer[READ_DATA_HEADER_SIZE..], &sample_bytes(100)[10..15]);
}

#[test]
fn reading_an_empty_file_sends_one_empty_chunk() {
    let file_system = file_system();
    file_system.write_file("/empty", b"").unwrap();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &read_request("/empty", 0, 100));
    assert_eq!(answer[1], status::OK);
    assert_eq!(u32_at(&answer, 8), 0);
    assert_eq!(u32_at(&answer, 12), 0);
    let pacing = one_answer(&mut transfer, &file_system, &read_pacing_request(0, 100));
    assert_eq!(pacing[1], status::PROTOCOL_ERROR);
}

#[test]
fn reading_a_missing_file_reports_no_file() {
    let file_system = file_system();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &read_request("/missing", 0, 100));
    assert_eq!(answer[0], command::READ_DATA);
    assert_eq!(answer[1], status::NO_FILE);
    assert_eq!(answer.len(), READ_DATA_HEADER_SIZE);
}

#[test]
fn reading_a_folder_is_an_error() {
    let file_system = file_system();
    file_system.mkdir("/apps").unwrap();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &read_request("/apps", 0, 100));
    assert_eq!(answer[1], status::ERROR);
}

#[test]
fn pacing_without_a_read_is_a_protocol_error() {
    let file_system = file_system();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &read_pacing_request(0, 100));
    assert_eq!(answer[0], command::READ_DATA);
    assert_eq!(answer[1], status::PROTOCOL_ERROR);
}

#[test]
fn short_or_garbled_reads_are_protocol_errors() {
    let file_system = file_system();
    let mut transfer = transfer();
    let short = one_answer(&mut transfer, &file_system, &[command::READ, 0, 0]);
    assert_eq!(short[1], status::PROTOCOL_ERROR);

    let mut cut_path = read_request("/notes.txt", 0, 10);
    cut_path.truncate(cut_path.len() - 2);
    assert_eq!(
        one_answer(&mut transfer, &file_system, &cut_path)[1],
        status::PROTOCOL_ERROR
    );

    let mut not_text = read_request("/ab", 0, 10);
    let last = not_text.len() - 1;
    not_text[last] = 0xFF;
    assert_eq!(
        one_answer(&mut transfer, &file_system, &not_text)[1],
        status::PROTOCOL_ERROR
    );

    assert_eq!(
        one_answer(&mut transfer, &file_system, &[command::READ_PACING])[1],
        status::PROTOCOL_ERROR
    );
}

#[test]
fn a_read_past_the_end_sends_nothing_and_finishes() {
    let file_system = file_system();
    file_system.write_file("/notes.txt", b"hello").unwrap();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &read_request("/notes.txt", 50, 10));
    assert_eq!(answer[1], status::OK);
    assert_eq!(u32_at(&answer, 12), 0);
}

#[test]
fn a_file_deleted_between_chunks_reports_no_file() {
    let file_system = file_system();
    file_system.write_file("/notes.txt", &sample_bytes(200)).unwrap();
    let mut transfer = transfer();
    one_answer(&mut transfer, &file_system, &read_request("/notes.txt", 0, 1000));
    file_system.remove("/notes.txt").unwrap();
    let answer = one_answer(&mut transfer, &file_system, &read_pacing_request(45, 1000));
    assert_eq!(answer[1], status::NO_FILE);
}

// ============================================================================
// Writing
// ============================================================================

#[test]
fn writes_a_file_in_pieces_and_reads_it_back() {
    let file_system = file_system();
    let bytes = sample_bytes(1000);
    let mut transfer = transfer();
    upload(&mut transfer, &file_system, "/apps/clock/app.ntapp", &bytes);
    assert!(!transfer.is_writing());
    assert_eq!(file_system.read_to_vec("/apps/clock/app.ntapp").unwrap(), bytes);
    assert_eq!(
        download(&mut transfer, &file_system, "/apps/clock/app.ntapp"),
        bytes
    );
}

#[test]
fn write_answers_give_the_next_offset_and_what_may_follow() {
    let file_system = file_system();
    let mut transfer = transfer();
    let start = one_answer(&mut transfer, &file_system, &write_request("/a", 0, 10));
    assert_eq!(start[0], command::WRITE_PACING);
    assert_eq!(start.len(), WRITE_PACING_SIZE);
    assert_eq!(u32_at(&start, 4), 0);
    assert_eq!(u32_at(&start, 16), 10);

    let middle = one_answer(&mut transfer, &file_system, &write_data_request(0, b"abcd"));
    assert_eq!(u32_at(&middle, 4), 4);
    assert_eq!(u32_at(&middle, 16), 6);
    assert!(transfer.is_writing());

    let end = one_answer(&mut transfer, &file_system, &write_data_request(4, b"efghij"));
    assert_eq!(u32_at(&end, 4), 10);
    assert_eq!(u32_at(&end, 16), 0);
    assert!(!transfer.is_writing());
}

#[test]
fn writing_replaces_a_longer_file() {
    let file_system = file_system();
    file_system.write_file("/a", &sample_bytes(400)).unwrap();
    let mut transfer = transfer();
    upload(&mut transfer, &file_system, "/a", b"short");
    assert_eq!(file_system.read_to_vec("/a").unwrap(), b"short");
}

#[test]
fn an_empty_write_leaves_an_empty_file() {
    let file_system = file_system();
    file_system.write_file("/a", b"old").unwrap();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &write_request("/a", 0, 0));
    assert_eq!(answer[1], status::OK);
    assert_eq!(u32_at(&answer, 16), 0);
    assert!(!transfer.is_writing());
    assert_eq!(file_system.read_to_vec("/a").unwrap(), b"");
}

#[test]
fn an_interrupted_upload_resumes_at_its_offset() {
    let file_system = file_system();
    let bytes = sample_bytes(100);
    let mut transfer = transfer();
    one_answer(&mut transfer, &file_system, &write_request("/a", 0, 100));
    one_answer(&mut transfer, &file_system, &write_data_request(0, &bytes[..40]));
    transfer.abort();
    assert!(!transfer.is_writing());

    let resumed = one_answer(&mut transfer, &file_system, &write_request("/a", 40, 100));
    assert_eq!(resumed[1], status::OK);
    assert_eq!(u32_at(&resumed, 4), 40);
    assert_eq!(u32_at(&resumed, 16), 60);
    one_answer(&mut transfer, &file_system, &write_data_request(40, &bytes[40..]));
    assert_eq!(file_system.read_to_vec("/a").unwrap(), bytes);
}

#[test]
fn resuming_past_what_was_written_is_refused() {
    let file_system = file_system();
    file_system.write_file("/a", b"abc").unwrap();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &write_request("/a", 10, 20));
    assert_eq!(answer[1], status::ERROR);
    assert!(!transfer.is_writing());
}

#[test]
fn an_offset_beyond_the_total_is_a_protocol_error() {
    let file_system = file_system();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &write_request("/a", 11, 10));
    assert_eq!(answer[1], status::PROTOCOL_ERROR);
}

#[test]
fn data_without_a_write_is_a_protocol_error() {
    let file_system = file_system();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &write_data_request(0, b"abc"));
    assert_eq!(answer[0], command::WRITE_PACING);
    assert_eq!(answer[1], status::PROTOCOL_ERROR);
}

#[test]
fn data_past_the_total_or_after_a_gap_is_a_protocol_error() {
    let file_system = file_system();
    let mut transfer = transfer();
    one_answer(&mut transfer, &file_system, &write_request("/a", 0, 4));
    let too_long = one_answer(&mut transfer, &file_system, &write_data_request(0, b"abcdef"));
    assert_eq!(too_long[1], status::PROTOCOL_ERROR);
    let gap = one_answer(&mut transfer, &file_system, &write_data_request(2, b"cd"));
    assert_eq!(gap[1], status::PROTOCOL_ERROR);
    assert!(transfer.is_writing());
}

#[test]
fn a_piece_sent_again_is_written_again() {
    let file_system = file_system();
    let mut transfer = transfer();
    one_answer(&mut transfer, &file_system, &write_request("/a", 0, 6));
    one_answer(&mut transfer, &file_system, &write_data_request(0, b"abc"));
    one_answer(&mut transfer, &file_system, &write_data_request(0, b"xyz"));
    one_answer(&mut transfer, &file_system, &write_data_request(3, b"def"));
    assert_eq!(file_system.read_to_vec("/a").unwrap(), b"xyzdef");
}

#[test]
fn a_stated_size_longer_than_the_data_uses_what_arrived() {
    let file_system = file_system();
    let mut transfer = transfer();
    one_answer(&mut transfer, &file_system, &write_request("/a", 0, 6));
    let mut request = write_data_request(0, b"abc");
    request[8] = 50;
    let answer = one_answer(&mut transfer, &file_system, &request);
    assert_eq!(answer[1], status::OK);
    assert_eq!(u32_at(&answer, 4), 3);
}

#[test]
fn short_or_garbled_writes_are_protocol_errors() {
    let file_system = file_system();
    let mut transfer = transfer();
    assert_eq!(
        one_answer(&mut transfer, &file_system, &[command::WRITE, 0])[1],
        status::PROTOCOL_ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &file_system, &[command::WRITE_DATA, 0])[1],
        status::PROTOCOL_ERROR
    );
    let mut cut_path = write_request("/abc", 0, 3);
    cut_path.pop();
    assert_eq!(
        one_answer(&mut transfer, &file_system, &cut_path)[1],
        status::PROTOCOL_ERROR
    );
}

#[test]
fn writing_where_a_folder_stands_is_an_error() {
    let file_system = file_system();
    file_system.mkdir("/apps").unwrap();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &write_request("/apps", 0, 3));
    assert_eq!(answer[1], status::ERROR);
}

#[test]
fn free_space_limits_what_may_follow() {
    let file_system = file_system();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &write_request("/big", 0, 10_000_000));
    let free = u32_at(&answer, 16);
    assert!(free < BLOCK_SIZE * BLOCK_COUNT);
    assert!(free > 0);
}

#[test]
fn a_full_file_system_stops_the_upload() {
    let file_system = file_system();
    let mut transfer = transfer();
    let total = BLOCK_SIZE * BLOCK_COUNT;
    one_answer(&mut transfer, &file_system, &write_request("/big", 0, total));
    let piece = vec![0x5A; 4096];
    let mut offset = 0;
    let failed = loop {
        let answer = one_answer(&mut transfer, &file_system, &write_data_request(offset, &piece));
        if answer[1] != status::OK {
            break answer;
        }
        offset += piece.len() as u32;
    };
    assert_eq!(failed[1], status::ERROR);
    assert!(!transfer.is_writing());
}

// ============================================================================
// Folders
// ============================================================================

#[test]
fn makes_a_folder_and_its_parents() {
    let file_system = file_system();
    let mut transfer = transfer();
    let answer = one_answer(
        &mut transfer,
        &file_system,
        &make_directory_request("/resources/fonts/"),
    );
    assert_eq!(answer[0], command::MAKE_DIRECTORY_STATUS);
    assert_eq!(answer[1], status::OK);
    assert_eq!(answer.len(), MAKE_DIRECTORY_STATUS_SIZE);
    assert!(is_directory(&file_system, "/resources/fonts"));
}

#[test]
fn making_a_folder_that_exists_succeeds_but_not_over_a_file() {
    let file_system = file_system();
    file_system.mkdir("/apps").unwrap();
    file_system.write_file("/file", b"x").unwrap();
    let mut transfer = transfer();
    assert_eq!(
        one_answer(&mut transfer, &file_system, &make_directory_request("/apps"))[1],
        status::OK
    );
    assert_eq!(
        one_answer(&mut transfer, &file_system, &make_directory_request("/file"))[1],
        status::ERROR
    );
    assert_eq!(
        one_answer(&mut transfer, &file_system, &[command::MAKE_DIRECTORY, 0, 0])[1],
        status::PROTOCOL_ERROR
    );
    let mut cut_path = make_directory_request("/abc");
    cut_path.pop();
    assert_eq!(
        one_answer(&mut transfer, &file_system, &cut_path)[1],
        status::PROTOCOL_ERROR
    );
}

#[test]
fn lists_a_folder_then_closes_the_list() {
    let file_system = file_system();
    file_system.mkdir("/apps").unwrap();
    file_system.mkdir("/apps/clock").unwrap();
    file_system.write_file("/apps/readme", b"hello").unwrap();
    let mut transfer = transfer();
    let answers = transfer.answer(
        Some(&file_system),
        &path_request(command::LIST_DIRECTORY, "/apps"),
        ROOM,
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
    let file_system = file_system();
    file_system.mkdir("/apps").unwrap();
    let mut transfer = transfer();
    let answers = transfer.answer(
        Some(&file_system),
        &path_request(command::LIST_DIRECTORY, ""),
        ROOM,
    );
    assert_eq!(answers.len(), 2);
    assert_eq!(&answers[0][LIST_DIRECTORY_ENTRY_HEADER_SIZE..], b"apps");
}

#[test]
fn a_long_name_is_cut_to_fit_the_notification() {
    let file_system = file_system();
    let long_name = "a".repeat(60);
    file_system
        .write_file(&std::format!("/{long_name}"), b"")
        .unwrap();
    let mut transfer = transfer();
    let answers = transfer.answer(
        Some(&file_system),
        &path_request(command::LIST_DIRECTORY, "/"),
        ROOM,
    );
    assert_eq!(answers[0].len(), ROOM);
    assert_eq!(
        read_u16(&answers[0], 2) as usize,
        ROOM - LIST_DIRECTORY_ENTRY_HEADER_SIZE
    );
}

#[test]
fn listing_something_missing_or_garbled_is_one_failure() {
    let file_system = file_system();
    file_system.write_file("/file", b"x").unwrap();
    let mut transfer = transfer();
    let missing = transfer.answer(
        Some(&file_system),
        &path_request(command::LIST_DIRECTORY, "/nope"),
        ROOM,
    );
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0][1], status::NO_FILE);
    let file = transfer.answer(
        Some(&file_system),
        &path_request(command::LIST_DIRECTORY, "/file"),
        ROOM,
    );
    assert_eq!(file[0][1], status::ERROR);
    let short = transfer.answer(Some(&file_system), &[command::LIST_DIRECTORY, 0], ROOM);
    assert_eq!(short[0][1], status::PROTOCOL_ERROR);
    let mut cut_path = path_request(command::LIST_DIRECTORY, "/abc");
    cut_path.pop();
    assert_eq!(
        transfer.answer(Some(&file_system), &cut_path, ROOM)[0][1],
        status::PROTOCOL_ERROR
    );
}

// ============================================================================
// Deleting and moving
// ============================================================================

#[test]
fn deletes_a_file() {
    let file_system = file_system();
    file_system.write_file("/a", b"x").unwrap();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &path_request(command::DELETE, "/a"));
    assert_eq!(answer, vec![command::DELETE_STATUS, status::OK]);
    assert!(!file_system.exists("/a"));
}

#[test]
fn deletes_a_folder_with_everything_in_it() {
    let file_system = file_system();
    let mut transfer = transfer();
    upload(&mut transfer, &file_system, "/apps/clock/app.ntapp", b"code");
    upload(&mut transfer, &file_system, "/apps/clock/data/state", b"state");
    let answer = one_answer(
        &mut transfer,
        &file_system,
        &path_request(command::DELETE, "/apps/clock/"),
    );
    assert_eq!(answer[1], status::OK);
    assert!(!file_system.exists("/apps/clock"));
    assert!(file_system.exists("/apps"));
}

#[test]
fn a_delete_too_deep_stops_without_removing_the_top() {
    let file_system = file_system();
    let mut path = String::new();
    for _ in 0..=DEEPEST_DELETE + 1 {
        path.push_str("/d");
        file_system.mkdir(&path).unwrap();
    }
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &path_request(command::DELETE, "/d"));
    assert_eq!(answer[1], status::ERROR);
    assert!(file_system.exists("/d"));
}

#[test]
fn deleting_the_top_folder_or_something_missing_fails() {
    let file_system = file_system();
    let mut transfer = transfer();
    assert_eq!(
        one_answer(&mut transfer, &file_system, &path_request(command::DELETE, "/"))[1],
        status::ERROR
    );
    assert_eq!(
        one_answer(
            &mut transfer,
            &file_system,
            &path_request(command::DELETE, "/nope")
        )[1],
        status::NO_FILE
    );
    assert_eq!(
        one_answer(&mut transfer, &file_system, &[command::DELETE, 0])[1],
        status::PROTOCOL_ERROR
    );
    let mut cut_path = path_request(command::DELETE, "/abc");
    cut_path.pop();
    assert_eq!(
        one_answer(&mut transfer, &file_system, &cut_path)[1],
        status::PROTOCOL_ERROR
    );
}

#[test]
fn moves_a_file() {
    let file_system = file_system();
    file_system.write_file("/a", b"hello").unwrap();
    let mut transfer = transfer();
    let answer = one_answer(&mut transfer, &file_system, &move_request("/a", "/b"));
    assert_eq!(answer, vec![command::MOVE_STATUS, status::OK]);
    assert_eq!(file_system.read_to_vec("/b").unwrap(), b"hello");
    assert!(!file_system.exists("/a"));
}

#[test]
fn moving_something_missing_or_garbled_fails() {
    let file_system = file_system();
    let mut transfer = transfer();
    assert_eq!(
        one_answer(&mut transfer, &file_system, &move_request("/a", "/b"))[1],
        status::NO_FILE
    );
    assert_eq!(
        one_answer(&mut transfer, &file_system, &[command::MOVE, 0, 0])[1],
        status::PROTOCOL_ERROR
    );
    let mut cut_path = move_request("/a", "/bcd");
    cut_path.pop();
    assert_eq!(
        one_answer(&mut transfer, &file_system, &cut_path)[1],
        status::PROTOCOL_ERROR
    );
}

// ============================================================================
// Without a file system, and unknown commands
// ============================================================================

#[test]
fn every_command_fails_with_its_own_answer_when_there_is_no_file_system() {
    let mut transfer = transfer();
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
        let answers = transfer.answer(None::<&Filesystem<RamStorage>>, &[request], ROOM);
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0][0], reply);
        assert_eq!(answers[0][1], status::ERROR);
    }
}

#[test]
fn unknown_and_empty_commands_get_no_answer() {
    let file_system = file_system();
    let mut transfer = transfer();
    assert!(transfer.answer(Some(&file_system), &[0x99], ROOM).is_empty());
    assert!(transfer.answer(Some(&file_system), &[], ROOM).is_empty());
    assert!(transfer
        .answer(None::<&Filesystem<RamStorage>>, &[0x99], ROOM)
        .is_empty());
}
