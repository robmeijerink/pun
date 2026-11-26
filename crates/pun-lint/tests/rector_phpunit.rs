//! rector-phpunit's own fixtures for the ported `phpunit` rules, copied verbatim from rectorphp/rector-phpunit at
//! d0b6caa (rules-tests/...) into tests/rector/<Rector class>/Fixture (and its Source/). Oracle/ holds extra inputs
//! with what Rector 2.8.4 and that rector-phpunit (`.oracle/rector-ecosystem`, the rule alone, `phpVersion(80400)`)
//! wrote for them; Pun/ holds the documented cases where Pun differs. A fixture is `input -----\n expected`, or only
//! the input when the rule must not change it.
//!
//! rector-phpunit runs its fixtures with phpunit/phpunit 13.2 installed, so every fixture here sees the PHPUnit
//! classes of tests/phpunit-stubs (PHPUnit 13.2.6's files with empty method bodies, scripts/export-phpunit-stub.php)
//! beside its Source/ classes, and the Symfony and Behat classes of rector-phpunit's own stubs/ (tests/rector-phpunit-stubs,
//! verbatim at d0b6caa).
//!
//! Pun/noargs: a call that lacks the arguments a rule reads (`$this->assertSame()`, `is_a($a)`) makes Rector fail on the
//! file; Pun leaves the call alone.
//!
//! Pun/declined (the annotation-to-attribute rules): Pun leaves a doc alone that has another comment before it
//! (php-parser reprints those with the doc), a tag Pun's doc reader does not follow (`@\...`, `@_...`), or a tab in the
//! node's indentation (Rector writes the new attribute and the reprinted doc at its own indentation, in spaces).
//! Pun/tabbed is that last case for `ReplaceTestAnnotationWithPrefixedFunctionRector`; Pun/tabbed7 is the same for
//! `AllowMockObjectsWhereParentClassRector`, `ChangeMockObjectReturnUnionToIntersectionRector` and
//! `CoversAnnotationWithValueToAttributeRector`, and Pun/decl7 the tags Pun's doc reader does not follow (`@_1`, a
//! `})` in a text line, a `__remove_array__` value) for the annotation rules that Rector changes there.
//! Pun/mixed7 (`TestWithToDataProviderRector`) has a tab where the new provider method goes, and Pun/declined7
//! (`CoversAnnotationWithValueToAttributeRector`) a doc comment inside `new ... class`, which Pun does not read.
//! Pun/decl11 (`CoversAnnotationWithValueToAttributeRector`) has the same in a trait's methods (a tag Pun's doc
//! reader does not follow, a tab) and in an anonymous class's method.
//!
//! Pun/never8: Mago types no expression in code after a call that returns `never` (`$this->fail()`), where PHPStan
//! still does, so the rules that need such a type (`AddReturnTypeToDependedRector`,
//! `MergeWithCallableAndWillReturnRector`) leave that code alone.
//!
//! Pun/noargs2 (`NarrowSingleWillReturnCallbackRector`, a returned `match` without arms), Pun/unnamed7
//! (`AssertInstanceOfComparisonRector`, `instanceof` a class expression without a name) and Pun/cyc7
//! (`ConstructClassMethodToSetUpTestCaseRector`, classes that extend each other) make Rector fail on the file; Pun
//! leaves it alone.
//!
//! TypeWillReturnCallableArrowFunctionRector/Pun/intersection_order: the mocked method is declared, with other types, by
//! two classes of the receiver's intersection type; Rector reads the first as written, and Mago keeps an intersection's
//! members in an order of its own, so Pun leaves the callback alone. Pun/setup_fails: `setUp()` mocks a class
//! expression without a name (`('A' . 'B')::class`), which makes Rector fail on the file; Pun leaves it alone.
//!
//! AssertEmptyNullableObjectToAssertInstanceofRector/Pun/narrowed: Mago narrows a variable by an earlier PHPUnit
//! assertion (`self::assertNull($a)` leaves `$a` as `null`), where PHPStan without its PHPUnit extension, as Rector
//! runs it, keeps the declared type; Pun leaves the later call alone.
//!
//! AssertArrayCastedObjectToAssertSameRector/Pun/invalid_name: a key that is not a property name
//! (`'my-key'`) makes Rector write `$object->my-key`, which is not a property fetch; Pun leaves the call alone. Pun
//! also leaves an array alone whose values are not literals, nulls, booleans, numbers, strings or arrays of them.
//!
//! ClosureReturnTypeFromAssertInstanceOfRector: its fixture expects the type `assertInstanceOf()` narrows to, which
//! `.oracle/rector-bin` does not give (it has no PHPUnit extension for PHPStan), so Oracle/ holds only closures whose
//! type does not depend on the narrowing.
//!
//! AddInstanceofAssertForNullableInstanceRector/Pun/expect_instanceof: `expect($x)->toBeInstanceOf(...)`, which
//! `lint/pest/prefer-expect` makes of the `assertInstanceOf()` this rule inserts, counts as that assert, also when
//! `lint/pest/prefer-chained-expect` has chained it with other expectations (after `->not->toBeNull()`, or with
//! `->and($x)` as its subject). Rector runs each
//! rule once and would insert another; Pun fixes until nothing changes, and the two rules would never settle.
//!
//! AddInstanceofAssertForNullableArgumentRector/Pun/expect_reassigned: a variable assigned twice, each value checked by
//! `expect($x)->toBeInstanceOf(...)`. Rector gets the second call's argument as non-nullable from PHPStan's narrowing;
//! Pun skips a variable that an assert or expectation has covered until it is assigned again.
//!
//! Pun/missing11 (`NamedArgumentForDataProviderRector`): a data provider whose arrays have empty items, which
//! php-parser does not parse; Pun leaves it alone.

