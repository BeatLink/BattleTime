//! `send_to_phone(channel, data)`: the channel and a small object of whole numbers, sent as two texts.
//!
//! The data object becomes JSON text built while the handler runs, such as `{"button":2}`, so the
//! runtime's `SendToPhone` effect carries only the channel's text and that text.

use std::rc::Rc;

use battletime_runtime::instruction::{EffectKind, Instruction};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use oxc_span::{GetSpan, Span};

use crate::compiler::Compiler;
use crate::constants::{static_key, strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled};
use crate::emitter::Emitter;
use crate::permissions::effect_permission;
use crate::scope::Scope;
use crate::types::Type;

/// The most fields one message's data may have.
pub const MOST_MESSAGE_FIELDS: usize = 4;

/// The longest a channel or a field's name may be.
pub const LONGEST_MESSAGE_NAME: usize = 32;

/// Whether a channel or field name can go into the JSON line without escaping.
pub fn is_message_name(name: &str) -> bool {
    (1..=LONGEST_MESSAGE_NAME).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

impl<'a> Compiler<'a> {
    /// Compiles `send_to_phone(channel, { key: number, ... })` into its two texts and the effect.
    pub(crate) fn compile_send_to_phone(
        &mut self,
        emitter: &mut Emitter,
        arguments: &[&'a Expression<'a>],
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<()> {
        let [channel, data] = arguments else {
            return fail(span, "`send_to_phone` takes a channel and an object of numbers");
        };
        let channel_name = match self.evaluate_constant(channel, scope) {
            Some(Constant::Text(text)) if is_message_name(&text) => text,
            _ => {
                return fail(
                    channel.span(),
                    format!("the channel must be written out, 1 to {LONGEST_MESSAGE_NAME} letters, digits, `_` or `-`"),
                )
            }
        };
        let Expression::ObjectExpression(object) = strip_parentheses(data) else {
            return fail(
                data.span(),
                "the data must be an object written out, such as `{ button: 2 }`",
            );
        };
        if object.properties.len() > MOST_MESSAGE_FIELDS {
            return fail(
                object.span,
                format!("a message may have at most {MOST_MESSAGE_FIELDS} fields"),
            );
        }

        let channel_string = self.string(&channel_name);
        emitter.emit(Instruction::PushText(channel_string), span)?;
        for (position, property) in object.properties.iter().enumerate() {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return fail(object.span, "write each field out by name, without `...`");
            };
            let key = match static_key(&property.key) {
                Some(key) if !property.computed && is_message_name(&key) => key,
                _ => {
                    return fail(
                        property.span,
                        format!("a field's name must be written out, 1 to {LONGEST_MESSAGE_NAME} letters, digits, `_` or `-`"),
                    )
                }
            };
            let separator = if position == 0 { "{" } else { "," };
            let prefix = self.string(&format!("{separator}\"{key}\":"));
            emitter.emit(Instruction::PushText(prefix), property.span)?;
            if position > 0 {
                emitter.emit(Instruction::Join, property.span)?;
            }
            self.compile_value_of_type(emitter, &property.value, scope, Type::NUMBER)?;
            emitter.emit(Instruction::FormatNumber(0), property.span)?;
            emitter.emit(Instruction::Join, property.span)?;
        }
        let closing = match object.properties.is_empty() {
            true => "{}",
            false => "}",
        };
        let closing_string = self.string(closing);
        emitter.emit(Instruction::PushText(closing_string), span)?;
        if !object.properties.is_empty() {
            emitter.emit(Instruction::Join, span)?;
        }
        emitter.emit(Instruction::Effect(EffectKind::SendToPhone), span)?;
        self.note_permission(
            effect_permission(EffectKind::SendToPhone),
            "`send_to_phone`",
            span,
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "phone_messages_tests.rs"]
mod phone_messages_tests;
