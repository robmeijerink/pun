//! Rector 2.9.2's own fixtures for the `php-7.2` rules, copied verbatim from rector-src (rules-tests/Php72/...) into
//! tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the
//! rule alone, `withPhpVersion(80400)`) wrote for them. Each rule also runs on every node of its fixtures
//! (`everywhere`) and must report what it reports on its target nodes.

use crate::common::run_class;
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::{
    NoConstantDefineName, NoEach, NoGetClassNull, NoListEach, NoStringAssert,
    PreferForeachOverEach, RequireParseStrResult,
};

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn stringify_define() {
    check(|| Box::new(NoConstantDefineName), "StringifyDefineRector");
}

#[test]
fn get_class_on_null() {
    check(|| Box::new(NoGetClassNull), "GetClassOnNullRector");
}

#[test]
fn list_each() {
    check(|| Box::new(NoListEach), "ListEachRector");
}

#[test]
fn parse_str_with_result_argument() {
    check(
        || Box::new(RequireParseStrResult),
        "ParseStrWithResultArgumentRector",
    );
}

#[test]
fn replace_each_assignment_with_key_current() {
    check(
        || Box::new(NoEach),
        "ReplaceEachAssignmentWithKeyCurrentRector",
    );
}

#[test]
fn strings_assert_naked() {
    check(|| Box::new(NoStringAssert), "StringsAssertNakedRector");
}

#[test]
fn while_each_to_foreach() {
    check(
        || Box::new(PreferForeachOverEach),
        "WhileEachToForeachRector",
    );
}