use std::path::{Path, PathBuf};

use pun_format::Pipeline;
use pun_lint::rules::facts::with_sources;
use pun_lint::rules::*;
use pun_lint::{Linter, Rule};

fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.map(|e| e.unwrap().path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "php") {
            out.push((
                path.to_string_lossy().into_owned(),
                std::fs::read_to_string(&path).unwrap(),
            ));
        }
    }
}

fn format(pipeline: &Pipeline, code: &str) -> String {
    match pipeline
        .run(Path::new("fixture.php"), code.as_bytes())
        .unwrap()
    {
        Some(fixed) => String::from_utf8(fixed.code).unwrap(),
        None => code.to_string(),
    }
}

pub(crate) fn run_class(rules: Vec<Box<dyn Rule>>, class: &str) {
    let linter = Linter::new(rules, pun_lint::PhpVersion::from_id(80400));
    let preset = pun_config::rule_set(pun_config::preset_rules("laravel").unwrap()).unwrap();
    let pipeline = Pipeline::new(&preset, &pun_fixers::all).unwrap();
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let root = base.join("rector").join(class);
    assert!(
        root.join("Fixture").is_dir(),
        "{class}: no Fixture directory"
    );
    let mut extra = Vec::new();
    sources(&base.join("phpunit-stubs"), &mut extra);
    sources(&base.join("rector-phpunit-stubs"), &mut extra);
    sources(&root.join("Source"), &mut extra);
    for sub in ["Fixture", "Oracle", "Pun"] {
        let dir = root.join(sub);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.map(|e| e.unwrap().path()).collect();
        paths.sort();
        assert!(!paths.is_empty(), "no fixtures in {}", dir.display());
        for path in paths {
            let content = std::fs::read_to_string(&path)
                .unwrap()
                .replace("\r\n", "\n");
            let (input, expected) = match content.split_once("-----\n") {
                Some((i, e)) => (i.to_string(), e.to_string()),
                None => (content.clone(), content.clone()),
            };
            let name = format!(
                "{class} {sub}/{}",
                path.file_name().unwrap().to_string_lossy()
            );
            let fixed = with_sources(extra.clone(), || linter.fix(&path, &input)).unwrap();
            assert_eq!(fixed.source, expected, "{name}: lint --fix");
            assert_eq!(
                format(&pipeline, &fixed.source),
                format(&pipeline, &expected),
                "{name}: format(lint --fix)"
            );
            assert_eq!(fixed.applied.is_empty(), input == expected, "{name}");
        }
    }
}

