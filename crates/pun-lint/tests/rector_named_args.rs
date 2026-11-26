//! Rector 2.9.2's own fixtures for its named-args rules, copied verbatim from rector-src's rules-tests at 2.9.2
//! (CodeQuality/Rector/CallLike/..., CodeQuality/Rector/Attribute/...) into tests/rector/<Rector class>/Fixture (and
//! its Source/). Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the rule alone,
//! `phpVersion(80400)`) wrote for them. A fixture is `input -----\n expected`, or only the input when the rule must not
//! change it.

use crate::common::run_class;
use pun_lint::rules::{AddAttributeArgumentNames, AddBooleanArgumentName, AddNullArgumentName};

#[test]
fn add_name_to_boolean_argument() {
    run_class(
        vec![Box::new(AddBooleanArgumentName)],
        "AddNameToBooleanArgumentRector",
    );
}

#[test]
fn add_name_to_null_argument() {
    run_class(
        vec![Box::new(AddNullArgumentName)],
        "AddNameToNullArgumentRector",
    );
}

#[test]
fn explicit_attribute_named_args() {
    run_class(
        vec![Box::new(AddAttributeArgumentNames)],
        "ExplicitAttributeNamedArgsRector",
    );
}
