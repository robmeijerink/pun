//! Rector 2.9.2's own fixtures for the `php-5.5` rules, copied verbatim from rector-src (rules-tests/Php55/...) into
//! tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the
//! rule alone, `withPhpVersion(80400)`) wrote for them. Each rule also runs on every node of its fixtures
//! (`everywhere`) and must report what it reports on its target nodes.

use crate::common::run_class;
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::{
    NoClassMagicConstant, PreferSelfOverGetCalledClass, PreferStaticOverGetCalledClass,
};

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn class_constant_to_self_class() {
    check(
        || Box::new(NoClassMagicConstant),
        "ClassConstantToSelfClassRector",
    );
}

#[test]
fn get_called_class_to_self_class() {
    check(
        || Box::new(PreferSelfOverGetCalledClass),
        "GetCalledClassToSelfClassRector",
    );
}

#[test]
fn get_called_class_to_static_class() {
    check(
        || Box::new(PreferStaticOverGetCalledClass),
        "GetCalledClassToStaticClassRector",
    );
}