fn check(rule: impl Rule + 'static, class: &str) {
    run_class(vec![Box::new(rule)], class);
}

#[test]
fn remove_empty_test_method() {
    check(NoEmptyTest, "RemoveEmptyTestMethodRector");
}

#[test]
fn assert_not_operator() {
    check(NoNegatedAssertion, "AssertNotOperatorRector");
}

#[test]
fn assert_same_bool_null_to_specific_method() {
    check(
        PreferAssertNullTrueFalse,
        "AssertSameBoolNullToSpecificMethodRector",
    );
}

#[test]
fn assert_same_true_false_to_assert_true_false() {
    check(
        PreferAssertTrueFalse,
        "AssertSameTrueFalseToAssertTrueFalseRector",
    );
}

#[test]
fn assert_false_strpos_to_contains() {
    check(
        PreferAssertStringContains,
        "AssertFalseStrposToContainsRector",
    );
}

#[test]
fn assert_is_type_method_call() {
    check(NoAssertIsType, "AssertIsTypeMethodCallRector");
}

#[test]
fn assert_contains_only_method_call() {
    check(NoAssertContainsOnly, "AssertContainsOnlyMethodCallRector");
}

#[test]
fn assert_comparison_to_specific_method() {
    check(
        PreferComparisonAssertion,
        "AssertComparisonToSpecificMethodRector",
    );
}

#[test]
fn assert_instance_of_comparison() {
    check(PreferAssertInstanceof, "AssertInstanceOfComparisonRector");
}

#[test]
fn assert_empty_nullable_object_to_assert_instanceof() {
    check(
        PreferAssertInstanceofOverEmpty,
        "AssertEmptyNullableObjectToAssertInstanceofRector",
    );
}

#[test]
fn assert_array_casted_object_to_assert_same() {
    check(
        PreferPropertyAssertions,
        "AssertArrayCastedObjectToAssertSameRector",
    );
}

#[test]
fn closure_return_type_from_assert_instance_of() {
    check(
        AddClosureReturnTypeFromAssert,
        "ClosureReturnTypeFromAssertInstanceOfRector",
    );
}

#[test]
fn flip_assert() {
    check(NoFlippedAssertion, "FlipAssertRector");
}

#[test]
fn assert_equals_or_assert_same_float_parameter_to_specific_methods_type() {
    check(
        NoFloatDeltaParameter,
        "AssertEqualsOrAssertSameFloatParameterToSpecificMethodsTypeRector",
    );
}

#[test]
fn assert_true_false_to_specific_method() {
    check(
        PreferSpecificTrueFalseAssertion,
        "AssertTrueFalseToSpecificMethodRector",
    );
}

#[test]
fn assert_isset_to_specific_method() {
    check(PreferAssertArrayHasKey, "AssertIssetToSpecificMethodRector");
}

#[test]
fn string_cast_assert_string_contains_string() {
    check(
        RequireStringAssertArgument,
        "StringCastAssertStringContainsStringRector",
    );
}

#[test]
fn simplify_foreach_instance_of() {
    check(PreferAssertInstancesOf, "SimplifyForeachInstanceOfRector");
}

#[test]
fn match_assert_same_expected_type() {
    check(
        NoAssertSameTypeMismatch,
        "MatchAssertSameExpectedTypeRector",
    );
}

#[test]
fn assert_compare_on_countable_with_method_to_assert_count() {
    check(
        PreferAssertCount,
        "AssertCompareOnCountableWithMethodToAssertCountRector",
    );
}

#[test]
fn remove_expect_any_from_mock() {
    check(NoExpectsAny, "RemoveExpectAnyFromMockRector");
}

#[test]
fn use_specific_with_method() {
    check(PreferSimpleWithMatcher, "UseSpecificWithMethodRector");
}

#[test]
fn use_specific_will_method() {
    check(PreferSpecificWillMethod, "UseSpecificWillMethodRector");
}

