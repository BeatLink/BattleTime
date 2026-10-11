//! Errors and warnings, each tied to a place in the source and printed as `file:line:column: message`.

use oxc_span::Span;

// ============================================================================
// Diagnostics
// ============================================================================

/// Whether a diagnostic stops the image from being written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// One problem found in the source, with the byte offset it starts at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub offset: u32,
    pub message: String,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity: Severity::Error,
            offset: span.start,
            message: message.into(),
        }
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity: Severity::Warning,
            offset: span.start,
            message: message.into(),
        }
    }

    /// The line and column of the diagnostic, both counted from 1, with columns in characters.
    pub fn line_and_column(&self, source: &str) -> LineAndColumn {
        let offset = (self.offset as usize).min(source.len());
        let before = source.get(..offset).unwrap_or(source);
        let line = before.matches('\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
        let column = before[line_start..].chars().count() + 1;
        LineAndColumn { line, column }
    }

    /// The diagnostic as the command line prints it.
    pub fn render(&self, file_name: &str, source: &str) -> String {
        let place = self.line_and_column(source);
        let label = match self.severity {
            Severity::Error => "",
            Severity::Warning => "warning: ",
        };
        format!(
            "{file_name}:{}:{}: {label}{}",
            place.line, place.column, self.message
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineAndColumn {
    pub line: usize,
    pub column: usize,
}

/// The result of most compiling steps: a value, or the error that stopped it.
pub type Compiled<Value> = Result<Value, Diagnostic>;

/// A shorthand for returning an error at a span.
pub fn fail<Value>(span: Span, message: impl Into<String>) -> Compiled<Value> {
    Err(Diagnostic::error(span, message))
}
