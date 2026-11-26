//! rector-phpunit's own fixtures for its rules in no set (and `AddSeeTestAnnotationRector`, only in rector-preset),
//! copied verbatim from rectorphp/rector-phpunit at d0b6caa (rules-tests/...) into tests/rector/<Rector class>/Fixture
//! (and its Source/), run as `rector_phpunit.rs` runs its fixtures. Oracle/ holds extra inputs with what Rector 2.8.4
//! and that rector-phpunit (`.oracle/rector-ecosystem`, the rule alone, `phpVersion(80400)`) wrote for them.
//! The oracle ran with tests/phpunit-stubs loaded by a bootstrap autoloader; AddSeeTestAnnotationRector's oracle also
//! had Source/OracleTests.php (the test classes its Oracle/ input names) on `withAutoloadPaths`.
//!
//! AddKeysExistsAssertForKeyUseRector/Pun/non_string_key: Rector inserts another `assertArrayHasKey()` before
//! `assertSame(..., $data[0])` or `$data[$key]` on every run (only a string literal key becomes known); Pun fixes until
//! nothing changes, so it leaves a key that is not a string literal alone.

use crate::rector_phpunit::run_class;
use pun_lint::Rule;
use pun_lint::rules::{
    AddSeeTestTag, PreferCamelCaseTestNames, PreferDirectAssert, PreferSelfCall,
    PreferSnakeCaseTestNames, PreferThisAssert, RequireArrayKeyAssert, RequireExplicitMockExpects,
};

fn check(rule: impl Rule + 'static, class: &str) {
    run_class(vec![Box::new(rule)], class);
}

#[test]
fn prefer_phpunit_self_call() {
    check(PreferSelfCall, "PreferPHPUnitSelfCallRector");
}

#[test]
fn assert_class_to_this_assert() {
    check(PreferThisAssert, "AssertClassToThisAssertRector");
}

#[test]
fn assert_that_to_direct_assert() {
    check(PreferDirectAssert, "AssertThatToDirectAssertRector");
}

#[test]
fn prefer_tests_with_camel_case() {
    check(PreferCamelCaseTestNames, "PreferTestsWithCamelCaseRector");
}

#[test]
fn prefer_tests_with_snake_case() {
    check(PreferSnakeCaseTestNames, "PreferTestsWithSnakeCaseRector");
}

#[test]
fn explicit_mock_expects_call() {
    check(RequireExplicitMockExpects, "ExplicitMockExpectsCallRector");
}

#[test]
fn add_keys_exists_assert_for_key_use() {
    check(RequireArrayKeyAssert, "AddKeysExistsAssertForKeyUseRector");
}

#[test]
fn add_see_test_annotation() {
    check(AddSeeTestTag, "AddSeeTestAnnotationRector");
}