#[test]
fn single_with_consecutive_to_with() {
    check(
        PreferSimpleMockExpectation,
        "SingleWithConsecutiveToWithRector",
    );
}

#[test]
fn narrow_identical_with_consecutive() {
    check(
        PreferNarrowWithConsecutive,
        "NarrowIdenticalWithConsecutiveRector",
    );
}

#[test]
fn remove_override_final_construct_test_case() {
    check(
        NoFinalConstructorOverride,
        "RemoveOverrideFinalConstructTestCaseRector",
    );
}

#[test]
fn no_setup_with_parent_call_override() {
    check(
        NoSetupOverrideAttribute,
        "NoSetupWithParentCallOverrideRector",
    );
}

#[test]
fn declare_strict_types_tests() {
    check(RequireStrictTypesInTests, "DeclareStrictTypesTestsRector");
}

#[test]
fn prefer_p_h_p_unit_this_call() {
    check(PreferThisCall, "PreferPHPUnitThisCallRector");
}

#[test]
fn remove_standalone_create_mock() {
    check(NoUnusedMock, "RemoveStandaloneCreateMockRector");
}

#[test]
fn replace_at_method_with_desired_matcher() {
    check(NoAtMatcher, "ReplaceAtMethodWithDesiredMatcherRector");
}

#[test]
fn get_mock_builder_get_mock_to_create_mock() {
    check(NoBareMockBuilder, "GetMockBuilderGetMockToCreateMockRector");
}

#[test]
fn create_stub_in_coalesce_arg() {
    check(PreferCreateStubInCoalesce, "CreateStubInCoalesceArgRector");
}

#[test]
fn data_provider_array_items_new_lined() {
    check(
        RequireDataProviderNewlines,
        "DataProviderArrayItemsNewLinedRector",
    );
}

#[test]
fn simpler_with_is_instance_of() {
    check(PreferWithIsInstanceOf, "SimplerWithIsInstanceOfRector");
}

#[test]
fn callback_single_assert_to_simpler() {
    check(
        PreferSimpleCallbackAssert,
        "CallbackSingleAssertToSimplerRector",
    );
}

#[test]
fn narrow_single_will_return_callback() {
    check(
        PreferNarrowWillReturnCallback,
        "NarrowSingleWillReturnCallbackRector",
    );
}

#[test]
fn assert_func_call_to_p_h_p_unit_assert() {
    check(PreferAssert, "AssertFuncCallToPHPUnitAssertRector");
}

#[test]
fn merge_with_callable_and_will_return() {
    check(
        PreferMergedWillReturnCallback::default(),
        "MergeWithCallableAndWillReturnRector",
    );
}

#[test]
fn add_return_type_to_depended() {
    check(AddDependedTestReturnType, "AddReturnTypeToDependedRector");
}

#[test]
fn construct_class_method_to_set_up_test_case() {
    check(
        NoTestConstructor,
        "ConstructClassMethodToSetUpTestCaseRector",
    );
}

#[test]
fn named_argument_for_data_provider() {
    check(
        PreferNamedDataProviderKeys,
        "NamedArgumentForDataProviderRector",
    );
}

#[test]
fn replace_test_annotation_with_prefixed_function() {
    check(
        PreferTestPrefix,
        "ReplaceTestAnnotationWithPrefixedFunctionRector",
    );
}

#[test]
fn direct_instance_over_mock_arg() {
    check(NoValueObjectMock, "DirectInstanceOverMockArgRector");
}

#[test]
fn bare_create_mock_assign_to_direct_use() {
    check(NoBareMockAssign, "BareCreateMockAssignToDirectUseRector");
}

#[test]
fn with_callback_identical_to_standalone_asserts() {
    check(
        PreferStandaloneAssertsOverCallback,
        "WithCallbackIdenticalToStandaloneAssertsRector",
    );
}

#[test]
fn ticket_annotation_to_attribute() {
    check(PreferTicketAttribute, "TicketAnnotationToAttributeRector");
}

#[test]
fn data_provider_annotation_to_attribute() {
    check(
        PreferDataProviderAttribute,
        "DataProviderAnnotationToAttributeRector",
    );
}

