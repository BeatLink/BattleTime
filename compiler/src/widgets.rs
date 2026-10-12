//! Turns each screen's JSX into a fixed widget tree: components are inlined, `{cond && <W/>}`
//! becomes the widget's `visible` property, and `.map` over a fixed-length array is unrolled.

use std::rc::Rc;

use neotime_runtime::events::EventKind;
use neotime_runtime::instruction::Instruction;
use neotime_runtime::widgets::PropertyValue;
use neotime_runtime::{Property, PropertySource, WidgetKind};
use oxc_ast::ast::{
    CallExpression, Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElement,
    JSXElementName, JSXExpression, Statement,
};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::LogicalOperator;

use crate::compiler::{infer_declared_type, Compiler};
use crate::constants::{strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled, Diagnostic};
use crate::emitter::{CodeKind, Emitter};
use crate::expressions::Typed;
use crate::scope::{FunctionBodyReference, FunctionReference, LibraryModule, Reference, Scope};
use crate::statements::ArgumentSource;
use crate::types::{percentage_length, ChoiceSet, Kind, Type};

// ============================================================================
// Widget trees
// ============================================================================

/// One widget with its properties and children, before it is written into the image.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetNode {
    pub kind: WidgetKind,
    pub properties: Vec<(Property, PropertySource)>,
    pub children: Vec<WidgetNode>,
}

impl WidgetNode {
    /// How many widgets this one and everything inside it make.
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(WidgetNode::count).sum::<usize>()
    }
}

/// A test a widget's `visible` property must pass, from a `&&` or `? :` around it.
#[derive(Clone, Debug)]
struct Condition<'a> {
    expression: &'a Expression<'a>,
    scope: Rc<Scope<'a>>,
    negated: bool,
}

/// A piece of the text inside a `Label` or `Button`.
#[derive(Clone, Debug)]
enum TextPart<'a> {
    Literal(String),
    Expression(&'a Expression<'a>, Rc<Scope<'a>>),
}

/// What a stretch of JSX makes: widgets, and any text between them.
#[derive(Default)]
struct Rendered<'a> {
    widgets: Vec<WidgetNode>,
    text: Vec<TextPart<'a>>,
}

/// The widget a name from `neotime/widgets` makes.
fn widget_kind(name: &str) -> Option<WidgetKind> {
    use WidgetKind::*;
    Some(match name {
        "Screen" | "Column" => Column,
        "Row" => Row,
        "Grid" => Grid,
        "Stack" => Stack,
        "Scroll" => Scroll,
        "List" => List,
        "Pages" => Pages,
        "Tabs" => Tabs,
        "Label" => Label,
        "RichText" => RichText,
        "Image" => Image,
        "Icon" => Icon,
        "AnimatedImage" => AnimatedImage,
        "Line" => Line,
        "Arc" => Arc,
        "ArcLabel" => ArcLabel,
        "Bar" => Bar,
        "Spinner" => Spinner,
        "Led" => Led,
        "Chart" => Chart,
        "Scale" => Scale,
        "AnalogClock" => AnalogClock,
        "Table" => Table,
        "Calendar" => Calendar,
        "Button" => Button,
        "ImageButton" => ImageButton,
        "ClickArea" => ClickArea,
        "Switch" => Switch,
        "Checkbox" => Checkbox,
        "Radio" => Radio,
        "Slider" => Slider,
        "Roller" => Roller,
        "Spinbox" => Spinbox,
        "Dropdown" => Dropdown,
        "ButtonMatrix" => ButtonMatrix,
        "TextArea" => TextArea,
        "Keyboard" => Keyboard,
        "CellGrid" => CellGrid,
        "AppList" => AppList,
        _ => return None,
    })
}

/// The property a prop name sets, and the type its value must have.
fn property_for(name: &str) -> Option<(Property, Type)> {
    use Property::*;
    let number = Type::NUMBER;
    Some(match name {
        "x" => (X, number),
        "y" => (Y, number),
        "width" => (Width, number),
        "height" => (Height, number),
        "grow" => (Grow, number),
        "padding" => (Padding, number),
        "margin" => (Margin, number),
        "background" => (Background, Type::COLOR),
        "color" => (Color, Type::COLOR),
        "border_color" => (BorderColor, Type::COLOR),
        "border_width" => (BorderWidth, number),
        "radius" => (Radius, number),
        "font" => (Font, Type::choice(ChoiceSet::Font)),
        "align" => (Align, Type::choice(ChoiceSet::Align)),
        "text_align" => (Align, Type::choice(ChoiceSet::TextAlign)),
        "visible" => (Visible, Type::BOOL),
        "animate" => (Animate, number),
        "text" => (Text, Type::TEXT),
        "value" => (Value, number),
        "minimum" => (Minimum, number),
        "maximum" => (Maximum, number),
        "step" => (Step, number),
        "icon" => (Icon, Type::ICON),
        "time" => (Time, number),
        "show_seconds" => (ShowSeconds, Type::BOOL),
        "start_angle" => (StartAngle, number),
        "end_angle" => (EndAngle, number),
        "long_mode" => (LongMode, Type::choice(ChoiceSet::LongMode)),
        "opacity" => (Opacity, number),
        "disabled" => (Disabled, Type::BOOL),
        "gap" => (Gap, number),
        "justify" => (Justify, Type::choice(ChoiceSet::Justify)),
        "align_items" => (AlignItems, Type::choice(ChoiceSet::AlignItems)),
        "columns" => (Columns, number),
        "rows" => (Rows, number),
        "cell_size" => (CellSize, number),
        "cell_gap" => (CellGap, number),
        "cell_shape" => (CellShape, Type::choice(ChoiceSet::CellShape)),
        "ticks" => (Ticks, number),
        "major_every" => (MajorEvery, number),
        "needle_color" => (NeedleColor, Type::COLOR),
        "hour_color" => (HourColor, Type::COLOR),
        "minute_color" => (MinuteColor, Type::COLOR),
        "second_color" => (SecondColor, Type::COLOR),
        "hour_length" => (HourLength, number),
        "minute_length" => (MinuteLength, number),
        "second_length" => (SecondLength, number),
        "hand_style" => (HandStyle, Type::choice(ChoiceSet::HandStyle)),
        "recolor" => (Recolor, Type::COLOR),
        "major_color" => (MajorColor, Type::COLOR),
        "onClick" => (OnClick, number),
        "onLongPress" => (OnLongPress, number),
        "onDoubleClick" => (OnDoubleClick, number),
        "onPress" => (OnPress, number),
        "onRelease" => (OnRelease, number),
        "onDrag" => (OnDrag, number),
        "onSwipe" => (OnSwipe, number),
        "onButton" => (OnButton, number),
        "onChange" => (OnChange, number),
        "onPageChange" => (OnPageChange, number),
        "onScrollEnd" => (OnScrollEnd, number),
        "onShow" => (OnShow, number),
        "onHide" => (OnHide, number),
        _ => return None,
    })
}

