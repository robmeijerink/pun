//! Rector 2.9.2's own fixtures for the `php-5.3` rules, copied verbatim from rector-src (rules-tests/Php53/...) into
//! tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the
//! rule alone, `withPhpVersion(80400)`) wrote for them, and Pun/ the inputs where Pun departs from Rector on purpose.
//! Each rule also runs on every node of its fixtures (`everywhere`) and must report what it reports on its target
//! nodes.

use crate::common::run_class;
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::NoHttpVars;

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn replace_http_server_vars_by_server() {
    check(
        || Box::new(NoHttpVars),
        "ReplaceHttpServerVarsByServerRector",
    );
}
