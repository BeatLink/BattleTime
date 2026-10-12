//! Tests for values known when compiling: the constant type itself, the operators and library
//! functions, and the evaluator run over small modules.

use std::collections::HashMap;
use std::rc::Rc;

use oxc_allocator::Allocator;
use oxc_ast::ast::{BindingPattern, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::operator::{AssignmentOperator, BinaryOperator};

use super::{compound_operator, divide_constant, library_constant, whole_number, Constant, Progression};
use crate::compiler::Compiler;
use crate::scope::Scope;
use crate::types::icon_number;

// ============================================================================
// Helpers
// ============================================================================

const IMPORTS: &str = r#"
import { signal } from "@preact/signals";
import { clamp, format_number, range, to_upper, text_length, slice_text, pad_start, join, HOUR, DAY, SCREEN_SIZE } from "neotime";
import { icon_named } from "neotime/icons";
"#;

/// What the evaluator makes of `expression` in a module that also holds `declarations`.
fn evaluate_with(declarations: &str, expression: &str) -> Option<Constant> {
    let source = format!("{IMPORTS}\n{declarations}\nconst evaluated_result = {expression};\n");
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, SourceType::tsx()).parse();
    assert_eq!(
        parsed.diagnostics.errors().count(),
        0,
        "the source does not parse"
    );
    let program = &parsed.program;
    let mut compiler = Compiler::new(HashMap::new());
    let module = compiler.load_module(program).expect("the module loads");
    let root = Scope::module_root(module);
    let Some(Statement::VariableDeclaration(declaration)) = program.body.last() else {
        panic!("the result is not the last declaration");
    };
    let declarator = &declaration.declarations[0];
    assert!(
        matches!(&declarator.id, BindingPattern::BindingIdentifier(name) if name.name == "evaluated_result")
    );
    let init = declarator.init.as_ref().expect("an initial value");

    compiler.evaluate_constant(init, &root)
}

/// What the evaluator makes of `expression` on its own.
fn evaluate(expression: &str) -> Option<Constant> {
    evaluate_with("", expression)
}

fn number(value: i64) -> Option<Constant> {
    Some(Constant::Number(value))
}

fn text(value: &str) -> Option<Constant> {
    Some(Constant::Text(value.into()))
}

fn boolean(value: bool) -> Option<Constant> {
    Some(Constant::Bool(value))
}

fn numbers(values: &[i64]) -> Option<Constant> {
    Some(Constant::Array(Rc::new(
        values.iter().copied().map(Constant::Number).collect(),
    )))
}

fn object(fields: &[(&str, Constant)]) -> Constant {
    Constant::Object(Rc::new(
        fields
            .iter()
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect(),
    ))
}

// ============================================================================
// The constant type
// ============================================================================

#[test]
fn truthiness_follows_javascript() {
    assert!(!Constant::Number(0).is_truthy());
    assert!(Constant::Number(-1).is_truthy());
    assert!(!Constant::Bool(false).is_truthy());
    assert!(!Constant::Text("".into()).is_truthy());
    assert!(Constant::Text("0".into()).is_truthy());
    assert!(!Constant::Null.is_truthy());
    assert!(!Constant::Undefined.is_truthy());
    assert!(Constant::Icon(0).is_truthy());
    assert!(Constant::Array(Rc::new(Vec::new())).is_truthy());
    assert!(object(&[]).is_truthy());
}

#[test]
fn a_missing_object_field_is_undefined() {
    let point = object(&[("x", Constant::Number(3))]);

    assert_eq!(point.field("x"), Some(Constant::Number(3)));
    assert_eq!(point.field("y"), Some(Constant::Undefined));
}

#[test]
fn length_counts_array_elements_and_text_characters() {
    let array = Constant::Array(Rc::new(vec![Constant::Null, Constant::Null]));
    let accented = Constant::Text("héllo".into());

    assert_eq!(array.field("length"), Some(Constant::Number(2)));
    assert_eq!(accented.field("length"), Some(Constant::Number(5)));
    assert_eq!(array.field("size"), None);
    assert_eq!(Constant::Number(5).field("length"), None);
}