/// Props that set other props: an Icon's `size` is its width and height, and so on.
fn alias_targets(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "size" | "diameter" => &["width", "height"],
        "thickness" => &["border_width"],
        "track_color" => &["background"],
        _ => return None,
    })
}

/// The props written on a widget with each alias replaced by what it sets, unless that is written too.
fn expand_aliases<'a>(attributes: Vec<(String, Reference<'a>)>) -> Vec<(String, Reference<'a>)> {
    let written: Vec<String> = attributes.iter().map(|(name, _)| name.clone()).collect();
    let mut expanded = Vec::new();
    for (name, value) in attributes {
        match alias_targets(&name) {
            Some(targets) => {
                for target in targets {
                    if !written.iter().any(|written| written == target) {
                        expanded.push((target.to_string(), value.clone()));
                    }
                }
            }
            None => expanded.push((name, value)),
        }
    }
    expanded
}

/// How a prop given as an array is laid out in the state array a property names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArrayLayout {
    /// One element per element.
    Plain,
    /// `{ x, y }` points, as x and y one after the other.
    Points,
    /// Rows of labels, with a `"\n"` element between rows, as LVGL's button matrix map has.
    Rows,
}

/// The array property a prop sets, the type of its elements and how they are laid out.
fn array_property_for(name: &str, kind: WidgetKind) -> Option<(Property, Type, ArrayLayout)> {
    Some(match name {
        "cells" => (Property::Cells, Type::NUMBER, ArrayLayout::Plain),
        "palette" => (Property::Palette, Type::COLOR, ArrayLayout::Plain),
        "options" => (Property::Options, Type::NAME, ArrayLayout::Plain),
        "rows" if kind == WidgetKind::ButtonMatrix => (Property::Options, Type::NAME, ArrayLayout::Rows),
        "points" => (Property::Points, Type::NUMBER, ArrayLayout::Points),
        "values" => (Property::Series, Type::NUMBER, ArrayLayout::Plain),
        "labels" => (Property::Labels, Type::NAME, ArrayLayout::Plain),
        "skip" => (Property::Skip, Type::NUMBER, ArrayLayout::Plain),
        _ => return None,
    })
}

/// JSX text as Preact shows it: lines trimmed, blank lines dropped, the rest joined by spaces.
fn jsx_text(raw: &str) -> String {
    let lines: Vec<&str> = raw.split('\n').collect();
    let last = lines.len() - 1;
    let mut pieces: Vec<String> = Vec::new();
    for (position, line) in lines.iter().enumerate() {
        let mut piece = line.replace('\t', " ");
        if position != 0 {
            piece = piece.trim_start().to_string();
        }
        if position != last {
            piece = piece.trim_end().to_string();
        }
        if !piece.is_empty() {
            pieces.push(piece);
        }
    }
    pieces.join(" ")
}

// ============================================================================
// Screens and components
// ============================================================================

