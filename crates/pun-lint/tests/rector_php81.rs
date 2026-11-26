//! Rector 2.9.2's own fixtures for the `php-8.1` rules, copied verbatim from rector-src (rules-tests/Php81/... and
//! rules-tests/CodingStyle/...) into tests/rector/<Rector class>/Fixture. Each rule also runs on every node of its
//! fixtures (`everywhere`) and must report what it reports on its target nodes.
//!
//! ArrayToFirstClassCallableRector/Pun/config/sentry.php.inc: Rector also rewrites the array callables a `config/`
//! file returns, which Laravel's `config:cache` then cannot export; Pun leaves those and converts the rest.

use crate::common::{run_class, run_dirs};
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::{
    NoClosureFromCallable, NoNullPregSplitLimit, NoNullStringArgument, NoReflectionSetAccessible,
    PreferEnumCaseOverMyCLabsCall, PreferEnumCaseOverSpatieCall, PreferEnumFrom,
    PreferFirstClassCallableArgument, PreferFirstClassCallableOverArray,
    PreferFirstClassCallableOverArrowFunction, PreferFirstClassCallableOverClosure,
    PreferNativeEnumOverMyCLabs, PreferNativeEnumOverSpatie, PreferReadonlyProperty,
};

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn function_first_class_callable() {
    check(
        || Box::new(PreferFirstClassCallableArgument),
        "FunctionFirstClassCallableRector",
    );
}

#[test]
fn closure_from_callable_to_first_class_callable() {
    check(
        || Box::new(NoClosureFromCallable),
        "ClosureFromCallableToFirstClassCallableRector",
    );
}

#[test]
fn remove_reflection_set_accessible_calls() {
    check(
        || Box::new(NoReflectionSetAccessible),
        "RemoveReflectionSetAccessibleCallsRector",
    );
}

#[test]
fn null_to_strict_int_preg_split_limit_arg() {
    check(
        || Box::new(NoNullPregSplitLimit),
        "NullToStrictIntPregSlitFuncCallLimitArgRector",
    );
}

#[test]
fn null_to_strict_string_func_call_arg() {
    check(
        || Box::new(NoNullStringArgument),
        "NullToStrictStringFuncCallArgRector",
    );
}

#[test]
fn array_to_first_class_callable() {
    check(
        || Box::new(PreferFirstClassCallableOverArray),
        "ArrayToFirstClassCallableRector",
    );
}

#[test]
fn arrow_function_delegating_call_to_first_class_callable() {
    check(
        || Box::new(PreferFirstClassCallableOverArrowFunction),
        "ArrowFunctionDelegatingCallToFirstClassCallableRector",
    );
}

#[test]
fn closure_delegating_call_to_first_class_callable() {
    check(
        || Box::new(PreferFirstClassCallableOverClosure),
        "ClosureDelegatingCallToFirstClassCallableRector",
    );
}

#[test]
fn my_c_labs_constructor_call_to_enum_from() {
    check(
        || Box::new(PreferEnumFrom),
        "MyCLabsConstructorCallToEnumFromRector",
    );
}

#[test]
fn my_c_labs_class_to_enum() {
    check(
        || Box::new(PreferNativeEnumOverMyCLabs),
        "MyCLabsClassToEnumRector",
    );
}

#[test]
fn spatie_enum_class_to_enum() {
    check(
        || Box::new(PreferNativeEnumOverSpatie::default()),
        "SpatieEnumClassToEnumRector",
    );
    let mut snake = PreferNativeEnumOverSpatie::default();
    snake
        .configure(&serde_json::json!({"to-upper-snake-case": true}))
        .unwrap();
    run_dirs(
        vec![Box::new(snake)],
        "SpatieEnumClassToEnumRector",
        &["FixtureUpperCaseSnakeCase"],
        80400,
        &[],
    );
    assert_eq!(
        PreferNativeEnumOverSpatie::default()
            .configure(&serde_json::json!({"toUpperSnakeCase": true}))
            .unwrap_err()
            .to_string(),
        "unknown option `toUpperSnakeCase` or a value of the wrong type"
    );
}

#[test]
fn spatie_enum_method_call_to_enum_const() {
    check(
        || Box::new(PreferEnumCaseOverSpatieCall),
        "SpatieEnumMethodCallToEnumConstRector",
    );
}

#[test]
fn read_only_property() {
    check(
        || Box::new(PreferReadonlyProperty),
        "ReadOnlyPropertyRector",
    );
}

#[test]
fn my_c_labs_method_call_to_enum_const() {
    check(
        || Box::new(PreferEnumCaseOverMyCLabsCall),
        "MyCLabsMethodCallToEnumConstRector",
    );
}