#[test]
fn requires_annotation_with_value_to_attribute() {
    check(
        PreferRequiresAttribute,
        "RequiresAnnotationWithValueToAttributeRector",
    );
}

#[test]
fn depends_annotation_with_value_to_attribute() {
    check(
        PreferDependsAttribute,
        "DependsAnnotationWithValueToAttributeRector",
    );
}

#[test]
fn covers_annotation_with_value_to_attribute() {
    check(
        PreferCoversAttribute,
        "CoversAnnotationWithValueToAttributeRector",
    );
}

#[test]
fn test_with_annotation_to_attribute() {
    check(
        PreferTestWithAttribute,
        "TestWithAnnotationToAttributeRector",
    );
}

#[test]
fn annotation_with_value_to_attribute() {
    let mut rule = PreferAttributeOverAnnotation::default();
    assert!(rule.configure(&serde_json::json!(null)).is_ok());
    assert!(rule.configure(&serde_json::json!(true)).is_err());
    assert!(rule.configure(&serde_json::json!({"other": []})).is_err());
    assert!(rule.configure(&serde_json::json!({"items": true})).is_err());
    assert!(rule.configure(&serde_json::json!({"items": [1]})).is_err());
    assert!(
        rule.configure(&serde_json::json!({"items": [{"annotation-name": 1}]}))
            .is_err()
    );
    assert!(
        rule.configure(&serde_json::json!({"items": [{"annotation-name": "a"}]}))
            .is_err()
    );
    rule.configure(&serde_json::json!({ "items": [
        { "annotation-name": "backupGlobals", "attribute-class": "PHPUnit\\Framework\\Attributes\\BackupGlobals",
          "value-map": { "enabled": true, "disabled": false }, "is-on-class-level": false },
        { "annotation-name": "dataProvider", "attribute-class": "PHPUnit\\Framework\\Attributes\\DataProvider",
          "value-map": [], "is-on-class-level": false },
        { "annotation-name": "uses", "attribute-class": "PHPUnit\\Framework\\Attributes\\UsesClass",
          "value-map": [], "is-on-class-level": true },
        { "annotation-name": "testdox", "attribute-class": "PHPUnit\\Framework\\Attributes\\TestDox",
          "value-map": [], "is-on-class-level": false },
        { "annotation-name": "backupStaticAttributes",
          "attribute-class": "PHPUnit\\Framework\\Attributes\\BackupStaticProperties",
          "value-map": { "enabled": 1, "disabled": 2.5, "maybe": "yes" } },
        { "annotation-name": "Some\\Tag", "attribute-class": "PHPUnit\\Framework\\Attributes\\Group" },
    ]}))
    .unwrap();
    check(rule, "AnnotationWithValueToAttributeRector");
}

#[test]
fn test_with_to_data_provider() {
    check(PreferDataProvider, "TestWithToDataProviderRector");
}

#[test]
fn finalize_test_case_class() {
    check(RequireFinalTestCase, "FinalizeTestCaseClassRector");
}

#[test]
fn add_tests_void_return_type_where_no_return() {
    check(
        AddTestVoidReturn,
        "AddTestsVoidReturnTypeWhereNoReturnRector",
    );
}

#[test]
fn return_iterator_in_data_provider() {
    check(
        AddDataProviderIteratorReturnType,
        "ReturnIteratorInDataProviderRector",
    );
}

#[test]
fn return_type_from_mock_object() {
    check(
        AddReturnTypeFromMockObject,
        "ReturnTypeFromMockObjectRector",
    );
}

#[test]
fn expression_create_mock_to_create_stub() {
    check(
        PreferCreateStubExpression,
        "ExpressionCreateMockToCreateStubRector",
    );
}

#[test]
fn single_mock_property_type() {
    check(NoMockIntersectionProperty, "SingleMockPropertyTypeRector");
}

#[test]
fn remove_never_used_mock_property() {
    check(NoUnusedMockProperty, "RemoveNeverUsedMockPropertyRector");
}

#[test]
fn property_create_mock_to_create_stub() {
    check(PreferCreateStub, "PropertyCreateMockToCreateStubRector");
}

