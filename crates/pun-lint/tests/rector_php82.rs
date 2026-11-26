//! Rector 2.9.2's own fixtures for the `php-8.2` rules, copied verbatim from rector-src (rules-tests/Php82/...) into
//! tests/rector/<Rector class>/Fixture with their Source/ classes. Oracle/ holds extra inputs with what Rector 2.8.4
//! (`.oracle/rector-bin`, the rule alone, `withPhpVersion(80400)`) wrote for them; Pun/ holds the cases where Pun keeps
//! the meaning where Rector does not, each with its reason in the file. Each rule also runs on every node of its
//! fixtures (`everywhere`) and must report what it reports on its target nodes.

use crate::common::run_class;
use crate::rector_refactor::everywhere;
use pun_lint::rules::{
    AddSkipDotsFlag, NoDollarBraceInterpolation, NoUtf8EncodeDecode, PreferReadonlyClass,
};

#[test]
fn readonly_class() {
    run_class(vec![Box::new(PreferReadonlyClass)], "ReadOnlyClassRector");
    everywhere(
        || Box::new(PreferReadonlyClass),
        "ReadOnlyClassRector",
        80400,
    );
}

#[test]
fn utf8_decode_encode() {
    run_class(
        vec![Box::new(NoUtf8EncodeDecode)],
        "Utf8DecodeEncodeToMbConvertEncodingRector",
    );
    everywhere(
        || Box::new(NoUtf8EncodeDecode),
        "Utf8DecodeEncodeToMbConvertEncodingRector",
        80400,
    );
}

#[test]
fn filesystem_iterator_skip_dots() {
    run_class(
        vec![Box::new(AddSkipDotsFlag)],
        "FilesystemIteratorSkipDotsRector",
    );
    everywhere(
        || Box::new(AddSkipDotsFlag),
        "FilesystemIteratorSkipDotsRector",
        80400,
    );
}

#[test]
fn variable_in_string_interpolation() {
    run_class(
        vec![Box::new(NoDollarBraceInterpolation)],
        "VariableInStringInterpolationFixerRector",
    );
    everywhere(
        || Box::new(NoDollarBraceInterpolation),
        "VariableInStringInterpolationFixerRector",
        80400,
    );
}
