//! Rector 2.9.2's own fixtures for the `php-7.3` rules, copied verbatim from rector-src (rules-tests/Php73/...) into
//! tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the
//! rule alone, `withPhpVersion(80400)`) wrote for them. Each rule also runs on every node of its fixtures
//! (`everywhere`) and must report what it reports on its target nodes.

use crate::common::run_class;
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::{
    NoCaseInsensitiveConstant, NoCaseInsensitiveDefine, NoHeredocLabelInBody, NoUnescapedRegexDash,
    PreferArrayKeyFirst, PreferArrayKeyFirstLast, PreferIsCountable, PreferSetcookieOptions,
    RequireStringNeedle,
};

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn is_countable() {
    check(|| Box::new(PreferIsCountable), "IsCountableRector");
}

#[test]
fn regex_dash_escape() {
    check(|| Box::new(NoUnescapedRegexDash), "RegexDashEscapeRector");
}

#[test]
fn sensitive_define() {
    check(
        || Box::new(NoCaseInsensitiveDefine),
        "SensitiveDefineRector",
    );
}

#[test]
fn array_keys_to_array_key_first_last() {
    check(
        || Box::new(PreferArrayKeyFirstLast),
        "ArrayKeysToArrayKeyFirstLastRector",
    );
}

#[test]
fn array_key_first_last() {
    check(|| Box::new(PreferArrayKeyFirst), "ArrayKeyFirstLastRector");
}

#[test]
fn sensitive_constant_name() {
    check(
        || Box::new(NoCaseInsensitiveConstant),
        "SensitiveConstantNameRector",
    );
}

#[test]
fn sensitive_here_now_doc() {
    check(
        || Box::new(NoHeredocLabelInBody),
        "SensitiveHereNowDocRector",
    );
}

#[test]
fn setcookie() {
    check(|| Box::new(PreferSetcookieOptions), "SetcookieRector");
}

#[test]
fn stringify_str_needles() {
    check(
        || Box::new(RequireStringNeedle),
        "StringifyStrNeedlesRector",
    );
}
