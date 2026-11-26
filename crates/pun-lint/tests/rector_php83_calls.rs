//! Rector's own fixtures for the PHP 8.3 rules that rewrite a call or a `json_decode()` check, laid out as
//! tests/rector.rs describes: Fixture/ copied verbatim from rector-src 2.9.2 (rules-tests/Php83/...), Oracle/ with
//! what Rector 2.8.4 (`.oracle/rector-bin`, the rule alone, `withPhpVersion(80300)`) wrote, and Pun/ for the cases
//! where Pun keeps the meaning but not Rector's bytes (expected derived by hand).

use pun_lint::rules;

use crate::common::run_class_at;

#[test]
fn json_validate() {
    run_class_at(
        vec![Box::new(rules::PreferJsonValidate)],
        "JsonValidateRector",
        80300,
    );
}

#[test]
fn remove_get_class_get_parent_class_no_args() {
    run_class_at(
        vec![Box::new(rules::PreferSelfClass)],
        "RemoveGetClassGetParentClassNoArgsRector",
        80300,
    );
}

#[test]
fn dynamic_class_const_fetch() {
    run_class_at(
        vec![Box::new(rules::PreferDynamicClassConstant)],
        "DynamicClassConstFetchRector",
        80300,
    );
}

#[test]
fn combine_host_port_ldap_uri() {
    run_class_at(
        vec![Box::new(rules::PreferLdapUri)],
        "CombineHostPortLdapUriRector",
        80300,
    );
}