#[test]
fn display_text_matches_a_template_literal() {
    assert_eq!(Constant::Number(-4).display_text().as_deref(), Some("-4"));
    assert_eq!(Constant::Bool(true).display_text().as_deref(), Some("true"));
    assert_eq!(Constant::Text("hi".into()).display_text().as_deref(), Some("hi"));
    assert_eq!(Constant::Null.display_text().as_deref(), Some("null"));
    assert_eq!(Constant::Undefined.display_text(), None);
    assert_eq!(Constant::Icon(1).display_text(), None);
}

#[test]
fn an_evenly_rising_array_is_a_progression() {
    let elements: Vec<Constant> = [3, 5, 7, 9].into_iter().map(Constant::Number).collect();

    assert_eq!(
        Constant::progression(&elements),
        Some(Progression { start: 3, step: 2 })
    );
}

#[test]
fn a_single_number_is_a_progression_of_step_one() {
    assert_eq!(
        Constant::progression(&[Constant::Number(8)]),
        Some(Progression { start: 8, step: 1 })
    );
}

#[test]
fn uneven_text_or_empty_arrays_are_no_progression() {
    let uneven: Vec<Constant> = [1, 2, 4].into_iter().map(Constant::Number).collect();

    assert_eq!(Constant::progression(&uneven), None);
    assert_eq!(Constant::progression(&[Constant::Text("a".into())]), None);
    assert_eq!(Constant::progression(&[]), None);
}

#[test]
fn an_array_of_one_repeated_value_is_uniform() {
    let same = vec![Constant::Number(2); 3];
    let mixed = vec![Constant::Number(2), Constant::Number(3)];

    assert!(Constant::is_uniform_array(&same));
    assert!(!Constant::is_uniform_array(&mixed));
    assert!(Constant::is_uniform_array(&[]));
}

#[test]
fn the_library_constants_are_in_milliseconds_and_pixels() {
    assert_eq!(library_constant("SECOND"), number(1000));
    assert_eq!(library_constant("HOUR"), number(3_600_000));
    assert_eq!(library_constant("DAY"), number(86_400_000));
    assert_eq!(library_constant("SCREEN_SIZE"), number(240));
    assert_eq!(library_constant("WEEK"), None);
}

#[test]
fn only_whole_numbers_that_fit_are_whole() {
    assert_eq!(whole_number(12.0), Some(12));
    assert_eq!(whole_number(-3.0), Some(-3));
    assert_eq!(whole_number(2.5), None);
    assert_eq!(whole_number(1.0e19), None);
}

#[test]
fn compound_assignments_name_their_operator() {
    assert_eq!(
        compound_operator(AssignmentOperator::Addition),
        Some(BinaryOperator::Addition)
    );
    assert_eq!(
        compound_operator(AssignmentOperator::Remainder),
        Some(BinaryOperator::Remainder)
    );
    assert_eq!(
        compound_operator(AssignmentOperator::ShiftLeft),
        Some(BinaryOperator::ShiftLeft)
    );
    assert_eq!(
        compound_operator(AssignmentOperator::BitwiseXOR),
        Some(BinaryOperator::BitwiseXOR)
    );
    assert_eq!(
        compound_operator(AssignmentOperator::Division),
        Some(BinaryOperator::Division)
    );
    assert_eq!(compound_operator(AssignmentOperator::Exponential), None);
    assert_eq!(compound_operator(AssignmentOperator::LogicalAnd), None);
}

// ============================================================================
// Division
// ============================================================================

#[test]
fn rounded_division_rounds_each_way_like_math() {
    assert_eq!(divide_constant("Math.trunc", -7, 2), Some(-3));
    assert_eq!(divide_constant("Math.floor", -7, 2), Some(-4));
    assert_eq!(divide_constant("Math.floor", 7, 2), Some(3));
    assert_eq!(divide_constant("Math.ceil", 7, 2), Some(4));
    assert_eq!(divide_constant("Math.ceil", -7, 2), Some(-3));
    assert_eq!(divide_constant("Math.round", 5, 2), Some(3));
    assert_eq!(divide_constant("Math.round", -5, 2), Some(-2));
    assert_eq!(divide_constant("Math.round", 4, 3), Some(1));
}

#[test]
fn division_by_zero_or_by_an_unknown_rounding_is_not_known() {
    assert_eq!(divide_constant("Math.floor", 1, 0), None);
    assert_eq!(divide_constant("Math.abs", 4, 2), None);
    assert_eq!(divide_constant("Math.trunc", i64::MIN, -1), None);
}

