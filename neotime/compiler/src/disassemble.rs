//! An app image as readable text: its state, subscriptions and every instruction of its code,
//! decoded with the runtime's own decoder so the listing always matches what the watch runs.

use std::fmt::Write;

use neotime_runtime::instruction::{decode, Instruction};
use neotime_runtime::AppImage;

// ============================================================================
// Listing
// ============================================================================

/// The whole image as text, or why it cannot be read.
pub fn disassemble(bytes: &[u8]) -> Result<String, String> {
    let image = AppImage::parse(bytes).map_err(|error| format!("not an app image: {error:?}"))?;
    let mut text = String::new();
    let _ = writeln!(text, "image: {} bytes", bytes.len());

    let _ = writeln!(text, "\nstate:");
    for variable in 0..image.state_count() {
        let Some(slot) = image.state_variable(variable) else {
            continue;
        };
        let shape = if slot.length > 0 {
            format!("[{}] x {} B", slot.length, slot.element_bytes)
        } else {
            "value".to_string()
        };
        let _ = writeln!(
            text,
            "  {variable:2} {:22} {:12} {:?}{} default {}",
            image.string(slot.key_string).unwrap_or("?"),
            shape,
            slot.value_type,
            if slot.persists { ", persists" } else { "" },
            slot.default_value
        );
    }

    let _ = writeln!(text, "\nsubscriptions:");
    for number in 0..image.subscription_count() {
        if let Some(subscription) = image.subscription(number) {
            let _ = writeln!(text, "  {number:2} {subscription:?}");
        }
    }

    for number in 0..image.expression_count() {
        let _ = writeln!(text, "\nexpression {number}:");
        list_code(
            &mut text,
            &image,
            image.expression_code(number).unwrap_or_default(),
        );
    }
    for number in 0..image.handler_count() {
        let _ = writeln!(text, "\nhandler {number}:");
        list_code(&mut text, &image, image.handler_code(number).unwrap_or_default());
    }
    for number in 0..image.function_count() {
        let Some(function) = image.function(number) else {
            continue;
        };
        let _ = writeln!(
            text,
            "\nfunction {number} (arguments {}, results {}, locals {}):",
            function.argument_count, function.result_count, function.local_count
        );
        list_code(&mut text, &image, function.code);
    }
    Ok(text)
}

/// One line per instruction: its offset, its bytes, the instruction and what its operand names.
fn list_code(text: &mut String, image: &AppImage, code: &[u8]) {
    let mut offset = 0;
    while offset < code.len() {
        match decode(code, offset) {
            Ok((instruction, next)) => {
                let bytes: Vec<String> = code[offset..next]
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                let note = note(image, instruction, next);
                let _ = writeln!(text, "  {offset:4}  {:24} {instruction:?}{note}", bytes.join(" "));
                offset = next;
            }
            Err(error) => {
                let _ = writeln!(text, "  {offset:4}  cannot decode: {error:?}");
                return;
            }
        }
    }
}

/// What an instruction's operand stands for, such as the state variable's name or a jump's target.
fn note(image: &AppImage, instruction: Instruction, next: usize) -> String {
    let variable_name = |variable: u8| {
        image
            .state_variable(variable as u16)
            .and_then(|slot| image.string(slot.key_string))
            .unwrap_or("?")
            .to_string()
    };
    match instruction {
        Instruction::ReadState(variable)
        | Instruction::WriteState(variable)
        | Instruction::ReadElement(variable)
        | Instruction::WriteElement(variable)
        | Instruction::FillArray(variable)
        | Instruction::TextFromCodes(variable) => format!("  ; {}", variable_name(variable)),
        Instruction::PushText(string)
        | Instruction::FormatDuration(string)
        | Instruction::FormatTime(string) => {
            format!("  ; {:?}", image.string(string).unwrap_or("?"))
        }
        Instruction::Jump(distance) | Instruction::JumpIfFalse(distance) => {
            format!("  ; to {}", next + distance as usize)
        }
        Instruction::LoopBack(distance) => format!("  ; to {}", next.saturating_sub(distance as usize)),
        _ => String::new(),
    }
}

#[cfg(test)]
#[path = "disassemble_tests.rs"]
mod disassemble_tests;
