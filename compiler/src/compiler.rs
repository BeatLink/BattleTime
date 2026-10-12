//! The compiler's state, the module's top-level names, the app's state variables and the
//! `define_app` call that ties the screens together into one image.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use neotime_runtime::image::{MOST_STATE_BYTES, MOST_STATE_VARIABLES, MOST_VARIABLE_BYTES};
use neotime_runtime::widgets::MOST_WIDGETS;
use neotime_runtime::writer::ImageWriter;
use neotime_runtime::{AppKind, Property, PropertySource};
use oxc_ast::ast::{
    ArrowFunctionBody, BindingPattern, Declaration, ExportDefaultDeclarationKind, Expression, Function,
    ImportDeclarationSpecifier, ModuleExportName, ObjectExpression, ObjectPropertyKind, Program, Statement,
    TSLiteral, TSType, TSTypeName, VariableDeclarationKind,
};
use oxc_span::{GetSpan, Span};

use crate::constants::{static_key, strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled, Diagnostic};
use crate::scope::{FunctionBodyReference, FunctionReference, LibraryModule, Reference, Scope};
use crate::subscriptions::lifecycle_kind;
use crate::types::{ChoiceSet, Kind, NumberHint, Range, Type};
use crate::widgets::WidgetNode;
use neotime_runtime::subscriptions::SubscriptionKind;

// ============================================================================
// Limits and versions
// ============================================================================

/// The shared library version images are built against, which the library image carries too.
pub const SHARED_LIBRARY_VERSION: u16 = 2;

// ============================================================================
// State variables
// ============================================================================

/// One state variable as the compiler lays it out.
#[derive(Clone, Debug)]
pub struct StateSlot {
    pub key: String,
    pub value_type: Type,
    pub hint: NumberHint,
    pub persists: bool,
    /// Zero for a single value.
    pub length: u16,
    pub element_bytes: u8,
    pub default_value: i64,
    /// Each element's own starting value, for an array whose elements start different.
    pub start_values: Option<Rc<Vec<i64>>>,
    pub span: Span,
}

impl StateSlot {
    pub fn is_array(&self) -> bool {
        self.length > 0
    }

    pub fn stored_bytes(&self) -> usize {
        self.length.max(1) as usize * self.element_bytes as usize
    }

    /// The values the variable starts with.
    pub fn start_range(&self) -> Range {
        let start = Range::exactly(self.default_value);
        match &self.start_values {
            Some(values) => values
                .iter()
                .fold(start, |range, value| range.union(Range::exactly(*value))),
            None => start,
        }
    }
}

/// What a type annotation or a starting value says a variable holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeclaredType {
    pub value_type: Type,
    pub hint: NumberHint,
    pub is_array: bool,
}

// ============================================================================
// Modules
// ============================================================================

/// One source file: its top-level names and the `define_app` options, if it has them.
pub struct Module<'a> {
    pub source: &'a str,
    pub names: HashMap<String, Reference<'a>>,
    pub type_aliases: HashMap<String, &'a TSType<'a>>,
    pub app_options: Option<&'a ObjectExpression<'a>>,
}

/// A signal declared at the top of the file, before its slot is laid out.
struct SignalDeclaration<'a> {
    name: String,
    persisted_key: Option<String>,
    default_value: &'a Expression<'a>,
    type_argument: Option<&'a TSType<'a>>,
    span: Span,
}

// ============================================================================
// Compiler
// ============================================================================

/// Everything the compiler keeps while it builds one image.
pub struct Compiler<'a> {
    pub modules: Vec<Module<'a>>,
    pub slots: Vec<StateSlot>,
    pub writer: ImageWriter,
    pub warnings: Vec<Diagnostic>,
    warned_about: HashSet<String>,
    expression_numbers: HashMap<Vec<u8>, u16>,
    handler_numbers: HashMap<Vec<u8>, u16>,
    /// The values each variable was assumed to hold, from the previous pass; empty in the first.
    pub assumed_ranges: HashMap<String, Range>,
    /// The values each variable may be given in this pass, gathered to choose array widths.
    pub written_ranges: HashMap<u8, Range>,
    /// Arrays copied into others, so the copy's values count for the destination too.
    pub copied_into: Vec<(u8, u8)>,
    /// Functions being inlined, innermost last, to catch recursion.
    pub inline_stack: Vec<usize>,
    /// The scratch arrays whole new array values are built in, with the shape each serves.
    pub scratch_arrays: Vec<(crate::statements::ScratchShape, u8)>,
    /// Scratch arrays being filled, which no other assignment may take.
    pub scratch_in_use: Vec<u8>,
    pub use_signal_count: usize,
    /// The module holding the `neotime/components` source, once it is loaded.
    pub components_module: Option<usize>,
    /// The constant arrays widgets name, kept as state arrays, by element type and values.
    pub constant_tables: Vec<((Type, Vec<i64>), u8)>,
    /// The loops being compiled, innermost last.
    pub loop_labels: Vec<crate::statements::LoopLabels>,
    /// The folder of the source file, which the paths of the app's own images are relative to.
    pub source_directory: Option<PathBuf>,
    /// The app's own images already added, by file, with their image numbers.
    pub image_numbers: Vec<(PathBuf, u16)>,
    /// The hidden arrays derive subscriptions keep, by the expression they hold.
    pub derived_arrays: Vec<((usize, usize), u8)>,
    /// Every hidden array a derive handler writes, which needs no scratch copy.
    pub derived_targets: Vec<u8>,
    /// How many hidden derived variables there are, for their keys.
    pub derived_count: usize,
    /// The first use of each permission the app needs, to check against its `permissions`.
    pub permission_uses: Vec<crate::permissions::PermissionUse>,
    /// Which of the app's functions are called rather than inlined in this pass.
    pub function_plan: crate::functions::FunctionPlan,
    /// What inlining each function cost in this pass, by its identity.
    pub inline_costs: HashMap<usize, crate::functions::InlineCost>,
    /// Functions this pass tried and failed to compile on their own.
    pub failed_functions: HashSet<usize>,
    /// The functions compiled so far, one for each way of calling them.
    pub compiled_functions: Vec<crate::functions::CompiledFunction>,
    /// The values each runtime parameter of a called function was given in this pass.
    pub parameter_ranges: HashMap<String, Range>,
    /// Whether a `.map` rendered here is directly inside a widget that holds others, so it becomes one Repeat.
    pub rows_repeat_here: bool,
    /// The `.map` calls, by where they start, whose row could not be drawn as one repeated row.
    pub failed_maps: HashSet<usize>,
}