// ============================================================================
// Operators
// ============================================================================

#[test]
fn adding_text_joins_the_display_text() {
    assert_eq!(evaluate(r#""a" + 1"#), text("a1"));
    assert_eq!(evaluate(r#"2 + "b""#), text("2b"));
    assert_eq!(evaluate(r#""x" + null"#), text("xnull"));
    assert_eq!(evaluate(r#""x" + undefined"#), None);
}

#[test]
fn null_and_undefined_are_loosely_equal() {
    assert_eq!(evaluate("null == undefined"), boolean(true));
    assert_eq!(evaluate("null === undefined"), boolean(true));
    assert_eq!(evaluate("1 !== 2"), boolean(true));
    assert_eq!(evaluate(r#""a" != "a""#), boolean(false));
}

#[test]
fn comparisons_give_booleans() {
    assert_eq!(evaluate("2 < 3"), boolean(true));
    assert_eq!(evaluate("3 <= 3"), boolean(true));
    assert_eq!(evaluate("2 > 3"), boolean(false));
    assert_eq!(evaluate("2 >= 3"), boolean(false));
    assert_eq!(evaluate(r#""a" < "b""#), None);
}

#[test]
fn booleans_count_as_numbers_in_arithmetic() {
    assert_eq!(evaluate("true + 1"), number(2));
    assert_eq!(evaluate("false * 5"), number(0));
}

#[test]
fn arithmetic_works_on_whole_numbers() {
    assert_eq!(evaluate("10 - 4"), number(6));
    assert_eq!(evaluate("7 % 3"), number(1));
    assert_eq!(evaluate("6 / 3"), number(2));
}

#[test]
fn division_with_a_remainder_or_by_zero_is_not_known() {
    assert_eq!(evaluate("7 / 2"), None);
    assert_eq!(evaluate("1 / 0"), None);
}

#[test]
fn arithmetic_that_overflows_is_not_known() {
    assert_eq!(evaluate("4000000000000000000 * 4"), None);
}

#[test]
fn bitwise_operators_and_shifts_work() {
    assert_eq!(evaluate("12 & 10"), number(8));
    assert_eq!(evaluate("12 | 3"), number(15));
    assert_eq!(evaluate("6 ^ 3"), number(5));
    assert_eq!(evaluate("1 << 4"), number(16));
    assert_eq!(evaluate("32 >> 2"), number(8));
    assert_eq!(evaluate("1 << 64"), None);
}

#[test]
fn unary_operators_work_on_numbers() {
    assert_eq!(evaluate("!0"), boolean(true));
    assert_eq!(evaluate("-5"), number(-5));
    assert_eq!(evaluate("+7"), number(7));
    assert_eq!(evaluate("~5"), number(-6));
    assert_eq!(evaluate(r#"+"x""#), None);
    assert_eq!(evaluate("typeof 1"), None);
}

#[test]
fn logical_operators_pick_a_side_like_javascript() {
    assert_eq!(evaluate("0 && 5"), number(0));
    assert_eq!(evaluate("1 && 5"), number(5));
    assert_eq!(evaluate("0 || 5"), number(5));
    assert_eq!(evaluate("null ?? 3"), number(3));
    assert_eq!(evaluate("0 ?? 3"), number(0));
}

#[test]
fn a_conditional_takes_the_branch_its_test_chooses() {
    assert_eq!(evaluate(r#"1 > 2 ? "yes" : "no""#), text("no"));
}

#[test]
fn a_fraction_is_not_a_constant() {
    assert_eq!(evaluate("1.5"), None);
}

// ============================================================================
// Literals
// ============================================================================

#[test]
fn a_template_literal_joins_its_parts() {
    assert_eq!(evaluate("`${1}-${true}-${null}`"), text("1-true-null"));
    assert_eq!(evaluate("`${[1]}`"), None);
}

#[test]
fn array_spread_flattens_constant_arrays() {
    assert_eq!(
        evaluate_with("const start = [1, 2];", "[...start, 3]"),
        numbers(&[1, 2, 3])
    );
    assert_eq!(evaluate("[...5]"), None);
    assert_eq!(evaluate("[1, , 2]"), None);
}

#[test]
fn object_spread_and_repeated_keys_keep_the_last_value() {
    let result = evaluate_with(
        "const base = { a: 1, b: 2 };",
        r#"{ ...base, b: 3, "c": 4, 5: 6, a: 7 }"#,
    );

    assert_eq!(
        result,
        Some(object(&[
            ("b", Constant::Number(3)),
            ("c", Constant::Number(4)),
            ("5", Constant::Number(6)),
            ("a", Constant::Number(7)),
        ]))
    );
}

#[test]
fn spreading_something_other_than_an_object_or_a_computed_key_is_not_known() {
    assert_eq!(evaluate("{ ...3 }"), None);
    assert_eq!(evaluate_with(r#"const key = "k";"#, "{ [key]: 1 }"), None);
}

#[test]
fn type_assertions_are_seen_through() {
    assert_eq!(evaluate("(5 as number)"), number(5));
    assert_eq!(evaluate("(6 satisfies number)"), number(6));
    assert_eq!(
        evaluate_with("const value: number | null = 4;", "value!"),
        number(4)
    );
}

// ============================================================================
// Members
// ============================================================================

#[test]
fn members_read_object_fields_and_array_elements() {
    let declarations = r#"const table = { name: "x", list: [10, 20] };"#;

    assert_eq!(evaluate_with(declarations, "table.name"), text("x"));
    assert_eq!(evaluate_with(declarations, "table.list[1]"), number(20));
    assert_eq!(evaluate_with(declarations, r#"table["name"]"#), text("x"));
    assert_eq!(evaluate_with(declarations, "table.list.length"), number(2));
}

#[test]
fn an_index_outside_the_array_is_undefined() {
    let declarations = "const list = [10, 20];";

    assert_eq!(evaluate_with(declarations, "list[5]"), Some(Constant::Undefined));
    assert_eq!(evaluate_with(declarations, "list[-1]"), Some(Constant::Undefined));
    assert_eq!(evaluate_with(declarations, r#"list["0"]"#), None);
}

#[test]
fn optional_chaining_on_nothing_is_undefined() {
    let declarations = "const nothing = null; const point = { x: 2 };";

    assert_eq!(
        evaluate_with(declarations, "nothing?.x"),
        Some(Constant::Undefined)
    );
    assert_eq!(evaluate_with(declarations, "point?.x"), number(2));
}

#[test]
fn reading_a_signal_is_not_known_when_compiling() {
    assert_eq!(evaluate_with("const count = signal(3);", "count.value + 1"), None);
}

// ============================================================================
// Library calls
// ============================================================================

#[test]
fn array_from_maps_each_index() {
    assert_eq!(
        evaluate("Array.from({ length: 4 }, (_, index) => index * index)"),
        numbers(&[0, 1, 4, 9])
    );
    assert_eq!(
        evaluate("Array.from({ length: 5000 }, (_, index) => index)"),
        None
    );
}

#[test]
fn text_methods_become_library_calls() {
    assert_eq!(evaluate(r#""abc".toUpperCase()"#), text("ABC"));
    assert_eq!(evaluate(r#""hello".slice(1, 3)"#), text("el"));
    assert_eq!(evaluate(r#""7".padStart(3, "0")"#), text("007"));
    assert_eq!(evaluate(r#"" x ".trim()"#), None);
}

#[test]
fn array_map_passes_each_element_and_index() {
    assert_eq!(
        evaluate("[10, 20, 30].map((value, index) => value + index)"),
        numbers(&[10, 21, 32])
    );
}

#[test]
fn array_map_can_call_a_named_function() {
    assert_eq!(
        evaluate_with(
            "function double(value: number) { return value * 2; }",
            "[1, 2].map(double)"
        ),
        numbers(&[2, 4])
    );
}

#[test]
fn includes_and_index_of_search_the_array() {
    assert_eq!(evaluate("[1, 2, 3].includes(2)"), boolean(true));
    assert_eq!(evaluate("[1, 2, 3].includes(9)"), boolean(false));
    assert_eq!(evaluate(r#"["a", "b"].indexOf("b")"#), number(1));
    assert_eq!(evaluate(r#"["a", "b"].indexOf("z")"#), number(-1));
    assert_eq!(evaluate("[1, 2].filter((value) => value > 1)"), None);
    assert_eq!(evaluate("[1, 2].includes()"), None);
}

#[test]
fn methods_on_numbers_are_not_known() {
    assert_eq!(evaluate("(5).toString()"), None);
}

#[test]
fn math_around_a_division_rounds_the_exact_quotient() {
    assert_eq!(evaluate("Math.floor(7 / 2)"), number(3));
    assert_eq!(evaluate("Math.floor(-7 / 2)"), number(-4));
    assert_eq!(evaluate("Math.ceil(7 / 2)"), number(4));
    assert_eq!(evaluate("Math.round(5 / 2)"), number(3));
    assert_eq!(evaluate("Math.trunc(-7 / 2)"), number(-3));
    assert_eq!(evaluate("Math.floor(7 / 0)"), None);
}

#[test]
fn math_functions_of_whole_numbers_are_known() {
    assert_eq!(evaluate("Math.floor(4)"), number(4));
    assert_eq!(evaluate("Math.abs(-3)"), number(3));
    assert_eq!(evaluate("Math.min(3, 1, 2)"), number(1));
    assert_eq!(evaluate("Math.max(3, 1, 2)"), number(3));
    assert_eq!(evaluate(r#"Math.max(3, "a")"#), None);
}

#[test]
fn neotime_number_functions_are_known() {
    assert_eq!(evaluate("clamp(15, 0, 10)"), number(10));
    assert_eq!(evaluate("clamp(-5, 0, 10)"), number(0));
    assert_eq!(evaluate("format_number(-5, 3)"), text("-005"));
    assert_eq!(evaluate("format_number(42)"), text("42"));
    assert_eq!(evaluate("range(3)"), numbers(&[0, 1, 2]));
    assert_eq!(evaluate("range(5000)"), None);
}

#[test]
fn neotime_text_functions_are_known() {
    assert_eq!(evaluate(r#"to_upper("abc")"#), text("ABC"));
    assert_eq!(evaluate(r#"text_length("héllo")"#), number(5));
    assert_eq!(evaluate(r#"slice_text("hello", -3)"#), text("llo"));
    assert_eq!(evaluate(r#"slice_text("hello", 3, 1)"#), text(""));
    assert_eq!(evaluate(r#"pad_start("5", 3)"#), text("  5"));
    assert_eq!(evaluate(r#"pad_start("12345", 3, "0")"#), text("12345"));
}

#[test]
fn join_shows_every_element_between_separators() {
    assert_eq!(evaluate(r#"join([1, "a", true], "-")"#), text("1-a-true"));
    assert_eq!(evaluate(r#"join("abc", "-")"#), None);
    assert_eq!(evaluate(r#"join([[1]], "-")"#), None);
}

#[test]
fn icon_named_finds_a_library_icon() {
    assert_eq!(
        evaluate(r#"icon_named("timer")"#),
        icon_number("timer").map(Constant::Icon)
    );
    assert_eq!(evaluate(r#"icon_named("unicorn")"#), None);
    assert_eq!(evaluate("icon_named(3)"), None);
}

#[test]
fn the_library_constants_can_be_imported() {
    assert_eq!(
        evaluate("HOUR * 2 + DAY - SCREEN_SIZE"),
        number(7_200_000 + 86_400_000 - 240)
    );
}

// ============================================================================
// Functions
// ============================================================================

#[test]
fn a_missing_argument_takes_its_default() {
    let declarations = "function add(left: number, right = 10) { return left + right; }";

    assert_eq!(evaluate_with(declarations, "add(1)"), number(11));
    assert_eq!(evaluate_with(declarations, "add(1, 2)"), number(3));
}

#[test]
fn an_object_parameter_is_destructured_with_defaults() {
    let declarations =
        "const area = ({ width, height = 2 }: { width: number; height?: number }) => width * height;";

    assert_eq!(evaluate_with(declarations, "area({ width: 3 })"), number(6));
    assert_eq!(
        evaluate_with(declarations, "area({ width: 3, height: 4 })"),
        number(12)
    );
}

#[test]
fn rest_and_array_patterns_are_not_known() {
    assert_eq!(
        evaluate_with(
            "const first = ({ a, ...rest }: any) => a;",
            "first({ a: 1, b: 2 })"
        ),
        None
    );
    assert_eq!(
        evaluate_with("const head = ([a]: number[]) => a;", "head([1])"),
        None
    );
}

#[test]
fn a_function_without_a_return_gives_undefined() {
    assert_eq!(
        evaluate_with("function nothing() { ; }", "nothing()"),
        Some(Constant::Undefined)
    );
    assert_eq!(
        evaluate_with("function bare() { return; }", "bare()"),
        Some(Constant::Undefined)
    );
}

#[test]
fn local_variables_follow_updates_and_compound_assignments() {
    let declarations = r#"
function work() {
  let total = 1;
  let unset;
  total += 4;
  total *= 3;
  total -= 1;
  total++;
  total--;
  (total <<= 1);
  unset = total;
  return unset;
}"#;

    assert_eq!(evaluate_with(declarations, "work()"), number(28));
}

#[test]
fn if_and_else_choose_a_branch() {
    let declarations = r#"
function sign(value: number) {
  if (value > 0) { return 1; } else if (value < 0) { return -1; }
  if (value === 0) return 0;
}"#;

    assert_eq!(evaluate_with(declarations, "sign(5)"), number(1));
    assert_eq!(evaluate_with(declarations, "sign(-5)"), number(-1));
    assert_eq!(evaluate_with(declarations, "sign(0)"), number(0));
}

#[test]
fn for_of_loops_add_up_and_may_return_early() {
    let declarations = r#"
function total(list: number[]) {
  let sum = 0;
  for (const value of list) { sum += value; }
  return sum;
}
function first_over(list: number[], limit: number) {
  for (const value of list) { if (value > limit) { return value; } }
  return -1;
}"#;

    assert_eq!(evaluate_with(declarations, "total([1, 2, 3])"), number(6));
    assert_eq!(evaluate_with(declarations, "first_over([1, 5, 9], 4)"), number(5));
    assert_eq!(evaluate_with(declarations, "first_over([1, 2], 4)"), number(-1));
}

#[test]
fn a_for_of_over_something_other_than_an_array_is_not_known() {
    let declarations = "function walk() { for (const letter of \"ab\") { } return 1; }";

    assert_eq!(evaluate_with(declarations, "walk()"), None);
}

#[test]
fn statements_the_evaluator_does_not_run_make_it_give_up() {
    let declarations = r#"
function loop_forever() { let count = 0; while (count < 3) { count++; } return count; }
function set_field() { const point = { x: 1 }; point.x = 2; return point.x; }
function logical_assign() { let flag = 1; flag &&= 0; return flag; }
function update_text() { let word = "a"; word++; return word; }
function call_a_number() { const value = 2; return value(); }
function assign_unknown() { missing = 2; return 1; }
"#;

    assert_eq!(evaluate_with(declarations, "loop_forever()"), None);
    assert_eq!(evaluate_with(declarations, "set_field()"), None);
    assert_eq!(evaluate_with(declarations, "logical_assign()"), None);
    assert_eq!(evaluate_with(declarations, "update_text()"), None);
    assert_eq!(evaluate_with(declarations, "call_a_number()"), None);
    assert_eq!(evaluate_with(declarations, "assign_unknown()"), None);
}

#[test]
fn recursion_gives_up_as_it_does_on_the_watch() {
    let declarations =
        "function countdown(value: number): number { return value <= 0 ? 0 : countdown(value - 1); }";

    assert_eq!(evaluate_with(declarations, "countdown(3)"), None);
}

#[test]
fn an_async_function_is_not_run() {
    assert_eq!(
        evaluate_with("async function later() { return 1; }", "later()"),
        None
    );
}

#[test]
fn calls_nested_too_deeply_give_up() {
    let mut declarations = String::from("function level_40() { return 40; }\n");
    for level in 0..40 {
        declarations.push_str(&format!(
            "function level_{level}() {{ return level_{}(); }}\n",
            level + 1
        ));
    }

    assert_eq!(evaluate_with(&declarations, "level_20()"), number(40));
    assert_eq!(evaluate_with(&declarations, "level_0()"), None);
}

#[test]
fn an_evaluation_that_runs_too_long_gives_up() {
    let declarations = r#"
function busy() {
  let total = 0;
  for (const outer of range(4096)) { for (const inner of range(4096)) { total += 1; } }
  return total;
}"#;

    assert_eq!(evaluate_with(declarations, "busy()"), None);
}
