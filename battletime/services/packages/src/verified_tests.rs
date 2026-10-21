use super::*;

const VERIFIER: u32 = 0xF1F1;

fn mark(crc: u32) -> ImageMark {
    ImageMark {
        crc,
        length: 1000 + crc,
    }
}

#[test]
fn an_added_image_is_held_and_others_are_not() {
    let file = with_added(&[], VERIFIER, mark(1));

    assert!(holds(&file, VERIFIER, mark(1)));
    assert!(!holds(&file, VERIFIER, mark(2)));
    assert!(!holds(&file, VERIFIER, ImageMark { crc: 1, length: 5 }));
}

#[test]
fn another_verifier_holds_nothing_and_starts_afresh() {
    let file = with_added(&with_added(&[], VERIFIER, mark(1)), VERIFIER, mark(2));

    let after_update = with_added(&file, VERIFIER + 1, mark(3));

    assert!(!holds(&file, VERIFIER + 1, mark(1)));
    assert!(holds(&after_update, VERIFIER + 1, mark(3)));
    assert!(!holds(&after_update, VERIFIER + 1, mark(1)));
}

#[test]
fn adding_an_image_twice_keeps_one_entry() {
    let once = with_added(&[], VERIFIER, mark(1));

    assert_eq!(with_added(&once, VERIFIER, mark(1)), once);
}

#[test]
fn the_image_recorded_longest_ago_makes_room() {
    let mut file = Vec::new();
    for crc in 0..=MOST_IMAGES as u32 {
        file = with_added(&file, VERIFIER, mark(crc));
    }

    assert!(!holds(&file, VERIFIER, mark(0)));
    assert!(holds(&file, VERIFIER, mark(1)));
    assert!(holds(&file, VERIFIER, mark(MOST_IMAGES as u32)));
}

#[test]
fn a_mark_is_the_images_crc_field_and_length() {
    let mut image = alloc::vec![0u8; 40];
    image[12..16].copy_from_slice(&0xABCD_u32.to_le_bytes());

    assert_eq!(
        ImageMark::of(&image),
        Some(ImageMark {
            crc: 0xABCD,
            length: 40
        })
    );
    assert_eq!(ImageMark::of(&[0; 8]), None);
}
