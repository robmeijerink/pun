//! Rector 2.9.2's own fixtures for the `php-7.0` rules, copied verbatim from rector-src (rules-tests/Php70/...) into
//! tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the
//! rule alone, `withPhpVersion(80400)`) wrote for them. Each rule also runs on every node of its fixtures
//! (`everywhere`) and must report what it reports on its target nodes.

use crate::common::run_class;
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::{
    NoCallUserMethod, NoListFromString, PreferBracedVariableVariable, PreferCoalesceReturn,
    PreferRandomInt, PreferSpaceship, PreferSpaceshipReturn, PreferStaticMethodCall,
    PreferThrowableHandler, PreferTime,
};

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn call_user_method() {
    check(|| Box::new(NoCallUserMethod), "CallUserMethodRector");
}

#[test]
fn random_function() {
    check(|| Box::new(PreferRandomInt), "RandomFunctionRector");
}

#[test]
fn rename_mktime_without_args_to_time() {
    check(
        || Box::new(PreferTime),
        "RenameMktimeWithoutArgsToTimeRector",
    );
}

#[test]
fn exception_handler_typehint() {
    check(
        || Box::new(PreferThrowableHandler),
        "ExceptionHandlerTypehintRector",
    );
}

#[test]
fn if_isset_to_coalescing() {
    check(
        || Box::new(PreferCoalesceReturn),
        "IfIssetToCoalescingRector",
    );
}

#[test]
fn if_to_spaceship() {
    check(|| Box::new(PreferSpaceshipReturn), "IfToSpaceshipRector");
}

#[test]
fn list_split_string() {
    check(|| Box::new(NoListFromString), "ListSplitStringRector");
}

#[test]
fn ternary_to_spaceship() {
    check(|| Box::new(PreferSpaceship), "TernaryToSpaceshipRector");
}

#[test]
fn this_call_on_static_method_to_static_call() {
    check(
        || Box::new(PreferStaticMethodCall),
        "ThisCallOnStaticMethodToStaticCallRector",
    );
}

#[test]
fn wrap_variable_variable_name_in_curly_braces() {
    check(
        || Box::new(PreferBracedVariableVariable),
        "WrapVariableVariableNameInCurlyBracesRector",
    );
}