#[test]
fn decorate_will_return_map_with_expects_mock() {
    check(
        RequireMockExpects,
        "DecorateWillReturnMapWithExpectsMockRector",
    );
}

#[test]
fn inline_stub_property_to_create_stub_method_call() {
    check(
        PreferInlineCreateStub,
        "InlineStubPropertyToCreateStubMethodCallRector",
    );
}

#[test]
fn mock_object_var_to_stub() {
    check(PreferStubVar, "MockObjectVarToStubRector");
}

#[test]
fn change_mock_object_return_union_to_intersection() {
    check(
        PreferMockIntersectionReturn,
        "ChangeMockObjectReturnUnionToIntersectionRector",
    );
}

#[test]
fn add_intersection_var_to_mock_object_property() {
    check(
        AddMockIntersectionType,
        "AddIntersectionVarToMockObjectPropertyRector",
    );
}

#[test]
fn add_stub_intersection_var_to_stub_property() {
    check(
        AddStubIntersectionType,
        "AddStubIntersectionVarToStubPropertyRector",
    );
}

#[test]
fn expects_param_to_mock_object() {
    check(AddMockObjectType, "ExpectsParamToMockObjectRector");
}

#[test]
fn add_intersection_param_to_mock_object_param() {
    check(
        AddMockIntersectionParam,
        "AddIntersectionParamToMockObjectParamRector",
    );
}

#[test]
fn allow_mock_objects_where_parent_class() {
    check(
        AddAllowMockObjectsAttribute,
        "AllowMockObjectsWhereParentClassRector",
    );
}

#[test]
fn allow_mock_objects_for_data_provider() {
    check(
        AddDataProviderAllowMockObjects,
        "AllowMockObjectsForDataProviderRector",
    );
}

#[test]
fn add_param_type_from_depends() {
    check(AddParamTypeDepends, "AddParamTypeFromDependsRector");
}

#[test]
fn narrow_unused_set_up_defined_property() {
    check(
        PreferLocalOverSetupProperty,
        "NarrowUnusedSetUpDefinedPropertyRector",
    );
}

#[test]
fn scalar_argument_to_expected_param_type() {
    check(
        RequireExactTestArgumentType,
        "ScalarArgumentToExpectedParamTypeRector",
    );
}

#[test]
fn entity_document_create_mock_to_direct_new() {
    check(NoEntityMock, "EntityDocumentCreateMockToDirectNewRector");
}

#[test]
fn yield_data_provider() {
    check(PreferYieldDataProvider, "YieldDataProviderRector");
}

#[test]
fn mock_object_arg_create_stub_to_create_mock() {
    check(
        NoStubWithExpectations,
        "MockObjectArgCreateStubToCreateMockRector",
    );
}

#[test]
fn create_stub_over_create_mock_arg() {
    check(
        PreferCreateStubArgument,
        "CreateStubOverCreateMockArgRector",
    );
}

#[test]
fn void_method_with_callback_to_will_return_callback() {
    check(
        PreferWillReturnCallback::default(),
        "VoidMethodWithCallbackToWillReturnCallbackRector",
    );
}

#[test]
fn type_will_return_callable_arrow_function() {
    check(
        AddWillReturnCallbackParamType,
        "TypeWillReturnCallableArrowFunctionRector",
    );
}

#[test]
fn add_instanceof_assert_for_nullable_instance() {
    check(
        AddInstanceofAssert,
        "AddInstanceofAssertForNullableInstanceRector",
    );
}

#[test]
fn add_instanceof_assert_for_nullable_argument() {
    check(
        AddInstanceofAssertForArgument,
        "AddInstanceofAssertForNullableArgumentRector",
    );
}

#[test]
fn any_matcher_to_new_any_invoked_count() {
    check(NoAnyMatcher, "AnyMatcherToNewAnyInvokedCountRector");
}

#[test]
fn will_return_callback_fallback_to_return_false() {
    check(
        AddWillReturnCallbackFalseFallback,
        "WillReturnCallbackFallbackToReturnFalseRector",
    );
}
