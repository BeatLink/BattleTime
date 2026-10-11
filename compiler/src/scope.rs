//! What a name stands for while compiling, and the nested scopes names are looked up in.
//!
//! A scope is an immutable chain: each new name makes a new link, so functions and props can keep
//! the scope they were written in. The chain ends at a module, whose top-level names the compiler
//! keeps in a table of its own.

use std::rc::Rc;

use neotime_runtime::events::EventKind;
use neotime_runtime::SystemValue;
use oxc_ast::ast::{Expression, FormalParameters, FunctionBody, JSXChild};
use oxc_span::Span;

use crate::constants::Constant;
use crate::types::{Range, Type};

// ============================================================================
// References
// ============================================================================

/// The `neotime` and Preact modules an app may import from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryModule {
    Neotime,
    Widgets,
    Components,
    Icons,
    Lib,
    Signals,
    Preact,
    PreactHooks,
}

impl LibraryModule {
    pub fn for_source(source: &str) -> Option<LibraryModule> {
        Some(match source {
            "neotime" | "./index.ts" => LibraryModule::Neotime,
            "neotime/widgets" | "./widgets.tsx" => LibraryModule::Widgets,
            "neotime/components" | "./components.tsx" => LibraryModule::Components,
            "neotime/icons" | "./icons.ts" => LibraryModule::Icons,
            "neotime/lib" | "./lib.ts" => LibraryModule::Lib,
            "@preact/signals" => LibraryModule::Signals,
            "preact" => LibraryModule::Preact,
            "preact/hooks" => LibraryModule::PreactHooks,
            _ => return None,
        })
    }
}

/// A function written in the source, with the scope it was written in.
#[derive(Clone, Debug)]
pub struct FunctionReference<'a> {
    pub name: String,
    pub parameters: &'a FormalParameters<'a>,
    pub body: FunctionBodyReference<'a>,
    pub scope: Rc<Scope<'a>>,
    pub span: Span,
    pub is_async: bool,
}

impl FunctionReference<'_> {
    /// A number that is the same for every reference to the same function, to catch recursion.
    pub fn identity(&self) -> usize {
        self.parameters as *const FormalParameters as usize
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FunctionBodyReference<'a> {
    Statements(&'a FunctionBody<'a>),
    Expression(&'a Expression<'a>),
}

/// A value the runtime keeps current, read through `.value`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemSignal {
    pub value: SystemValue,
    pub value_type: Type,
}

/// What a name or a member such as `props.place` stands for.
#[derive(Clone, Debug)]
pub enum Reference<'a> {
    /// A value known now.
    Constant(Constant),
    /// An expression compiled where it is used, in the scope it was written in.
    Expression {
        expression: &'a Expression<'a>,
        scope: Rc<Scope<'a>>,
    },
    /// A `signal`, `persisted` or `useSignal`, as an object; `.value` reads it.
    Signal(u8),
    /// A `computed`, as an object; `.value` compiles its body.
    Computed(FunctionReference<'a>),
    System(SystemSignal),
    /// An object of system values, such as `settings` or `current_weather`.
    SystemGroup(&'static str),
    /// `current_weather.value` and the like, whose fields are values to read.
    SystemGroupValue(&'static str),
    /// A system value read where it is used, such as `current_weather.value.forecast.length`.
    SystemRead(SystemSignal),
    /// A list whose elements are records of system values: `current_weather.value.forecast` or `installed_apps.value`.
    SystemList(&'static str),
    /// One element of such a list; each field is read with `ReadSystemElement`.
    SystemListElement {
        list: &'static str,
        index: ElementIndex<'a>,
    },
    /// One field of one element of such a list, read where it is used.
    SystemElement {
        system: SystemSignal,
        index: ElementIndex<'a>,
    },
    /// A signal holding a record, as an object; each field has a state variable of its own.
    Record(Rc<Vec<(String, u8)>>),
    /// The value of a record signal: its fields, each read from its own state variable.
    RecordValue(Rc<Vec<(String, u8)>>),
    /// A signal holding an array of records, kept as one array per field.
    RecordArray(Rc<Vec<(String, u8)>>),
    /// The value of an array-of-records signal.
    RecordArrayValue(Rc<Vec<(String, u8)>>),
    /// One record of such an array; each field is an element of that field's array.
    RecordElement {
        fields: Rc<Vec<(String, u8)>>,
        index: ElementIndex<'a>,
    },
    /// One element of an array signal at an index worked out where it is read.
    StateElementAt {
        variable: u8,
        index: ElementIndex<'a>,
    },
    /// One single-value state variable, read where it is used.
    StateScalar(u8),
    /// The value of an array signal: the whole array.
    StateArray(u8),
    /// One element of an array signal, at an index known now.
    StateElement {
        variable: u8,
        index: i64,
    },
    Function(FunctionReference<'a>),
    /// A name imported from `neotime` or Preact.
    Library {
        module: LibraryModule,
        name: String,
    },
    /// One of a handler's locals.
    Local {
        index: u8,
        value_type: Type,
        /// What a `const` local can hold; `Range::ANY` for one that can change.
        range: Range,
        is_constant: bool,
    },
    /// A loop's element over constant numbers that go up evenly: `start + step * index`.
    ProgressionElement {
        index_local: u8,
        start: i64,
        step: i64,
    },
    /// The event a handler answers.
    Event(EventKind),
    /// One field of the event, read by its position.
    EventField {
        field: u8,
        value_type: Type,
    },
    /// An object whose fields are references, such as a component's props.
    Object(Rc<Vec<(String, Reference<'a>)>>),
    /// JSX children passed to a component.
    Children {
        children: &'a [JSXChild<'a>],
        scope: Rc<Scope<'a>>,
    },
}

/// Where the index of an element read or written comes from.
#[derive(Clone, Debug)]
pub enum ElementIndex<'a> {
    Constant(i64),
    Local(u8),
    Expression {
        expression: &'a Expression<'a>,
        scope: Rc<Scope<'a>>,
    },
}

// ============================================================================
// Scopes
// ============================================================================

/// One link of a scope chain; the chain ends at the module it belongs to.
#[derive(Debug)]
pub struct Scope<'a> {
    pub parent: Option<Rc<Scope<'a>>>,
    pub name: String,
    pub reference: Option<Reference<'a>>,
    pub module: usize,
}

impl<'a> Scope<'a> {
    /// The empty scope at the top of a module.
    pub fn module_root(module: usize) -> Rc<Scope<'a>> {
        Rc::new(Scope {
            parent: None,
            name: String::new(),
            reference: None,
            module,
        })
    }

    /// A scope with one more name than this one.
    pub fn with(self: &Rc<Self>, name: &str, reference: Reference<'a>) -> Rc<Scope<'a>> {
        Rc::new(Scope {
            parent: Some(self.clone()),
            name: name.to_string(),
            reference: Some(reference),
            module: self.module,
        })
    }

    /// The nearest binding of a name in this chain, before the module's own names.
    pub fn find(&self, name: &str) -> Option<&Reference<'a>> {
        let mut link = Some(self);
        while let Some(scope) = link {
            if scope.name == name {
                if let Some(reference) = &scope.reference {
                    return Some(reference);
                }
            }
            link = scope.parent.as_deref();
        }
        None
    }
}