impl<'a> Compiler<'a> {
    pub fn new(assumed_ranges: HashMap<String, Range>) -> Compiler<'a> {
        Compiler {
            modules: Vec::new(),
            slots: Vec::new(),
            writer: ImageWriter::new("", "", AppKind::App, SHARED_LIBRARY_VERSION),
            warnings: Vec::new(),
            warned_about: HashSet::new(),
            expression_numbers: HashMap::new(),
            handler_numbers: HashMap::new(),
            assumed_ranges,
            written_ranges: HashMap::new(),
            copied_into: Vec::new(),
            inline_stack: Vec::new(),
            scratch_arrays: Vec::new(),
            scratch_in_use: Vec::new(),
            use_signal_count: 0,
            loop_labels: Vec::new(),
            constant_tables: Vec::new(),
            components_module: None,
            source_directory: None,
            image_numbers: Vec::new(),
            derived_arrays: Vec::new(),
            derived_targets: Vec::new(),
            derived_count: 0,
            permission_uses: Vec::new(),
            function_plan: crate::functions::FunctionPlan::default(),
            inline_costs: HashMap::new(),
            failed_functions: HashSet::new(),
            compiled_functions: Vec::new(),
            parameter_ranges: HashMap::new(),
            rows_repeat_here: false,
            failed_maps: HashSet::new(),
        }
    }

    /// Records a warning, once for each distinct message.
    pub fn warn(&mut self, span: Span, message: impl Into<String>) {
        let message = message.into();
        if self.warned_about.insert(message.clone()) {
            self.warnings.push(Diagnostic::warning(span, message));
        }
    }

    /// What a name stands for in a scope, falling back to the module's top-level names.
    pub fn lookup(&self, scope: &Rc<Scope<'a>>, name: &str) -> Option<Reference<'a>> {
        if let Some(reference) = scope.find(name) {
            return Some(reference.clone());
        }
        self.modules.get(scope.module)?.names.get(name).cloned()
    }

    pub fn string(&mut self, text: &str) -> u16 {
        self.writer.string(text)
    }

    /// Adds an expression's code, sharing the number of an identical one.
    pub fn add_expression(&mut self, code: Vec<u8>) -> u16 {
        if let Some(number) = self.expression_numbers.get(&code) {
            return *number;
        }
        let number = self.writer.expression(code.clone());
        self.expression_numbers.insert(code, number);
        number
    }

    pub fn add_handler(&mut self, code: Vec<u8>) -> u16 {
        if let Some(number) = self.handler_numbers.get(&code) {
            return *number;
        }
        let number = self.writer.handler(code.clone());
        self.handler_numbers.insert(code, number);
        number
    }

    // ------------------------------------------------------------------------
    // State
    // ------------------------------------------------------------------------

    /// Adds a state variable and returns its number.
    pub fn add_slot(&mut self, slot: StateSlot) -> Compiled<u8> {
        if self.slots.len() >= MOST_STATE_VARIABLES {
            return fail(
                slot.span,
                format!("an app may have at most {MOST_STATE_VARIABLES} state variables; combine some into an array"),
            );
        }
        let number = if let Some(values) = &slot.start_values {
            let Some(number) = self.writer.array_with_values(
                &slot.key,
                slot.value_type.value_type(slot.hint),
                slot.persists,
                slot.element_bytes,
                values,
            ) else {
                return fail(
                    slot.span,
                    "internal compiler error: a starting value does not fit its array",
                );
            };
            number
        } else if slot.is_array() {
            self.writer.array(
                &slot.key,
                slot.value_type.value_type(slot.hint),
                slot.persists,
                slot.length,
                slot.element_bytes,
                slot.default_value,
            )
        } else {
            self.writer.variable(
                &slot.key,
                slot.value_type.value_type(slot.hint),
                slot.persists,
                slot.default_value,
            )
        };
        self.slots.push(slot);
        Ok(number)
    }

    /// The element width of a new array, from the values the previous pass assumed it holds.
    pub fn element_bytes_for(&self, key: &str, declared: DeclaredType, start: Range) -> u8 {
        if declared.value_type.kind == Kind::Number && declared.hint != NumberHint::Plain {
            return 8;
        }
        let range = self
            .assumed_ranges
            .get(key)
            .map_or(start, |assumed| assumed.union(start));
        match declared.value_type.kind {
            Kind::Number => range.element_bytes(),
            _ => type_range(declared.value_type).union(range).element_bytes(),
        }
    }

    /// The values a state variable may hold when it is read, as far as the compiler can tell.
    pub fn read_range(&self, variable: u8) -> Range {
        let Some(slot) = self.slots.get(variable as usize) else {
            return Range::ANY;
        };
        if slot.value_type.kind != Kind::Number {
            return type_range(slot.value_type);
        }
        let start = slot.start_range();
        let assumed = self
            .assumed_ranges
            .get(&slot.key)
            .map_or(start, |assumed| assumed.union(start));
        if slot.value_type.nullable {
            assumed.union(Range::exactly(neotime_runtime::NONE))
        } else {
            assumed
        }
    }

    /// Records a value that may be written into a state variable, for choosing array widths.
    pub fn note_write(&mut self, variable: u8, range: Range) {
        let entry = self.written_ranges.entry(variable).or_insert(range);
        *entry = entry.union(range);
    }

    /// The values every state variable was given in this pass, its starting value included.
    pub fn observed_ranges(&self) -> HashMap<String, Range> {
        let mut ranges: Vec<Range> = self
            .slots
            .iter()
            .enumerate()
            .map(|(number, slot)| {
                let start = slot.start_range();
                let written = self.written_ranges.get(&(number as u8)).copied();
                written.map_or(start, |written| written.union(start))
            })
            .collect();
        for _ in 0..self.copied_into.len() + 1 {
            for (destination, source) in &self.copied_into {
                let (Some(from), Some(to)) = (
                    ranges.get(*source as usize).copied(),
                    ranges.get(*destination as usize).copied(),
                ) else {
                    continue;
                };
                ranges[*destination as usize] = to.union(from);
            }
        }
        self.slots
            .iter()
            .zip(ranges)
            .map(|(slot, range)| (slot.key.clone(), range))
            .chain(
                self.parameter_ranges
                    .iter()
                    .map(|(key, range)| (key.clone(), *range)),
            )
            .collect()
    }

    fn check_state_limits(&self) -> Compiled<()> {
        let mut total = 0;
        for slot in &self.slots {
            let bytes = slot.stored_bytes();
            if bytes > MOST_VARIABLE_BYTES {
                return fail(
                    slot.span,
                    format!(
                        "`{}` takes {bytes} bytes, and one variable may take at most {MOST_VARIABLE_BYTES}; use a shorter array or smaller numbers",
                        slot.key
                    ),
                );
            }
            total += bytes;
        }
        if total > MOST_STATE_BYTES {
            let span = self.slots.last().map_or(Span::default(), |slot| slot.span);
            return fail(
                span,
                format!("the app's state takes {total} bytes, and an app may have at most {MOST_STATE_BYTES}; shorten its arrays"),
            );
        }
        Ok(())
    }

    // ------------------------------------------------------------------------
    // Loading a module
    // ------------------------------------------------------------------------

    /// Reads a file's imports and top-level declarations into a new module.
    pub fn load_module(&mut self, program: &'a Program<'a>) -> Compiled<usize> {
        let module_number = self.modules.len();
        self.modules.push(Module {
            source: program.source_text,
            names: HashMap::new(),
            type_aliases: HashMap::new(),
            app_options: None,
        });
        let root = Scope::module_root(module_number);
        let mut signals: Vec<SignalDeclaration<'a>> = Vec::new();

        for statement in &program.body {
            self.load_statement(statement, module_number, &root, &mut signals)?;
        }

        // The app's name and kind are needed before anything else goes into the image.
        if let Some(options) = self.modules[module_number].app_options {
            self.start_image(options, &root)?;
        }
        for signal in signals {
            let reference = self.declare_signal(&signal, &root)?;
            self.modules[module_number]
                .names
                .insert(signal.name.clone(), reference);
        }
        Ok(module_number)
    }

    fn load_statement(
        &mut self,
        statement: &'a Statement<'a>,
        module: usize,
        root: &Rc<Scope<'a>>,
        signals: &mut Vec<SignalDeclaration<'a>>,
    ) -> Compiled<()> {
        match statement {
            Statement::ImportDeclaration(import) => {
                let source = import.source.value.as_str();
                let Some(library) = LibraryModule::for_source(source) else {
                    return fail(
                        import.span,
                        format!("apps may only import from preact, @preact/signals and neotime, not `{source}`; copy the code into the app"),
                    );
                };
                for specifier in import.specifiers.iter().flatten() {
                    let (local, imported) = match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                            let imported = match &specifier.imported {
                                ModuleExportName::IdentifierName(name) => name.name.as_str(),
                                ModuleExportName::IdentifierReference(name) => name.name.as_str(),
                                ModuleExportName::StringLiteral(name) => name.value.as_str(),
                            };
                            (specifier.local.name.as_str(), imported)
                        }
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                            (specifier.local.name.as_str(), "default")
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                            return fail(
                                specifier.span,
                                "import the names you use one by one, such as `import { now } from \"neotime\"`",
                            )
                        }
                    };
                    self.modules[module].names.insert(
                        local.to_string(),
                        Reference::Library {
                            module: library,
                            name: imported.to_string(),
                        },
                    );
                }
                Ok(())
            }
            Statement::ExportDeclaration(export) => self.load_declaration(&export.declaration, module, root, signals),
            Statement::ExportNamedDeclaration(_) | Statement::EmptyStatement(_) => Ok(()),
            Statement::ExportDefaultDeclaration(export) => match &export.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                    self.load_function(function, module, root)
                }
                other => {
                    let Some(expression) = other.as_expression() else {
                        return fail(export.span, "export the app with `export default define_app({ ... })`");
                    };
                    self.load_default_export(expression, module, root)
                }
            },
            Statement::ExportAllDeclaration(export) => {
                fail(export.span, "re-exporting other modules is not supported; copy the code into the app")
            }
            Statement::ExportFromDeclaration(export) => {
                fail(export.span, "re-exporting other modules is not supported; copy the code into the app")
            }
            other => match other.as_declaration() {
                Some(declaration) => self.load_declaration(declaration, module, root, signals),
                None => fail(
                    other.span(),
                    "only declarations may sit at the top of the file; put this code in a handler or a computed",
                ),
            },
        }
    }

    fn load_default_export(
        &mut self,
        expression: &'a Expression<'a>,
        module: usize,
        root: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let Expression::CallExpression(call) = strip_parentheses(expression) else {
            return fail(
                expression.span(),
                "export the app with `export default define_app({ ... })`",
            );
        };
        let is_define_app = matches!(
            self.resolve_reference(&call.callee, root),
            Some(Reference::Library { module: LibraryModule::Neotime, ref name }) if name == "define_app"
        );
        let options = call
            .arguments
            .first()
            .and_then(|argument| argument.as_expression());
        match (is_define_app, options.map(strip_parentheses)) {
            (true, Some(Expression::ObjectExpression(object))) => {
                self.modules[module].app_options = Some(object);
                Ok(())
            }
            _ => fail(
                call.span,
                "export the app with `export default define_app({ ... })`",
            ),
        }
    }

    fn load_declaration(
        &mut self,
        declaration: &'a Declaration<'a>,
        module: usize,
        root: &Rc<Scope<'a>>,
        signals: &mut Vec<SignalDeclaration<'a>>,
    ) -> Compiled<()> {
        match declaration {
            Declaration::FunctionDeclaration(function) => self.load_function(function, module, root),
            Declaration::VariableDeclaration(variables) => {
                if variables.kind != VariableDeclarationKind::Const {
                    return fail(
                        variables.span,
                        "top-level variables must be `const`; keep changing values in a `signal` or `persisted`",
                    );
                }
                for declarator in &variables.declarations {
                    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
                        return fail(declarator.span, "give each top-level constant a name of its own");
                    };
                    let name = identifier.name.as_str().to_string();
                    let Some(init) = &declarator.init else {
                        return fail(declarator.span, "give the constant a value");
                    };
                    let reference = self.top_level_value(&name, init, root, signals)?;
                    if let Some(reference) = reference {
                        self.modules[module].names.insert(name, reference);
                    }
                }
                Ok(())
            }
            Declaration::TSTypeAliasDeclaration(alias) => {
                self.modules[module]
                    .type_aliases
                    .insert(alias.id.name.as_str().to_string(), &alias.type_annotation);
                Ok(())
            }
            Declaration::TSInterfaceDeclaration(_) => Ok(()),
            Declaration::ClassDeclaration(class) => fail(
                class.span,
                "classes are not available; keep state in signals and write plain functions",
            ),
            other => fail(other.span(), "this kind of declaration is not supported in apps"),
        }
    }

    fn load_function(
        &mut self,
        function: &'a Function<'a>,
        module: usize,
        root: &Rc<Scope<'a>>,
    ) -> Compiled<()> {
        let Some(identifier) = &function.id else {
            return Ok(());
        };
        let reference = function_reference(function, root)?;
        self.modules[module].names.insert(
            identifier.name.as_str().to_string(),
            Reference::Function(reference),
        );
        Ok(())
    }

    /// What a top-level `const` stands for; a signal is only noted here and laid out later.
    fn top_level_value(
        &mut self,
        name: &str,
        init: &'a Expression<'a>,
        root: &Rc<Scope<'a>>,
        signals: &mut Vec<SignalDeclaration<'a>>,
    ) -> Compiled<Option<Reference<'a>>> {
        let stripped = strip_parentheses(init);
        match stripped {
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                return Ok(Some(Reference::Function(callable_reference(
                    name, stripped, root,
                )?)));
            }
            Expression::CallExpression(call) => {
                let callee = self.resolve_reference(&call.callee, root);
                let library_name = match &callee {
                    Some(Reference::Library { module, name }) => Some((*module, name.as_str())),
                    _ => None,
                };
                let arguments: Vec<&'a Expression<'a>> = call
                    .arguments
                    .iter()
                    .filter_map(|argument| argument.as_expression())
                    .collect();
                let type_argument = call
                    .type_arguments
                    .as_ref()
                    .and_then(|arguments| arguments.params.first());
                match library_name {
                    Some((LibraryModule::Signals, "signal")) => {
                        let Some(default_value) = arguments.first() else {
                            return fail(call.span, "give the signal a starting value");
                        };
                        signals.push(SignalDeclaration {
                            name: name.to_string(),
                            persisted_key: None,
                            default_value,
                            type_argument,
                            span: call.span,
                        });
                        return Ok(None);
                    }
                    Some((LibraryModule::Neotime, "persisted")) => {
                        let key = arguments
                            .first()
                            .and_then(|key| self.evaluate_constant(key, root));
                        let (Some(Constant::Text(key)), Some(default_value)) = (key, arguments.get(1)) else {
                            return fail(
                                call.span,
                                "write `persisted(\"key\", starting value)` with a literal key",
                            );
                        };
                        signals.push(SignalDeclaration {
                            name: name.to_string(),
                            persisted_key: Some(key.to_string()),
                            default_value,
                            type_argument,
                            span: call.span,
                        });
                        return Ok(None);
                    }
                    Some((LibraryModule::Signals, "computed")) => {
                        let Some(body) = arguments.first() else {
                            return fail(
                                call.span,
                                "give `computed` a function, such as `computed(() => ...)`",
                            );
                        };
                        return Ok(Some(Reference::Computed(callable_reference(name, body, root)?)));
                    }
                    Some((LibraryModule::Signals, "effect"))
                    | Some((LibraryModule::PreactHooks, "useEffect")) => {
                        return fail(
                            call.span,
                            "effects are not available; use `subscriptions` in define_app, or a binding that reads `now`",
                        );
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(Some(Reference::Expression {
            expression: init,
            scope: root.clone(),
        }))
    }

    // ------------------------------------------------------------------------
    // Signals
    // ------------------------------------------------------------------------

    fn declare_signal(
        &mut self,
        signal: &SignalDeclaration<'a>,
        root: &Rc<Scope<'a>>,
    ) -> Compiled<Reference<'a>> {
        let Some(default_value) = self.evaluate_constant(signal.default_value, root) else {
            return fail(
                signal.default_value.span(),
                "a signal's starting value must be known when the app is compiled, such as a number or a constant",
            );
        };
        let key = signal
            .persisted_key
            .clone()
            .unwrap_or_else(|| signal.name.clone());
        if let Some(record) =
            self.add_record_slots(&key, signal.persisted_key.is_some(), &default_value, signal.span)
        {
            return record;
        }
        let declared = match signal.type_argument {
            Some(annotation) => self.declared_type(annotation, root.module)?,
            None => None,
        };
        let declared = match declared {
            Some(declared) => declared,
            None => infer_declared_type(&default_value).ok_or_else(|| {
                Diagnostic::error(
                    signal.span,
                    "say what the signal holds, such as `signal<Instant | null>(null)`; objects in state are not supported yet",
                )
            })?,
        };
        let slot = self.add_signal_slot(
            key,
            signal.persisted_key.is_some(),
            declared,
            &default_value,
            signal.span,
        )?;
        Ok(Reference::Signal(slot))
    }

    /// Lays out a record, or an array of records, as one state variable or array per field, keyed
    /// `name.field`; anything else gives `None`.
    pub fn add_record_slots(
        &mut self,
        key: &str,
        persists: bool,
        value: &Constant,
        span: Span,
    ) -> Option<Compiled<Reference<'a>>> {
        let (fields, length) = match value {
            Constant::Object(fields) => (fields.clone(), None),
            Constant::Array(elements) => match elements.first() {
                Some(Constant::Object(fields)) => (fields.clone(), Some(elements.len())),
                _ => return None,
            },
            _ => return None,
        };
        Some((|| {
            let mut slots = Vec::new();
            for (field, field_value) in fields.iter() {
                let Some(declared) = infer_declared_type(field_value).filter(|declared| !declared.is_array)
                else {
                    return fail(
                        span,
                        format!("the field `{field}` cannot be kept in state; a record's fields must be numbers, booleans, text, colours or icons"),
                    );
                };
                let (declared, start) = match length {
                    Some(length) => (
                        DeclaredType {
                            is_array: true,
                            ..declared
                        },
                        Constant::Array(Rc::new(match value {
                            Constant::Array(records) => records
                                .iter()
                                .map(|record| record.field(field).unwrap_or(Constant::Undefined))
                                .collect(),
                            _ => vec![field_value.clone(); length],
                        })),
                    ),
                    None => (declared, field_value.clone()),
                };
                let slot =
                    self.add_signal_slot(format!("{key}.{field}"), persists, declared, &start, span)?;
                slots.push((field.clone(), slot));
            }
            let slots = Rc::new(slots);
            Ok(match length {
                Some(_) => Reference::RecordArray(slots),
                None => Reference::Record(slots),
            })
        })())
    }

    /// A state array holding constant values for a widget, such as a palette, shared when equal.
    pub fn constant_table(&mut self, elements: &[Constant], element_type: Type, span: Span) -> Compiled<u8> {
        let mut values = Vec::new();
        for element in elements {
            values.push(self.constant_number(element, element_type, span)?);
        }
        let shape = (element_type, values.clone());
        if let Some((_, table)) = self
            .constant_tables
            .iter()
            .find(|(existing, _)| *existing == shape)
        {
            return Ok(*table);
        }
        let key = format!("~table{}", self.constant_tables.len());
        let declared = DeclaredType {
            value_type: element_type,
            hint: NumberHint::Plain,
            is_array: true,
        };
        let constant = Constant::Array(Rc::new(elements.to_vec()));
        let table = self.add_signal_slot(key, false, declared, &constant, span)?;
        self.constant_tables.push((shape, table));
        Ok(table)
    }

    /// Lays out a signal's slot from its declared type and starting value.
    pub fn add_signal_slot(
        &mut self,
        key: String,
        persists: bool,
        declared: DeclaredType,
        default_value: &Constant,
        span: Span,
    ) -> Compiled<u8> {
        let (length, element_default, elements) = match (declared.is_array, default_value) {
            (true, Constant::Array(elements)) => {
                if elements.is_empty() || elements.len() > u16::MAX as usize {
                    return fail(span, "an array in state needs between 1 and 65535 elements");
                }
                (elements.len() as u16, elements[0].clone(), Some(elements.clone()))
            }
            (false, Constant::Array(_)) | (true, _) => {
                return fail(
                    span,
                    "the starting value does not match the type given for the signal",
                )
            }
            (false, value) => (0, value.clone(), None),
        };
        let default_number = self.constant_number(&element_default, declared.value_type, span)?;
        let start_values = match elements {
            Some(elements) if !Constant::is_uniform_array(&elements) => {
                let mut values = Vec::new();
                for element in elements.iter() {
                    values.push(self.constant_number(element, declared.value_type, span)?);
                }
                Some(Rc::new(values))
            }
            _ => None,
        };
        let start = start_values
            .as_ref()
            .map_or(Range::exactly(default_number), |values| {
                values
                    .iter()
                    .fold(Range::exactly(default_number), |range, value| {
                        range.union(Range::exactly(*value))
                    })
            });
        let element_bytes = if declared.is_array {
            self.element_bytes_for(&key, declared, start)
        } else {
            8
        };
        self.add_slot(StateSlot {
            key,
            value_type: declared.value_type,
            hint: declared.hint,
            persists,
            length,
            element_bytes,
            default_value: default_number,
            start_values,
            span,
        })
    }

    /// The number a constant is stored as when a value of `value_type` is expected.
    pub fn constant_number(&mut self, constant: &Constant, value_type: Type, span: Span) -> Compiled<i64> {
        let mismatch = || {
            Diagnostic::error(
                span,
                format!(
                    "expected {}, but this is {}",
                    value_type.describe(),
                    describe_constant(constant)
                ),
            )
        };
        Ok(match (constant, value_type.kind) {
            (Constant::Null, _) if value_type.nullable => neotime_runtime::NONE,
            (Constant::Number(number), Kind::Number) => *number,
            (Constant::Bool(value), Kind::Bool) => *value as i64,
            (Constant::Text(text), Kind::Text | Kind::Name) => self.string(text) as i64,
            (Constant::Text(text), Kind::Color) => color_constant(text).ok_or_else(|| {
                Diagnostic::error(
                    span,
                    format!("`{text}` is not a colour; write `#rrggbb` or a theme colour such as \"accent\""),
                )
            })?,
            (Constant::Text(text), Kind::Choice(set)) => set.number_of(text).ok_or_else(|| {
                Diagnostic::error(
                    span,
                    format!(
                        "`{text}` is not a {}; choose one of: {}",
                        set.description(),
                        set.names().join(", ")
                    ),
                )
            })?,
            (Constant::Icon(number), Kind::Icon) => *number,
            _ => return Err(mismatch()),
        })
    }

    /// The type a TypeScript annotation names, or `None` if the compiler should infer it instead.
    pub fn declared_type(&self, annotation: &'a TSType<'a>, module: usize) -> Compiled<Option<DeclaredType>> {
        let scalar = |kind: Kind, hint: NumberHint| {
            Ok(Some(DeclaredType {
                value_type: Type::plain(kind),
                hint,
                is_array: false,
            }))
        };
        match annotation {
            TSType::TSNumberKeyword(_) => scalar(Kind::Number, NumberHint::Plain),
            TSType::TSBooleanKeyword(_) => scalar(Kind::Bool, NumberHint::Plain),
            TSType::TSStringKeyword(_) => scalar(Kind::Name, NumberHint::Plain),
            TSType::TSNullKeyword(_) => Ok(Some(DeclaredType {
                value_type: Type::NULL,
                hint: NumberHint::Plain,
                is_array: false,
            })),
            TSType::TSParenthesizedType(inner) => self.declared_type(&inner.type_annotation, module),
            TSType::TSLiteralType(literal) => match &literal.literal {
                TSLiteral::StringLiteral(_) => scalar(Kind::Name, NumberHint::Plain),
                TSLiteral::NumericLiteral(_) => scalar(Kind::Number, NumberHint::Plain),
                TSLiteral::BooleanLiteral(_) => scalar(Kind::Bool, NumberHint::Plain),
                _ => Ok(None),
            },
            TSType::TSArrayType(array) => {
                Ok(self
                    .declared_type(&array.element_type, module)?
                    .map(|element| DeclaredType {
                        is_array: true,
                        ..element
                    }))
            }
            TSType::TSUnionType(union) => {
                let mut combined: Option<DeclaredType> = None;
                for member in &union.types {
                    let Some(member) = self.declared_type(member, module)? else {
                        return Ok(None);
                    };
                    combined = Some(match combined {
                        None => member,
                        Some(previous) => {
                            let unified = match (previous.value_type.kind, member.value_type.kind) {
                                (Kind::Choice(_), Kind::Name) | (Kind::Name, Kind::Choice(_)) => Some(Type {
                                    kind: Kind::Name,
                                    nullable: previous.value_type.nullable || member.value_type.nullable,
                                }),
                                _ => previous.value_type.unify(member.value_type),
                            };
                            let Some(value_type) = unified else {
                                return fail(
                                    annotation.span(),
                                    "the types in this union cannot be stored together",
                                );
                            };
                            DeclaredType {
                                value_type,
                                hint: if previous.hint == NumberHint::Plain {
                                    member.hint
                                } else {
                                    previous.hint
                                },
                                is_array: previous.is_array || member.is_array,
                            }
                        }
                    });
                }
                Ok(combined)
            }
            TSType::TSTypeReference(reference) => {
                let TSTypeName::IdentifierReference(identifier) = &reference.type_name else {
                    return Ok(None);
                };
                let name = identifier.name.as_str();
                if let Some(alias) = self
                    .modules
                    .get(module)
                    .and_then(|module| module.type_aliases.get(name))
                {
                    return self.declared_type(alias, module);
                }
                if let Some(set) = ChoiceSet::for_type_name(name) {
                    return scalar(Kind::Choice(set), NumberHint::Plain);
                }
                match name {
                    "Instant" => scalar(Kind::Number, NumberHint::Instant),
                    "Duration" => scalar(Kind::Number, NumberHint::Duration),
                    "Color" | "ThemeColorName" => scalar(Kind::Color, NumberHint::Plain),
                    "LibraryIcon" | "IconName" => scalar(Kind::Icon, NumberHint::Plain),
                    "Array" => match reference.type_arguments.as_ref().and_then(|arguments| arguments.params.first()) {
                        Some(element) => Ok(self.declared_type(element, module)?.map(|element| DeclaredType {
                            is_array: true,
                            ..element
                        })),
                        None => Ok(None),
                    },
                    _ => fail(
                        annotation.span(),
                        format!("`{name}` cannot be kept in state yet; state holds numbers, booleans, text, colours, icons and arrays of them"),
                    ),
                }
            }
            _ => Ok(None),
        }
    }

    // ------------------------------------------------------------------------
    // The app
    // ------------------------------------------------------------------------

    /// Reads the app's id, name and kind, and starts the image with them.
    fn start_image(&mut self, options: &'a ObjectExpression<'a>, root: &Rc<Scope<'a>>) -> Compiled<()> {
        let text_option = |compiler: &Compiler<'a>, name: &str| -> Compiled<Option<String>> {
            match option(options, name) {
                Some(value) => match compiler.evaluate_constant(value, root) {
                    Some(Constant::Text(text)) => Ok(Some(text.to_string())),
                    _ => fail(value.span(), format!("`{name}` must be literal text")),
                },
                None => Ok(None),
            }
        };
        let Some(id) = text_option(self, "id")? else {
            return fail(options.span, "define_app needs an `id`");
        };
        let name = text_option(self, "name")?.unwrap_or_else(|| id.clone());
        let kind = match text_option(self, "kind")?.as_deref() {
            None | Some("app") => AppKind::App,
            Some("face") => AppKind::Face,
            Some(other) => {
                return fail(
                    options.span,
                    format!("`kind` must be \"app\" or \"face\", not \"{other}\""),
                )
            }
        };
        self.writer = ImageWriter::new(&id, &name, kind, SHARED_LIBRARY_VERSION);
        Ok(())
    }

    /// Compiles every screen of the app and writes the widget tree.
    pub fn compile_app(&mut self, module: usize, app_span: Span) -> Compiled<()> {
        let Some(options) = self.modules[module].app_options else {
            return fail(
                app_span,
                "the file must end with `export default define_app({ ... })`",
            );
        };
        let root = Scope::module_root(module);

        let mut lifecycle: Vec<(SubscriptionKind, &'a Expression<'a>)> = Vec::new();
        for property in &options.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return fail(
                    options.span,
                    "write define_app's options out one by one, without `...`",
                );
            };
            let Some(name) = static_key(&property.key) else {
                return fail(property.span, "write define_app's options with plain names");
            };
            match name.as_str() {
                "id" | "name" | "kind" | "screens" | "icon" | "subscriptions" | "permissions" => {}
                other => match lifecycle_kind(other) {
                    Some(kind) => lifecycle.push((kind, &property.value)),
                    None => return fail(property.span, format!("define_app has no option `{other}`")),
                },
            }
        }

        // The icon is image 0, so it goes in before any image a screen shows.
        if let Some(icon) = option(options, "icon") {
            self.add_app_icon(icon, &root)?;
        }
        for (kind, handler) in lifecycle {
            self.compile_lifecycle(kind, handler, &root)?;
        }
        if let Some(subscriptions) = option(options, "subscriptions") {
            self.compile_subscriptions(subscriptions, &root)?;
        }

        let Some(screens) = option(options, "screens") else {
            return fail(
                options.span,
                "define_app needs `screens`, such as `screens: { main: Main }`",
            );
        };
        let Expression::ObjectExpression(screens) = strip_parentheses(screens) else {
            return fail(
                screens.span(),
                "write `screens` as an object, such as `{ main: Main }`",
            );
        };
        let mut ordered: Vec<(String, &'a Expression<'a>, Span)> = Vec::new();
        for property in &screens.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return fail(screens.span, "write each screen out by name");
            };
            let Some(name) = static_key(&property.key) else {
                return fail(property.span, "give each screen a plain name");
            };
            let value = &property.value;
            ordered.push((name, value, property.span));
        }
        let Some(main_position) = ordered.iter().position(|(name, _, _)| name == "main") else {
            return fail(
                screens.span,
                "one screen must be called `main`; the app opens on it",
            );
        };
        let main = ordered.remove(main_position);
        ordered.insert(0, main);

        let mut trees: Vec<WidgetNode> = Vec::new();
        for (name, component, span) in ordered {
            let mut tree = self.compile_screen(component, &root, span)?;
            let name_string = self.string(&name);
            tree.properties.insert(
                0,
                (Property::ScreenName, PropertySource::Constant(name_string as i32)),
            );
            trees.push(tree);
        }

        self.check_state_limits()?;
        let widget_count: usize = trees.iter().map(WidgetNode::count).sum();
        if widget_count > MOST_WIDGETS {
            return fail(
                screens.span,
                format!("the app has {widget_count} widgets and at most {MOST_WIDGETS} fit; draw a board with one widget per row, or fewer cells"),
            );
        }
        for tree in &trees {
            self.write_widget(tree, screens.span)?;
        }
        self.check_permissions(option(options, "permissions"), &root, options.span)
    }

    fn write_widget(&mut self, node: &WidgetNode, span: Span) -> Compiled<()> {
        if node.children.len() > u8::MAX as usize || node.properties.len() > u8::MAX as usize {
            return fail(span, "a widget has too many children or properties");
        }
        self.writer
            .widget(node.kind, node.children.len() as u8, &node.properties);
        for child in &node.children {
            self.write_widget(child, span)?;
        }
        Ok(())
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// The value of an option in an object literal, by name.
pub fn option<'a>(object: &'a ObjectExpression<'a>, name: &str) -> Option<&'a Expression<'a>> {
    object.properties.iter().find_map(|property| match property {
        ObjectPropertyKind::ObjectProperty(property)
            if static_key(&property.key).as_deref() == Some(name) =>
        {
            Some(&property.value)
        }
        _ => None,
    })
}

