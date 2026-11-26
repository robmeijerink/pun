//! Rector 2.9.2's own fixtures for the `php-8.0` rules, copied verbatim from rector-src (rules-tests/Php80/...) into
//! tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the
//! rule alone, `withPhpVersion(80400)`) wrote for them. Each rule also runs on every node of its fixtures
//! (`everywhere`) and must report what it reports on its target nodes.

use crate::common::{run_class, run_dirs};
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::{
    NoIntGetHeadersFormat, NoUnusedCatchVariable, NoVersionCompareOperatorAlias,
    PreferEndsWithFunction, PreferGetDebugType, PreferMatch, PreferNullsafeOverTernary,
    PreferStartsWithFunction, PreferStaticClass, PreferStrContainsOverMbStrpos,
};

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn get_headers_argument_format() {
    check(
        || Box::new(NoIntGetHeadersFormat),
        "GetHeadersArgumentFormatRector",
    );
}

#[test]
fn remove_unused_variable_in_catch() {
    check(
        || Box::new(NoUnusedCatchVariable),
        "RemoveUnusedVariableInCatchRector",
    );
}

#[test]
fn version_compare_func_call_argument() {
    check(
        || Box::new(NoVersionCompareOperatorAlias),
        "VersionCompareFuncCallArgumentRector",
    );
}

#[test]
fn get_debug_type() {
    check(|| Box::new(PreferGetDebugType), "GetDebugTypeRector");
}

#[test]
fn class_on_this_variable_object() {
    check(
        || Box::new(PreferStaticClass),
        "ClassOnThisVariableObjectRector",
    );
}

#[test]
fn mb_str_contains() {
    check(
        || Box::new(PreferStrContainsOverMbStrpos),
        "MbStrContainsRector",
    );
}

#[test]
fn str_ends_with() {
    check(|| Box::new(PreferEndsWithFunction), "StrEndsWithRector");
}

#[test]
fn str_starts_with() {
    check(|| Box::new(PreferStartsWithFunction), "StrStartsWithRector");
}

#[test]
fn ternary_to_nullsafe_coalesce() {
    check(
        || Box::new(PreferNullsafeOverTernary),
        "TernaryToNullsafeCoalesceRector",
    );
}

#[test]
fn change_switch_to_match() {
    check(|| Box::new(PreferMatch), "ChangeSwitchToMatchRector");
    // config/configured_rule_php_version_74.php
    run_dirs(
        vec![Box::new(PreferMatch)],
        "ChangeSwitchToMatchRector",
        &["FixtureSkipped"],
        70400,
        &[],
    );
}
