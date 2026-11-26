//! Rector 2.9.2's own fixtures for the `php-7.4` rules, copied verbatim from rector-src (rules-tests/Php74/...) into
//! tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the
//! rule alone, `withPhpVersion(80400)`) wrote for them. Each rule also runs on every node of its fixtures
//! (`everywhere`) and must report what it reports on its target nodes.

use crate::common::run_class;
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::{
    AddNullPropertyDefault, NoArrayKeyExistsOnObject, NoFilterMagicQuotes, NoHebrevc,
    NoMbStrrposEncodingOffset, NoMoneyFormat, NoReflectionExport, NoRestoreIncludePath,
    PreferArrowFunction, PreferCoalesceAssignOverIf,
};

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn closure_to_arrow_function() {
    check(
        || Box::new(PreferArrowFunction),
        "ClosureToArrowFunctionRector",
    );
}

#[test]
fn filter_var_to_add_slashes() {
    check(
        || Box::new(NoFilterMagicQuotes),
        "FilterVarToAddSlashesRector",
    );
}

#[test]
fn hebrevc_to_nl2br_hebrev() {
    check(|| Box::new(NoHebrevc), "HebrevcToNl2brHebrevRector");
}

#[test]
fn restore_include_path_to_ini_restore() {
    check(
        || Box::new(NoRestoreIncludePath),
        "RestoreIncludePathToIniRestoreRector",
    );
}

#[test]
fn mb_strrpos_encoding_argument_position() {
    check(
        || Box::new(NoMbStrrposEncodingOffset),
        "MbStrrposEncodingArgumentPositionRector",
    );
}

#[test]
fn array_key_exists_on_property() {
    check(
        || Box::new(NoArrayKeyExistsOnObject),
        "ArrayKeyExistsOnPropertyRector",
    );
}

#[test]
fn export_to_reflection_function() {
    check(
        || Box::new(NoReflectionExport),
        "ExportToReflectionFunctionRector",
    );
}

#[test]
fn if_to_null_coalescing_assign() {
    check(
        || Box::new(PreferCoalesceAssignOverIf),
        "IfToNullCoalescingAssignRector",
    );
}

#[test]
fn money_format_to_number_format() {
    check(
        || Box::new(NoMoneyFormat),
        "MoneyFormatToNumberFormatRector",
    );
}

#[test]
fn restore_default_null_to_nullable_type_property() {
    check(
        || Box::new(AddNullPropertyDefault),
        "RestoreDefaultNullToNullableTypePropertyRector",
    );
}