/// A function declaration as a reference that keeps its scope.
pub fn function_reference<'a>(
    function: &'a Function<'a>,
    scope: &Rc<Scope<'a>>,
) -> Compiled<FunctionReference<'a>> {
    if function.generator {
        return fail(
            function.span,
            "generators are not available; use `for ... of` over a fixed-length array",
        );
    }
    let Some(body) = &function.body else {
        return fail(function.span, "the function needs a body");
    };
    Ok(FunctionReference {
        name: function
            .id
            .as_ref()
            .map_or(String::new(), |id| id.name.as_str().to_string()),
        parameters: &function.params,
        body: FunctionBodyReference::Statements(body),
        scope: scope.clone(),
        span: function.span,
        is_async: function.r#async,
    })
}

/// An arrow or function expression as a reference that keeps its scope.
pub fn callable_reference<'a>(
    name: &str,
    expression: &'a Expression<'a>,
    scope: &Rc<Scope<'a>>,
) -> Compiled<FunctionReference<'a>> {
    match strip_parentheses(expression) {
        Expression::ArrowFunctionExpression(arrow) => Ok(FunctionReference {
            name: name.to_string(),
            parameters: &arrow.params,
            body: match &arrow.body {
                ArrowFunctionBody::FunctionBody(body) => FunctionBodyReference::Statements(body),
                other => match other.as_expression() {
                    Some(expression) => FunctionBodyReference::Expression(expression),
                    None => return fail(arrow.span, "this arrow function has no body"),
                },
            },
            scope: scope.clone(),
            span: arrow.span,
            is_async: arrow.r#async,
        }),
        Expression::FunctionExpression(function) => {
            let mut reference = function_reference(function, scope)?;
            if reference.name.is_empty() {
                reference.name = name.to_string();
            }
            Ok(reference)
        }
        other => fail(other.span(), "expected a function here, such as `() => ...`"),
    }
}

