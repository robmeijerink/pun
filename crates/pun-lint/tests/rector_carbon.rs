//! Rector 2.9.2's own fixtures for its datetime-to-carbon rules, copied verbatim from rector-src's rules-tests at 2.9.2
//! (Carbon/Rector/...) into tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4
//! (`.oracle/rector-bin`, the rule alone, `phpVersion(80400)`) wrote for them. `lint/style/prefer-carbon` is both
//! `DateTimeInstanceToCarbonRector` and `DateTimeMethodCallToCarbonRector`, so it runs on the fixtures of both.
//!
//! DateFuncCallToCarbonRector/Pun/dropped_parts: where Rector's rewrite drops part of the code (an unpacked argument,
//! a named argument out of place, a non-integer operand of `time() - 60 * $x`, the base timestamp of
//! `strtotime($date, $base)` inside `date()`), Pun leaves the call alone or takes the timestamp path.

use crate::common::run_class;
use pun_lint::rules::{PreferCarbon, PreferCarbonDate, PreferCarbonTimestamp};

#[test]
fn date_time_instance_to_carbon() {
    run_class(
        vec![Box::new(PreferCarbon)],
        "DateTimeInstanceToCarbonRector",
    );
}

#[test]
fn date_time_method_call_to_carbon() {
    run_class(
        vec![Box::new(PreferCarbon)],
        "DateTimeMethodCallToCarbonRector",
    );
}

#[test]
fn time_func_call_to_carbon() {
    run_class(
        vec![Box::new(PreferCarbonTimestamp)],
        "TimeFuncCallToCarbonRector",
    );
}

#[test]
fn date_func_call_to_carbon() {
    run_class(
        vec![Box::new(PreferCarbonDate)],
        "DateFuncCallToCarbonRector",
    );
}
