//! Rector's own fixtures for the ported `types` rules that add parameter types, copied verbatim from rector-src 2.9.2
//! (`.oracle/rector`, rules-tests/...) into tests/rector/<Rector class>/Fixture, with the classes they use in
//! Source/. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-ecosystem`, the rule alone,
//! `phpVersion(80400)`) wrote for them; Pun/ holds the cases where Pun keeps the meaning but not Rector's bytes. Each
//! one must pass as `format(pun lint --fix) == format(expected)`, and the text edits must give the expected bytes.

use crate::common::{run_class, run_class_at, run_class_stubs};
use pun_lint::Rule;
use pun_lint::rules::{
    AddParamTypeArrayAnyAll, AddParamTypeArrayDimFetch, AddParamTypeArrayFunction,
    AddParamTypeArrayMap, AddParamTypeArrayReduce, AddParamTypeArrowDimFetch,
    AddParamTypeDataProvider, AddParamTypeDimFetchKey, AddParamTypeIterableMethodCall,
    AddParamTypeMethodCall, AddParamTypeProperty, AddParamTypeSplFixedArray,
    AddParamTypeVariableCall, PreferNarrowParamType,
};

fn check(rule: impl Rule + 'static, class: &str) {
    run_class(vec![Box::new(rule)], class);
}

#[test]
fn add_array_any_all_closure_param_type() {
    check(
        AddParamTypeArrayAnyAll,
        "AddArrayAnyAllClosureParamTypeRector",
    );
}

#[test]
fn narrow_array_any_all_nullable_param_type() {
    check(
        PreferNarrowParamType,
        "NarrowArrayAnyAllNullableParamTypeRector",
    );
}

#[test]
fn add_array_function_closure_param_type() {
    check(
        AddParamTypeArrayFunction,
        "AddArrayFunctionClosureParamTypeRector",
    );
}

#[test]
fn add_closure_param_type_for_array_map() {
    check(AddParamTypeArrayMap, "AddClosureParamTypeForArrayMapRector");
}

#[test]
fn strict_array_param_dim_fetch() {
    check(AddParamTypeArrayDimFetch, "StrictArrayParamDimFetchRector");
}

#[test]
fn add_arrow_function_param_array_where_dim_fetch() {
    check(
        AddParamTypeArrowDimFetch,
        "AddArrowFunctionParamArrayWhereDimFetchRector",
    );
}

#[test]
fn add_param_type_spl_fixed_array() {
    check(AddParamTypeSplFixedArray, "AddParamTypeSplFixedArrayRector");
}

#[test]
fn add_param_type_from_property_type() {
    check(AddParamTypeProperty, "AddParamTypeFromPropertyTypeRector");
}

#[test]
fn add_closure_param_type_from_variable_call() {
    check(
        AddParamTypeVariableCall,
        "AddClosureParamTypeFromVariableCallRector",
    );
}

#[test]
fn add_param_from_dim_fetch_key_use() {
    check(AddParamTypeDimFetchKey, "AddParamFromDimFetchKeyUseRector");
}

#[test]
fn add_param_type_based_on_phpunit_data_provider() {
    for (class, version) in [
        ("AddParamTypeBasedOnPHPUnitDataProviderRector", 70400),
        ("AddParamTypeBasedOnPHPUnitDataProviderRectorUnion", 80000),
    ] {
        run_class_stubs(
            vec![Box::new(AddParamTypeDataProvider)],
            class,
            version,
            &["phpunit-stubs"],
        );
    }
}

#[test]
fn add_closure_param_type_for_array_reduce() {
    run_class_at(
        vec![Box::new(AddParamTypeArrayReduce)],
        "AddClosureParamTypeForArrayReduceRector",
        80000,
    );
}

#[test]
fn add_method_call_based_strict_param_type_options() {
    let mut rule = AddParamTypeMethodCall::default();
    assert!(
        rule.configure(&serde_json::json!({ "treatClassesAsFinal": true }))
            .is_err()
    );
    assert!(
        rule.configure(&serde_json::json!({ "treat-classes-as-final": "yes" }))
            .is_err()
    );
}

#[test]
fn add_method_call_based_strict_param_type() {
    let base = "AddMethodCallBasedStrictParamTypeRector";
    run_class_stubs(
        vec![Box::new(AddParamTypeMethodCall::default())],
        base,
        70400,
        &["phpunit-stubs"],
    );
    let source = format!("rector/{base}/Source");
    for (suffix, version, treat_classes_as_final) in [
        ("Union", 80000, false),
        ("Intersection", 80100, false),
        ("TreatClassesAsFinal", 80000, true),
        ("Php70", 70000, false),
    ] {
        let mut rule = AddParamTypeMethodCall::default();
        rule.configure(&serde_json::json!({ "treat-classes-as-final": treat_classes_as_final }))
            .unwrap();
        run_class_stubs(
            vec![Box::new(rule)],
            &format!("{base}{suffix}"),
            version,
            &[&source, "phpunit-stubs"],
        );
    }
}

#[test]
fn add_closure_param_type_from_iterable_method_call() {
    for (class, version) in [
        ("AddClosureParamTypeFromIterableMethodCallRector", 80000),
        (
            "AddClosureParamTypeFromIterableMethodCallRectorPhp74",
            70400,
        ),
    ] {
        run_class_at(
            vec![Box::new(AddParamTypeIterableMethodCall)],
            class,
            version,
        );
    }
}