/// The type a starting value implies when no type is written.
pub fn infer_declared_type(value: &Constant) -> Option<DeclaredType> {
    let scalar = |value: &Constant| -> Option<Type> {
        Some(match value {
            Constant::Number(_) => Type::NUMBER,
            Constant::Bool(_) => Type::BOOL,
            Constant::Text(text)
                if text.starts_with('#') && crate::types::parse_hex_color(text).is_some() =>
            {
                Type::COLOR
            }
            Constant::Text(_) => Type::NAME,
            Constant::Icon(_) => Type::ICON,
            _ => return None,
        })
    };
    match value {
        Constant::Array(elements) => Some(DeclaredType {
            value_type: scalar(elements.first()?)?,
            hint: NumberHint::Plain,
            is_array: true,
        }),
        other => Some(DeclaredType {
            value_type: scalar(other)?,
            hint: NumberHint::Plain,
            is_array: false,
        }),
    }
}

/// The values a type can hold, for choosing array widths.
pub fn type_range(value_type: Type) -> Range {
    let base = match value_type.kind {
        Kind::Bool => Range {
            lowest: 0,
            highest: 1,
        },
        Kind::Choice(set) => Range {
            lowest: 0,
            highest: set.names().len() as i64 - 1,
        },
        Kind::Icon => Range {
            lowest: crate::types::icon_number(crate::types::ICON_NAMES[0]).unwrap_or(0),
            highest: crate::types::icon_number(crate::types::ICON_NAMES[crate::types::ICON_NAMES.len() - 1])
                .unwrap_or(0),
        },
        Kind::Text | Kind::Name => Range {
            lowest: 0,
            highest: u16::MAX as i64,
        },
        Kind::Color => Range {
            lowest: 0,
            highest: (1 << 16) + 7,
        },
        Kind::Number | Kind::Null => Range::ANY,
    };
    if value_type.nullable {
        base.union(Range::exactly(neotime_runtime::NONE))
    } else {
        base
    }
}

/// A colour written as `#rrggbb` or a theme colour's name, as the number it is stored as.
pub fn color_constant(text: &str) -> Option<i64> {
    use neotime_runtime::{ColorValue, ThemeColor};
    if let Some(rgb565) = crate::types::parse_hex_color(text) {
        return Some(ColorValue::Fixed(rgb565).to_value());
    }
    ThemeColor::from_name(text).map(|color| ColorValue::Theme(color).to_value())
}

pub fn describe_constant(constant: &Constant) -> String {
    match constant {
        Constant::Number(number) => format!("the number {number}"),
        Constant::Bool(value) => format!("`{value}`"),
        Constant::Text(text) => format!("the text \"{text}\""),
        Constant::Null => "null".to_string(),
        Constant::Undefined => "undefined".to_string(),
        Constant::Icon(_) => "an icon".to_string(),
        Constant::Array(_) => "an array".to_string(),
        Constant::Object(_) => "an object".to_string(),
    }
}
