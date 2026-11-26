//! Rector 2.9.2's own fixtures for the core rules it added, copied verbatim from rector-src (rules-tests/...) into
//! tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the
//! rule alone, the PHP version of the rule's config) wrote for them; Pun/ holds the cases where Pun keeps the code
//! valid where Rector does not, each with its reason in the file.

use crate::common::{run_class, run_class_at};
use pun_lint::rules::{
    AddBoolReturnFromMixed, NoPhpstanPhpdocTag, PreferLiteralBoolPhpdoc,
    PreferReadonlyPropertyDefault,
};

#[test]
fn bool_return_type_from_boolean_const_and_strict_returns() {
    run_class(
        vec![Box::new(AddBoolReturnFromMixed)],
        "BoolReturnTypeFromBooleanConstAndStrictReturnsRector",
    );
}

#[test]
fn constructor_readonly_assign_to_default() {
    run_class_at(
        vec![Box::new(PreferReadonlyPropertyDefault)],
        "ConstructorReadonlyAssignToDefaultRector",
        80600,
    );
}

#[test]
fn merge_phpstan_doc_tag_into_native() {
    run_class(
        vec![Box::new(NoPhpstanPhpdocTag)],
        "MergePhpstanDocTagIntoNativeRector",
    );
}

#[test]
fn narrow_bool_docblock_return_type() {
    run_class(
        vec![Box::new(PreferLiteralBoolPhpdoc)],
        "NarrowBoolDocblockReturnTypeRector",
    );
}
