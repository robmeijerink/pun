//! Rector 2.9.2's own fixtures for the `php-7.1` rules, copied verbatim from rector-src (rules-tests/Php71/...) into
//! tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the
//! rule alone, `withPhpVersion(80400)`) wrote for them. Each rule also runs on every node of its fixtures
//! (`everywhere`) and must report what it reports on its target nodes.

use crate::common::run_class;
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::{
    NoAppendToString, NoNonNumericStringOperand, PreferIsIterable, PreferMultiCatch,
};

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn is_iterable() {
    check(|| Box::new(PreferIsIterable), "IsIterableRector");
}

#[test]
fn assign_array_to_string() {
    check(|| Box::new(NoAppendToString), "AssignArrayToStringRector");
}

#[test]
fn binary_op_between_number_and_string() {
    check(
        || Box::new(NoNonNumericStringOperand),
        "BinaryOpBetweenNumberAndStringRector",
    );
}

#[test]
fn multi_exception_catch() {
    check(|| Box::new(PreferMultiCatch), "MultiExceptionCatchRector");
}