impl<'a> Compiler<'a> {
    /// Renders one screen's component into its widget tree.
    pub fn compile_screen(
        &mut self,
        component: &'a Expression<'a>,
        root: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<WidgetNode> {
        let Some(Reference::Function(function)) = self.resolve_reference(component, root) else {
            return fail(
                component.span(),
                "a screen must be a component function, such as `main: Main`",
            );
        };
        let mut rendered = Rendered::default();
        self.render_function(&function, Vec::new(), &[], true, &mut rendered, span)?;
        if !rendered.text.is_empty() {
            return fail(
                function.span,
                "a screen's component must return one widget, such as `<Screen>`, not text",
            );
        }
        match rendered.widgets.len() {
            1 => Ok(rendered.widgets.remove(0)),
            _ => fail(
                function.span,
                "a screen's component must always return the same one widget, such as `<Screen>...</Screen>`; choose what it shows inside it with `&&` instead of an early `return`",
            ),
        }
    }

    /// Renders a component or `.map` callback with its parameters bound to the references given.
    fn render_function(
        &mut self,
        function: &FunctionReference<'a>,
        arguments: Vec<Reference<'a>>,
        conditions: &[Condition<'a>],
        is_screen: bool,
        out: &mut Rendered<'a>,
        span: Span,
    ) -> Compiled<()> {
        let identity = function.identity();
        if self.inline_stack.contains(&identity) {
            return fail(span, "a component cannot contain itself");
        }
        self.inline_stack.push(identity);
        let result = (|| {
            let mut scope = function.scope.clone();
            let mut arguments = arguments.into_iter();
            for parameter in &function.parameters.items {
                let reference = arguments
                    .next()
                    .unwrap_or(Reference::Constant(Constant::Undefined));
                scope = self.bind_component_pattern(&parameter.pattern, reference, &scope)?;
            }
            match function.body {
                FunctionBodyReference::Expression(expression) => {
                    self.render_expression(expression, &scope, conditions, is_screen, out)
                }
                FunctionBodyReference::Statements(body) => {
                    self.render_component_body(&body.statements, scope, conditions.to_vec(), is_screen, out)
                }
            }
        })();
        self.inline_stack.pop();
        result
    }

    fn bind_component_pattern(
        &mut self,
        pattern: &'a oxc_ast::ast::BindingPattern<'a>,
        reference: Reference<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<Rc<Scope<'a>>> {
        use oxc_ast::ast::BindingPattern;
        match pattern {
            BindingPattern::BindingIdentifier(identifier) => {
                Ok(scope.with(identifier.name.as_str(), reference))
            }
            BindingPattern::AssignmentPattern(assignment) => {
                let reference = match reference {
                    Reference::Constant(Constant::Undefined) => Reference::Expression {
                        expression: &assignment.right,
                        scope: scope.clone(),
                    },
                    other => other,
                };
                self.bind_component_pattern(&assignment.left, reference, scope)
            }
            BindingPattern::ObjectPattern(object) => {
                let mut scope = scope.clone();
                for property in &object.properties {
                    let Some(name) = crate::constants::static_key(&property.key) else {
                        return fail(property.span, "name each prop you take apart");
                    };
                    let field = match &reference {
                        Reference::Object(fields) => fields
                            .iter()
                            .find(|(field, _)| *field == name)
                            .map_or(Reference::Constant(Constant::Undefined), |(_, field)| {
                                field.clone()
                            }),
                        Reference::Constant(constant) => {
                            Reference::Constant(constant.field(&name).unwrap_or(Constant::Undefined))
                        }
                        _ => return fail(property.span, "only props can be taken apart here"),
                    };
                    scope = self.bind_component_pattern(&property.value, field, &scope)?;
                }
                Ok(scope)
            }
            BindingPattern::ArrayPattern(array) => {
                fail(array.span, "taking arrays apart with `[a, b]` is not supported")
            }
        }
    }

    /// A component's body: constants, `useSignal`, early `return null`, and the JSX it returns.
    fn render_component_body(
        &mut self,
        statements: &'a [Statement<'a>],
        scope: Rc<Scope<'a>>,
        conditions: Vec<Condition<'a>>,
        is_screen: bool,
        out: &mut Rendered<'a>,
    ) -> Compiled<()> {
        let mut scope = scope;
        let mut conditions = conditions;
        for statement in statements {
            if let Statement::FunctionDeclaration(function) = statement {
                if let Some(identifier) = &function.id {
                    let reference = crate::compiler::function_reference(function, &scope)?;
                    scope = scope.with(identifier.name.as_str(), Reference::Function(reference));
                }
            }
        }
        for statement in statements {
            match statement {
                Statement::VariableDeclaration(declaration) => {
                    for declarator in &declaration.declarations {
                        let Some(init) = &declarator.init else {
                            return fail(declarator.span, "give the constant a value");
                        };
                        let reference = self.component_constant(init, &scope, declarator)?;
                        scope = self.bind_component_pattern(&declarator.id, reference, &scope)?;
                    }
                }
                Statement::FunctionDeclaration(_) | Statement::EmptyStatement(_) => {}
                Statement::ReturnStatement(statement) => {
                    if let Some(argument) = &statement.argument {
                        self.render_expression(argument, &scope, &conditions, is_screen, out)?;
                    }
                    return Ok(());
                }
                Statement::IfStatement(statement) => {
                    let returns = |branch: &'a Statement<'a>| -> Option<Option<&'a Expression<'a>>> {
                        match branch {
                            Statement::ReturnStatement(returned) => Some(returned.argument.as_ref()),
                            Statement::BlockStatement(block) => match block.body.as_slice() {
                                [Statement::ReturnStatement(returned)] => Some(returned.argument.as_ref()),
                                _ => None,
                            },
                            _ => None,
                        }
                    };
                    let (Some(returned), None) = (returns(&statement.consequent), &statement.alternate)
                    else {
                        return fail(
                            statement.span,
                            "a component may only use `if (...) return ...;` before its main `return`; choose widgets with `&&` or `? :` instead",
                        );
                    };
                    let mut taken = conditions.clone();
                    taken.push(Condition {
                        expression: &statement.test,
                        scope: scope.clone(),
                        negated: false,
                    });
                    if let Some(returned) = returned {
                        self.render_expression(returned, &scope, &taken, is_screen, out)?;
                    }
                    conditions.push(Condition {
                        expression: &statement.test,
                        scope: scope.clone(),
                        negated: true,
                    });
                }
                other => {
                    return fail(
                        other.span(),
                        "a component may only declare constants and return JSX; put other work in a handler",
                    )
                }
            }
        }
        Ok(())
    }

    /// What a `const` inside a component stands for: a new `useSignal` slot or an alias.
    fn component_constant(
        &mut self,
        init: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        declarator: &'a oxc_ast::ast::VariableDeclarator<'a>,
    ) -> Compiled<Reference<'a>> {
        if let Expression::CallExpression(call) = strip_parentheses(init) {
            let callee = self.resolve_reference(&call.callee, scope);
            if let Some(Reference::Library { module, name }) = &callee {
                match (module, name.as_str()) {
                    (LibraryModule::Signals, "useSignal") => return self.use_signal(call, scope, declarator.span),
                    (LibraryModule::PreactHooks, "useState" | "useReducer" | "useRef") => {
                        return fail(call.span, format!("`{name}` is not available; use `signal` at the top of the file, or `useSignal`"))
                    }
                    (LibraryModule::PreactHooks, "useMemo" | "useCallback") => {
                        return fail(call.span, format!("`{name}` is not available; use `computed`"))
                    }
                    (LibraryModule::PreactHooks, "useEffect" | "useLayoutEffect") => {
                        return fail(call.span, "`useEffect` is not available; use `subscriptions` in define_app, or a binding that reads `now`")
                    }
                    (LibraryModule::Signals, "signal" | "computed") | (LibraryModule::Neotime, "persisted") => {
                        return fail(call.span, format!("`{name}` inside a component would make a new one each time; declare it at the top of the file"))
                    }
                    _ => {}
                }
            }
        }
        if let Some(constant) = self.evaluate_constant(init, scope) {
            return Ok(Reference::Constant(constant));
        }
        if let Some(reference) = self.resolve_reference(init, scope) {
            return Ok(reference);
        }
        Ok(Reference::Expression {
            expression: init,
            scope: scope.clone(),
        })
    }

    /// `useSignal(start)` inside a component: a state slot for that place in the tree.
    fn use_signal(
        &mut self,
        call: &'a CallExpression<'a>,
        scope: &Rc<Scope<'a>>,
        span: Span,
    ) -> Compiled<Reference<'a>> {
        let Some(start) = call
            .arguments
            .first()
            .and_then(|argument| argument.as_expression())
        else {
            return fail(call.span, "give `useSignal` a starting value");
        };
        let Some(start_value) = self.evaluate_constant(start, scope) else {
            return fail(
                start.span(),
                "a signal's starting value must be known when the app is compiled",
            );
        };
        self.use_signal_count += 1;
        let key = format!("use_signal_{}", self.use_signal_count);
        if let Some(record) = self.add_record_slots(&key, false, &start_value, span) {
            return record;
        }
        let declared = match call
            .type_arguments
            .as_ref()
            .and_then(|arguments| arguments.params.first())
        {
            Some(annotation) => self.declared_type(annotation, scope.module)?,
            None => None,
        };
        let Some(declared) = declared.or_else(|| infer_declared_type(&start_value)) else {
            return fail(
                call.span,
                "say what the signal holds, such as `useSignal<number | null>(null)`",
            );
        };
        let slot = self.add_signal_slot(key, false, declared, &start_value, span)?;
        Ok(Reference::Signal(slot))
    }

    // ------------------------------------------------------------------------
    // JSX
    // ------------------------------------------------------------------------

    /// Whether an expression inside JSX makes widgets rather than text.
    fn is_widget_expression(&self, expression: &'a Expression<'a>, scope: &Rc<Scope<'a>>) -> bool {
        match strip_parentheses(expression) {
            Expression::JSXElement(_) | Expression::JSXFragment(_) => true,
            Expression::LogicalExpression(logical) => self.is_widget_expression(&logical.right, scope),
            Expression::ConditionalExpression(conditional) => {
                self.is_widget_expression(&conditional.consequent, scope)
                    || self.is_widget_expression(&conditional.alternate, scope)
            }
            Expression::CallExpression(call) if self.array_from_length(call, scope).is_some() => {
                let Some((_, callback)) = self.array_from_length(call, scope) else {
                    return false;
                };
                self.callback_makes_widgets(callback, scope)
            }
            Expression::CallExpression(call) => match strip_parentheses(&call.callee) {
                Expression::StaticMemberExpression(member) if member.property.name == "map" => match call
                    .arguments
                    .first()
                    .and_then(|argument| argument.as_expression())
                {
                    Some(callback) => match self.resolve_reference(callback, scope) {
                        Some(Reference::Function(function)) => match function.body {
                            FunctionBodyReference::Expression(body) => {
                                self.is_widget_expression(body, &function.scope)
                            }
                            FunctionBodyReference::Statements(_) => true,
                        },
                        _ => false,
                    },
                    None => false,
                },
                _ => false,
            },
            other => match self.resolve_reference(other, scope) {
                Some(Reference::Children { .. }) => true,
                Some(Reference::Expression { expression, scope }) => {
                    self.is_widget_expression(expression, &scope)
                }
                _ => false,
            },
        }
    }

    fn render_expression(
        &mut self,
        expression: &'a Expression<'a>,
        scope: &Rc<Scope<'a>>,
        conditions: &[Condition<'a>],
        is_screen: bool,
        out: &mut Rendered<'a>,
    ) -> Compiled<()> {
        if let Some(constant) = self.evaluate_constant(expression, scope) {
            return match constant {
                Constant::Null | Constant::Undefined | Constant::Bool(_) => Ok(()),
                Constant::Text(text) => {
                    out.text.push(TextPart::Literal(text.to_string()));
                    Ok(())
                }
                Constant::Number(number) => {
                    out.text.push(TextPart::Literal(number.to_string()));
                    Ok(())
                }
                _ => fail(
                    expression.span(),
                    "this value cannot be shown; show one element of it in a Label",
                ),
            };
        }
        let stripped = strip_parentheses(expression);
        match stripped {
            Expression::JSXElement(element) => {
                self.render_element(element, scope, conditions, is_screen, out)
            }
            Expression::JSXFragment(fragment) => {
                self.render_children(&fragment.children, scope, conditions, out)
            }
            Expression::LogicalExpression(logical)
                if logical.operator == LogicalOperator::And
                    && self.is_widget_expression(&logical.right, scope) =>
            {
                let mut inner = conditions.to_vec();
                inner.push(Condition {
                    expression: &logical.left,
                    scope: scope.clone(),
                    negated: false,
                });
                self.render_expression(&logical.right, scope, &inner, false, out)
            }
            Expression::ConditionalExpression(conditional) if self.is_widget_expression(stripped, scope) => {
                for (branch, negated) in [(&conditional.consequent, false), (&conditional.alternate, true)] {
                    let mut inner = conditions.to_vec();
                    inner.push(Condition {
                        expression: &conditional.test,
                        scope: scope.clone(),
                        negated,
                    });
                    self.render_expression(branch, scope, &inner, false, out)?;
                }
                Ok(())
            }
            Expression::CallExpression(call) if self.is_widget_expression(stripped, scope) => {
                self.render_map(call, scope, conditions, out)
            }
            other => match self.resolve_reference(other, scope) {
                Some(Reference::Children { children, scope }) => {
                    self.render_children(children, &scope, conditions, out)
                }
                Some(Reference::Expression { expression, scope })
                    if self.is_widget_expression(expression, &scope) =>
                {
                    self.render_expression(expression, &scope, conditions, is_screen, out)
                }
                _ => {
                    if !conditions.is_empty() {
                        return fail(
                            expression.span(),
                            "text cannot be shown only sometimes; put it in a Label with `&&` around the Label",
                        );
                    }
                    out.text.push(TextPart::Expression(expression, scope.clone()));
                    Ok(())
                }
            },
        }
    }

    fn render_children(
        &mut self,
        children: &'a [JSXChild<'a>],
        scope: &Rc<Scope<'a>>,
        conditions: &[Condition<'a>],
        out: &mut Rendered<'a>,
    ) -> Compiled<()> {
        for child in children {
            match child {
                JSXChild::Text(text) => {
                    let text = jsx_text(text.value.as_str());
                    if !text.is_empty() {
                        if !conditions.is_empty() {
                            return fail(
                                child.span(),
                                "text cannot be shown only sometimes; put it in a Label",
                            );
                        }
                        out.text.push(TextPart::Literal(text));
                    }
                }
                JSXChild::Element(element) => self.render_element(element, scope, conditions, false, out)?,
                JSXChild::Fragment(fragment) => {
                    self.render_children(&fragment.children, scope, conditions, out)?
                }
                JSXChild::ExpressionContainer(container) => match &container.expression {
                    JSXExpression::EmptyExpression(_) => {}
                    other => {
                        let Some(expression) = other.as_expression() else {
                            continue;
                        };
                        self.render_expression(expression, scope, conditions, false, out)?;
                    }
                },
                JSXChild::Spread(spread) => return fail(spread.span, "`{...children}` is not supported"),
            }
        }
        Ok(())
    }

    /// `array.map((element, index) => <Widget/>)`, unrolled once for each element.
    /// Whether a `.map` callback returns widgets.
    fn callback_makes_widgets(&self, callback: &'a Expression<'a>, scope: &Rc<Scope<'a>>) -> bool {
        match self.resolve_reference(callback, scope) {
            Some(Reference::Function(function)) => match function.body {
                FunctionBodyReference::Expression(body) => self.is_widget_expression(body, &function.scope),
                FunctionBodyReference::Statements(_) => true,
            },
            _ => false,
        }
    }

    fn render_map(
        &mut self,
        call: &'a CallExpression<'a>,
        scope: &Rc<Scope<'a>>,
        conditions: &[Condition<'a>],
        out: &mut Rendered<'a>,
    ) -> Compiled<()> {
        if let Some((length, callback)) = self.array_from_length(call, scope) {
            let Some(Reference::Function(function)) = self.resolve_reference(callback, scope) else {
                return fail(call.span, "give `Array.from` a function that returns a widget");
            };
            for index in 0..length {
                let arguments = vec![
                    Reference::Constant(Constant::Undefined),
                    Reference::Constant(Constant::Number(index)),
                ];
                self.render_function(&function, arguments, conditions, false, out, call.span)?;
            }
            return Ok(());
        }
        let Expression::StaticMemberExpression(member) = strip_parentheses(&call.callee) else {
            return fail(call.span, "only `.map` can make repeated widgets");
        };
        let Some(Reference::Function(function)) = call
            .arguments
            .first()
            .and_then(|argument| argument.as_expression())
            .and_then(|callback| self.resolve_reference(callback, scope))
        else {
            return fail(call.span, "give `.map` a function that returns a widget");
        };
        let elements: Vec<Reference<'a>> = match self.evaluate_constant(&member.object, scope) {
            Some(Constant::Array(elements)) => elements.iter().cloned().map(Reference::Constant).collect(),
            Some(_) => return fail(member.object.span(), "only arrays can be mapped"),
            None => match self.resolve_reference(&member.object, scope) {
                Some(Reference::RecordArrayValue(fields)) => {
                    let length = fields
                        .first()
                        .map_or(0, |(_, variable)| self.slots[*variable as usize].length as i64);
                    (0..length)
                        .map(|index| Reference::RecordElement {
                            fields: fields.clone(),
                            index: crate::scope::ElementIndex::Constant(index),
                        })
                        .collect()
                }
                Some(Reference::StateArray(variable)) => {
                    let length = self.slots[variable as usize].length as i64;
                    (0..length)
                        .map(|index| Reference::StateElement { variable, index })
                        .collect()
                }
                _ => return fail(
                    member.object.span(),
                    "repeated widgets need a fixed-length array: a constant, `range(n)` or an array signal",
                ),
            },
        };
        for (index, element) in elements.into_iter().enumerate() {
            let arguments = vec![element, Reference::Constant(Constant::Number(index as i64))];
            self.render_function(&function, arguments, conditions, false, out, call.span)?;
        }
        Ok(())
    }

    fn render_element(
        &mut self,
        element: &'a JSXElement<'a>,
        scope: &Rc<Scope<'a>>,
        conditions: &[Condition<'a>],
        is_screen: bool,
        out: &mut Rendered<'a>,
    ) -> Compiled<()> {
        let span = element.span;
        let name = match &element.opening_element.name {
            JSXElementName::IdentifierReference(identifier) => identifier.name.as_str().to_string(),
            JSXElementName::Identifier(identifier) => {
                return fail(
                    span,
                    format!("`<{}>` is an HTML element, which the watch does not have; use widgets from neotime/widgets", identifier.name.as_str()),
                )
            }
            _ => return fail(span, "write each widget or component by its own name, such as `<Label>`"),
        };
        let Some(reference) = self.lookup(scope, &name) else {
            return fail(
                span,
                format!("`{name}` is not defined; import it from neotime/widgets"),
            );
        };
        let attributes = self.collect_attributes(element, scope)?;
        match reference {
            Reference::Library {
                module: LibraryModule::Widgets,
                name,
            } => {
                let node = self.build_widget(&name, attributes, element, scope, conditions, is_screen)?;
                out.widgets.push(node);
                Ok(())
            }
            Reference::Function(function) => {
                let mut fields = attributes;
                if !element.children.is_empty() {
                    fields.push((
                        "children".to_string(),
                        Reference::Children {
                            children: &element.children,
                            scope: scope.clone(),
                        },
                    ));
                }
                let props = Reference::Object(Rc::new(fields));
                self.render_function(&function, vec![props], conditions, is_screen, out, span)
            }
            Reference::Library {
                module: LibraryModule::Components,
                name,
            } => {
                // The image format cannot refer to the shared library yet, so its components are inlined.
                let component = self
                    .components_module
                    .and_then(|module| self.modules.get(module))
                    .and_then(|module| module.names.get(&name))
                    .cloned();
                let Some(Reference::Function(function)) = component else {
                    return fail(span, format!("neotime/components has no component `{name}`"));
                };
                let mut fields = attributes;
                if !element.children.is_empty() {
                    fields.push((
                        "children".to_string(),
                        Reference::Children {
                            children: &element.children,
                            scope: scope.clone(),
                        },
                    ));
                }
                let props = Reference::Object(Rc::new(fields));
                self.render_function(&function, vec![props], conditions, is_screen, out, span)
            }
            _ => fail(span, format!("`{name}` is not a widget or a component")),
        }
    }

    /// The props written on an element, each as what it stands for, with `{...spread}` expanded.
    fn collect_attributes(
        &mut self,
        element: &'a JSXElement<'a>,
        scope: &Rc<Scope<'a>>,
    ) -> Compiled<Vec<(String, Reference<'a>)>> {
        let mut attributes: Vec<(String, Reference<'a>)> = Vec::new();
        let put = |attributes: &mut Vec<(String, Reference<'a>)>, name: String, value: Reference<'a>| {
            attributes.retain(|(existing, _)| *existing != name);
            attributes.push((name, value));
        };
        for item in &element.opening_element.attributes {
            match item {
                JSXAttributeItem::Attribute(attribute) => {
                    let JSXAttributeName::Identifier(name) = &attribute.name else {
                        return fail(attribute.span, "namespaced props such as `a:b` are not supported");
                    };
                    let name = name.name.as_str().to_string();
                    let value = match &attribute.value {
                        None => Reference::Constant(Constant::Bool(true)),
                        Some(JSXAttributeValue::StringLiteral(literal)) => {
                            Reference::Constant(Constant::Text(literal.value.as_str().into()))
                        }
                        Some(JSXAttributeValue::ExpressionContainer(container)) => {
                            match &container.expression {
                                JSXExpression::EmptyExpression(_) => continue,
                                other => match other.as_expression() {
                                    Some(expression) => self.attribute_reference(expression, scope),
                                    None => continue,
                                },
                            }
                        }
                        Some(_) => return fail(attribute.span, "pass widgets as children, not as props"),
                    };
                    put(&mut attributes, name, value);
                }
                JSXAttributeItem::SpreadAttribute(spread) => {
                    match self.resolve_reference(&spread.argument, scope) {
                        Some(Reference::Object(fields)) => {
                            for (name, value) in fields.iter() {
                                if name != "children" {
                                    put(&mut attributes, name.clone(), value.clone());
                                }
                            }
                        }
                        _ => match self.evaluate_constant(&spread.argument, scope) {
                            Some(Constant::Object(fields)) => {
                                for (name, value) in fields.iter() {
                                    put(&mut attributes, name.clone(), Reference::Constant(value.clone()));
                                }
                            }
                            _ => {
                                return fail(
                                    spread.span,
                                    "only props or constant objects can be spread with `{...}`",
                                )
                            }
                        },
                    }
                }
            }
        }
        Ok(attributes)
    }

    fn attribute_reference(&self, expression: &'a Expression<'a>, scope: &Rc<Scope<'a>>) -> Reference<'a> {
        if let Some(constant) = self.evaluate_constant(expression, scope) {
            return Reference::Constant(constant);
        }
        match self.resolve_reference(expression, scope) {
            Some(reference) => reference,
            None => Reference::Expression {
                expression,
                scope: scope.clone(),
            },
        }
    }

    // ------------------------------------------------------------------------
    // Widgets
    // ------------------------------------------------------------------------

    fn build_widget(
        &mut self,
        name: &str,
        attributes: Vec<(String, Reference<'a>)>,
        element: &'a JSXElement<'a>,
        scope: &Rc<Scope<'a>>,
        conditions: &[Condition<'a>],
        is_screen: bool,
    ) -> Compiled<WidgetNode> {
        let span = element.span;
        if name == "Screen" && !is_screen {
            return fail(
                span,
                "`<Screen>` may only be the outermost widget of a screen's component",
            );
        }
        let Some(mut kind) = widget_kind(name) else {
            return fail(
                span,
                format!("`{name}` is not in the image format yet; draw it from the widgets the runtime has"),
            );
        };
        let mut properties: Vec<(Property, PropertySource)> = Vec::new();
        let mut conditions = conditions.to_vec();
        let mut bound_value: Option<u8> = None;
        let mut change_handler: Option<Reference<'a>> = None;
        let attributes = self.expand_series(expand_aliases(attributes), kind, span);

        for (prop, value) in attributes {
            if prop == "key" || prop == "children" {
                continue;
            }
            if name == "Screen" && prop == "direction" {
                if let Some(Constant::Text(direction)) = self.reference_constant(&value) {
                    kind = if &*direction == "row" {
                        WidgetKind::Row
                    } else {
                        WidgetKind::Column
                    };
                    continue;
                }
                return fail(span, "`direction` must be \"row\" or \"column\"");
            }
            if prop == "src" {
                match self.reference_constant(&value) {
                    Some(Constant::Icon(icon)) => {
                        properties.push((Property::Image, PropertySource::Constant(icon as i32)));
                    }
                    Some(Constant::Text(path)) => {
                        let number = self.app_image(&path, span)?;
                        let reference = neotime_runtime::resources::ImageReference::App(number);
                        properties.push((Property::Image, PropertySource::Constant(reference.to_value())));
                    }
                    _ => return fail(
                        span,
                        "`src` is a PNG file of the app's, such as \"./logo.png\", or a shared library icon",
                    ),
                }
                continue;
            }
            if let Some((property, element_type, layout)) = array_property_for(&prop, kind) {
                if !property.belongs_to(kind) {
                    self.warn(
                        span,
                        format!(
                            "`{prop}` on `{name}` is left out: the runtime has no `{prop}` on that widget"
                        ),
                    );
                    continue;
                }
                if let Some(source) =
                    self.array_property_source(value, element_type, layout, &prop, name, span)?
                {
                    properties.push((property, source));
                }
                continue;
            }
            if prop == "justify" {
                if let Some(Constant::Text(text)) = self.reference_constant(&value) {
                    if ChoiceSet::Justify.number_of(&text).is_none() {
                        return fail(
                            span,
                            format!("`justify=\"{text}\"` is not supported; use start, center, end or space_between"),
                        );
                    }
                }
            }
            let Some((property, value_type)) = property_for(&prop) else {
                self.warn(
                    span,
                    format!("`{prop}` on `{name}` is left out: the image format has no such property yet"),
                );
                continue;
            };
            if !property.belongs_to(kind) {
                self.warn(
                    span,
                    format!("`{prop}` on `{name}` is left out: the runtime has no `{prop}` on that widget"),
                );
                continue;
            }
            if property == Property::Visible {
                match value {
                    Reference::Expression { expression, scope } => conditions.push(Condition {
                        expression,
                        scope,
                        negated: false,
                    }),
                    Reference::Constant(constant) if constant.is_truthy() => {}
                    Reference::Constant(_) => {
                        properties.push((Property::Visible, PropertySource::Constant(0)))
                    }
                    _ => return fail(span, "`visible` must be true or false"),
                }
                continue;
            }
            if property == Property::OnChange {
                change_handler = Some(value);
                continue;
            }
            if let Some(event) = EventKind::for_property(property) {
                if let Some(handler) = self.event_property(value, event, span)? {
                    properties.push((property, PropertySource::Handler(handler)));
                }
                continue;
            }
            if property == Property::Value {
                if let Reference::Signal(variable) = value {
                    bound_value = Some(variable);
                    let source = self.read_state_expression(variable, span)?;
                    properties.push((property, source));
                    continue;
                }
            }
            if let Some(source) = self.property_source(value, value_type, property, span)? {
                properties.push((property, source));
            }
        }

        if bound_value.is_some() || change_handler.is_some() {
            if let Some(handler) = self.change_handler(bound_value, change_handler, span)? {
                properties.push((Property::OnChange, PropertySource::Handler(handler)));
            }
        }

        let mut rendered = Rendered::default();
        self.render_children(&element.children, scope, &[], &mut rendered)?;
        if !rendered.text.is_empty() {
            if !Property::Text.belongs_to(kind) {
                return fail(
                    span,
                    format!("`{name}` cannot hold text; put the text in a Label"),
                );
            }
            if properties.iter().any(|(property, _)| *property == Property::Text) {
                return fail(span, "give the text either as children or as `text`, not both");
            }
            let source = self.text_source(&rendered.text, span)?;
            properties.push((Property::Text, source));
        }
        if !rendered.widgets.is_empty() && !kind.holds_children() {
            return fail(span, format!("`{name}` cannot hold other widgets"));
        }
        if let Some(source) = self.visibility_source(&conditions, span)? {
            properties.push((Property::Visible, source));
        }
        // A property set twice, such as `align` and `text_align`, keeps the first.
        let mut seen: Vec<Property> = Vec::new();
        properties.retain(|(property, _)| {
            let first = !seen.contains(property);
            seen.push(*property);
            first
        });
        Ok(WidgetNode {
            kind,
            properties,
            children: rendered.widgets,
        })
    }

    /// A Chart's `series` as its first series' `values` and `color`, since the runtime draws one.
    fn expand_series(
        &mut self,
        attributes: Vec<(String, Reference<'a>)>,
        kind: WidgetKind,
        span: Span,
    ) -> Vec<(String, Reference<'a>)> {
        if kind != WidgetKind::Chart {
            return attributes;
        }
        let written: Vec<String> = attributes.iter().map(|(name, _)| name.clone()).collect();
        let mut expanded = Vec::new();
        for (name, value) in attributes {
            if name != "series" {
                expanded.push((name, value));
                continue;
            }
            let mut series: Vec<Vec<(String, Reference<'a>)>> = Vec::new();
            if let Some(Constant::Array(elements)) = self.reference_constant(&value) {
                for element in elements.iter() {
                    let fields = ["values", "color"]
                        .iter()
                        .filter_map(|field| {
                            Some((field.to_string(), Reference::Constant(element.field(field)?)))
                        })
                        .collect();
                    series.push(fields);
                }
            } else if let Reference::Expression { expression, scope } = &value {
                if let Expression::ArrayExpression(array) = strip_parentheses(expression) {
                    for element in &array.elements {
                        let Some(Expression::ObjectExpression(object)) =
                            element.as_expression().map(strip_parentheses)
                        else {
                            continue;
                        };
                        let mut fields = Vec::new();
                        for property in &object.properties {
                            let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(property) = property else {
                                continue;
                            };
                            if let Some(field) = crate::constants::static_key(&property.key) {
                                fields.push((field, self.attribute_reference(&property.value, scope)));
                            }
                        }
                        series.push(fields);
                    }
                }
            }
            if series.len() > 1 {
                self.warn(
                    span,
                    "only the first of a Chart's `series` is drawn: the runtime's Chart has one series",
                );
            }
            let Some(first) = series.into_iter().next() else {
                self.warn(
                    span,
                    "`series` on `Chart` is left out: write it as an array of `{ values, color }`",
                );
                continue;
            };
            for (field, reference) in first {
                if matches!(field.as_str(), "values" | "color") && !written.contains(&field) {
                    expanded.push((field, reference));
                }
            }
        }
        expanded
    }

    /// An array prop: the state array it names, made from a constant array if need be.
    fn array_property_source(
        &mut self,
        value: Reference<'a>,
        element_type: Type,
        layout: ArrayLayout,
        prop: &str,
        widget: &str,
        span: Span,
    ) -> Compiled<Option<PropertySource>> {
        if let Reference::StateArray(variable) = value {
            let slot = &self.slots[variable as usize];
            if layout != ArrayLayout::Plain || !Type::plain(slot.value_type.kind).fits(element_type) {
                return fail(
                    span,
                    format!("`{prop}` needs an array of {}", element_type.describe()),
                );
            }
            return Ok(Some(PropertySource::Constant(variable as i32)));
        }
        let constant = match self.reference_constant(&value) {
            Some(Constant::Undefined) => return Ok(None),
            Some(constant) => constant,
            None => {
                // An array worked out while running is kept in a hidden array a derive handler fills.
                let derived = match (&value, layout) {
                    (Reference::Expression { expression, scope }, ArrayLayout::Plain) => {
                        self.derived_array(expression, scope, element_type, span)?
                    }
                    (Reference::Expression { expression, scope }, ArrayLayout::Points) => {
                        self.derived_points(expression, scope, span)?
                    }
                    _ => None,
                };
                if let Some(variable) = derived {
                    return Ok(Some(PropertySource::Constant(variable as i32)));
                }
                self.warn(
                    span,
                    format!("`{prop}` on `{widget}` is left out: it changes while the app runs, and the compiler cannot tell how long it is"),
                );
                return Ok(None);
            }
        };
        let Constant::Array(elements) = constant else {
            return fail(span, format!("`{prop}` needs an array"));
        };
        let mut flat: Vec<Constant> = Vec::new();
        for (position, element) in elements.iter().enumerate() {
            match (layout, element) {
                (ArrayLayout::Plain, element) => flat.push(element.clone()),
                (ArrayLayout::Points, point) => {
                    let (Some(x), Some(y)) = (point.field("x"), point.field("y")) else {
                        return fail(span, "each point needs an `x` and a `y`");
                    };
                    flat.push(x);
                    flat.push(y);
                }
                (ArrayLayout::Rows, Constant::Array(labels)) => {
                    if position > 0 {
                        flat.push(Constant::Text("\n".into()));
                    }
                    flat.extend(labels.iter().cloned());
                }
                (ArrayLayout::Rows, _) => return fail(span, "`rows` needs an array of rows of labels"),
            }
        }
        if flat.is_empty() {
            return fail(span, format!("`{prop}` needs at least one element"));
        }
        let table = self.constant_table(&flat, element_type, span)?;
        Ok(Some(PropertySource::Constant(table as i32)))
    }

    /// An event prop: the handler compiled from the function given, or nothing if none is given.
    pub fn event_property(
        &mut self,
        value: Reference<'a>,
        event: EventKind,
        span: Span,
    ) -> Compiled<Option<u16>> {
        match value {
            Reference::Function(function) => self.compile_handler(&function, Some(event)).map(Some),
            Reference::Library {
                module: LibraryModule::Neotime,
                name,
            } => match crate::calls::effect_signature(&name) {
                Some((effect, [])) => {
                    let mut emitter = Emitter::new(CodeKind::Handler);
                    emitter.emit(Instruction::Effect(effect), span)?;
                    self.note_permission(
                        crate::permissions::effect_permission(effect),
                        format!("`{name}`"),
                        span,
                    );
                    let code = emitter.finish(span)?;
                    Ok(Some(self.add_handler(code)))
                }
                _ => fail(
                    span,
                    format!("`{name}` needs arguments; wrap it in an arrow, such as `() => {name}(...)`"),
                ),
            },
            Reference::Constant(Constant::Undefined | Constant::Null) => Ok(None),
            _ => fail(
                span,
                "an event needs a function, such as `onClick={() => ...}` or `onClick={start}`",
            ),
        }
    }

    /// The `onChange` of a widget: writes a bound signal, then runs the app's own handler.
    fn change_handler(
        &mut self,
        bound_value: Option<u8>,
        handler: Option<Reference<'a>>,
        span: Span,
    ) -> Compiled<Option<u16>> {
        let function = match handler {
            Some(Reference::Function(function)) => Some(function),
            Some(Reference::Constant(Constant::Undefined | Constant::Null)) | None => None,
            Some(_) => return fail(span, "`onChange` needs a function"),
        };
        if bound_value.is_none() && function.is_none() {
            return Ok(None);
        }
        let mut emitter = Emitter::new(CodeKind::Handler);
        if let Some(variable) = bound_value {
            let slot = &self.slots[variable as usize];
            if slot.is_array() || !matches!(slot.value_type.kind, Kind::Number | Kind::Bool) {
                return fail(
                    span,
                    "only a signal holding a number or a boolean can be bound to `value`",
                );
            }
            emitter.emit(Instruction::ReadEvent(0), span)?;
            emitter.emit(Instruction::WriteState(variable), span)?;
            self.note_write(variable, crate::types::Range::ANY);
        }
        if let Some(function) = function {
            let arguments = vec![ArgumentSource::Reference(Reference::Event(EventKind::Change))];
            self.inline_call(&mut emitter, &function, arguments, false, None, span)?;
        }
        let code = emitter.finish(span)?;
        Ok(Some(self.add_handler(code)))
    }

    fn read_state_expression(&mut self, variable: u8, span: Span) -> Compiled<PropertySource> {
        let mut emitter = Emitter::new(CodeKind::Expression);
        emitter.emit(Instruction::ReadState(variable), span)?;
        let code = emitter.finish(span)?;
        Ok(PropertySource::Expression(self.add_expression(code)))
    }

    /// The value of a property: a constant when it is known now, otherwise an expression.
    fn property_source(
        &mut self,
        value: Reference<'a>,
        value_type: Type,
        property: Property,
        span: Span,
    ) -> Compiled<Option<PropertySource>> {
        if let Some(constant) = self.reference_constant(&value) {
            if matches!(constant, Constant::Undefined) {
                return Ok(None);
            }
            if let Constant::Text(text) = &constant {
                if matches!(
                    property,
                    Property::X | Property::Y | Property::Width | Property::Height
                ) {
                    let Some(length) = percentage_length(text) else {
                        return fail(
                            span,
                            format!(
                                "`{text}` is not a length; write a number or a percentage such as \"100%\""
                            ),
                        );
                    };
                    return Ok(Some(PropertySource::Constant(length as i32)));
                }
            }
            let wanted = match (property, &constant) {
                (Property::Value, Constant::Bool(_)) => Type::BOOL,
                (Property::Text, Constant::Number(number)) => {
                    let string = self.string(&number.to_string());
                    return Ok(Some(PropertySource::Constant(string as i32)));
                }
                _ => value_type,
            };
            let number = self.constant_number(&constant, wanted, span)?;
            if property.value() == PropertyValue::Text || i32::try_from(number).is_ok() {
                return Ok(Some(PropertySource::Constant(number as i32)));
            }
        }
        let (expression, scope) = match value {
            Reference::Expression { expression, scope } => (expression, scope),
            Reference::Constant(constant) => {
                let mut emitter = Emitter::new(CodeKind::Expression);
                self.emit_constant(&mut emitter, &constant, Some(value_type), span)?;
                let code = emitter.finish(span)?;
                return Ok(Some(PropertySource::Expression(self.add_expression(code))));
            }
            other => {
                let mut emitter = Emitter::new(CodeKind::Expression);
                let typed = self.compile_reference_value(&mut emitter, other, span, Some(value_type))?;
                self.check_property_type(typed, value_type, property, span)?;
                let code = emitter.finish(span)?;
                return Ok(Some(PropertySource::Expression(self.add_expression(code))));
            }
        };
        let mut emitter = Emitter::new(CodeKind::Expression);
        let typed = self.compile_value(&mut emitter, expression, &scope, Some(value_type))?;
        if property == Property::Text {
            self.convert_to_text(&mut emitter, typed, span)?;
        } else {
            self.check_property_type(typed, value_type, property, expression.span())?;
        }
        let code = emitter.finish(span)?;
        Ok(Some(PropertySource::Expression(self.add_expression(code))))
    }

    fn check_property_type(
        &self,
        typed: Typed,
        wanted: Type,
        property: Property,
        span: Span,
    ) -> Compiled<()> {
        let fits = typed.value_type.fits(wanted)
            || (property == Property::Value
                && typed.value_type.kind == Kind::Bool
                && !typed.value_type.nullable);
        if !fits {
            return fail(
                span,
                format!(
                    "`{property:?}` needs {}, but this is {}",
                    wanted.describe(),
                    typed.value_type.describe()
                ),
            );
        }
        Ok(())
    }

    /// The text inside a Label or Button: one string when it is all known, otherwise an expression.
    fn text_source(&mut self, parts: &[TextPart<'a>], span: Span) -> Compiled<PropertySource> {
        let mut known = String::new();
        let mut all_known = true;
        for part in parts {
            match part {
                TextPart::Literal(text) => known.push_str(text),
                TextPart::Expression(expression, scope) => match self.evaluate_constant(expression, scope) {
                    Some(constant) => match constant.display_text() {
                        Some(text) => known.push_str(&text),
                        None => return fail(expression.span(), "this value cannot be shown as text"),
                    },
                    None => all_known = false,
                },
            }
        }
        if all_known {
            let string = self.string(&known);
            return Ok(PropertySource::Constant(string as i32));
        }
        let mut emitter = Emitter::new(CodeKind::Expression);
        for (position, part) in parts.iter().enumerate() {
            match part {
                TextPart::Literal(text) => {
                    let string = self.string(text);
                    emitter.emit(Instruction::PushText(string), span)?;
                }
                TextPart::Expression(expression, scope) => {
                    let typed = self.compile_value(&mut emitter, expression, scope, None)?;
                    self.convert_to_text(&mut emitter, typed, expression.span())?;
                }
            }
            if position > 0 {
                emitter.emit(Instruction::Join, span)?;
            }
        }
        let code = emitter.finish(span)?;
        Ok(PropertySource::Expression(self.add_expression(code)))
    }

    /// The `visible` property from the conditions around a widget, if it has any that are not known now.
    fn visibility_source(
        &mut self,
        conditions: &[Condition<'a>],
        span: Span,
    ) -> Compiled<Option<PropertySource>> {
        let mut unknown: Vec<&Condition<'a>> = Vec::new();
        for condition in conditions {
            match self.evaluate_constant(condition.expression, &condition.scope) {
                Some(constant) if constant.is_truthy() != condition.negated => {}
                Some(_) => return Ok(Some(PropertySource::Constant(0))),
                None => unknown.push(condition),
            }
        }
        if unknown.is_empty() {
            return Ok(None);
        }
        let mut emitter = Emitter::new(CodeKind::Expression);
        let hidden = emitter.label();
        let end = emitter.label();
        for condition in &unknown {
            self.compile_condition(&mut emitter, condition.expression, &condition.scope)?;
            if condition.negated {
                emitter.emit(Instruction::Not, span)?;
            }
            emitter.jump_if_false(hidden, span)?;
        }
        emitter.emit(Instruction::PushSmall(1), span)?;
        emitter.jump(end, span)?;
        emitter.place(hidden);
        emitter.emit(Instruction::PushSmall(0), span)?;
        emitter.place(end);
        let code = emitter
            .finish(span)
            .map_err(|error| Diagnostic::error(span, error.message))?;
        Ok(Some(PropertySource::Expression(self.add_expression(code))))
    }
}
