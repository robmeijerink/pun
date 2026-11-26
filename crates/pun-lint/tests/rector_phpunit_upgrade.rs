//! rector-phpunit's fixtures for its upgrade rules below phpunit110 (the `PHPUnit50` ... `PHPUnit100` rules that
//! the composer-based set registers), copied verbatim from rectorphp/rector-phpunit at d0b6caa (rules-tests/...) into
//! tests/rector/<Rector class>/Fixture (and its Source/), run as `rector_phpunit.rs` runs its fixtures. Oracle/ holds
//! extra inputs with what Rector 2.8.4 and that rector-phpunit (`.oracle/rector-ecosystem`, the rule alone,
//! `phpVersion(80400)`, tests/phpunit-stubs loaded by a bootstrap autoloader, and a `vendor/composer/installed.json`
//! with the phpunit/phpunit version the rule's constraint asks for) wrote for them. Pun/ holds the documented cases
//! where Pun differs:
//!
//! PublicDataProviderClassMethodRector/Pun/respect_parent_*: two upstream fixtures, kept here with Pun's output.
//! Rector also reads the `@dataProvider` tags of the test methods in the abstract parent classes below `TestCase`
//! (their AST, from another file); Pun reads only the class's own test methods, so it leaves a provider that only a
//! parent's test names alone.
//!
//! RemoveDataProviderTestPrefixRector/Pun/imported_data_provider: a `use` of a class named `DataProvider` (here the
//! attribute) makes Rector's PHPDoc parser read `@dataProvider` as a Doctrine annotation of that class, so Rector
//! finds no provider through the tag (`PublicDataProviderClassMethodRector` too); Pun reads the tag either way.
//!
//! AssertRegExpRector/Pun/condition_variable: an expected value that is not an integer or a constant, or a
//! `preg_match()` without a subject, makes Rector fail on the file; Pun leaves the call alone.
//!
//! AddDoesNotPerformAssertionToNonAssertingTestRector/Annotation: Rector run without PHPUnit 13's classes, on a test
//! that extends rector-phpunit's `PHPUnit_Framework_TestCase` stub (AnnotationStubs/, verbatim at 22c6dae), so the
//! `#[DoesNotPerformAssertions]` class does not exist and the rule writes the `@doesNotPerformAssertions` tag.
//!
//! WithConsecutiveRector/Pun/rector_fails: a top-level `callback()` that is not `fn ($x) => $x === ...`, and a
//! `willReturn()` without an argument, make Rector fail on the file; Pun leaves the statement alone.
//!
//! TestListenerToHooksRector/Pun/before_list_hook: the upstream fixture with the blank line between the two kept
//! methods, which Pun keeps as written where Rector's reprint of the class drops it.

use crate::rector_phpunit::run_class;
use pun_lint::Rule;
use pun_lint::rules::{
    AddDoesNotPerformAssertions, AddParamTypeDeclaration, AddParentTestConstructor,
    AddProphecyTrait, AddReturnTypeDeclaration, NoAssertClassHasAttribute,
    NoAssertContainsOnString, NoDataProviderTestPrefix, NoMockBuilderSetMethods,
    NoScenarioAnnotation, NoTestListener, NoWithConsecutive, PreferAssertContainsEquals,
    PreferAssertMatchesRegularExpression, PreferExpectPhpError, PreferSpecificAssertEquals,
    RequirePublicDataProvider,
};
use serde_json::json;

fn check(rule: impl Rule + 'static, class: &str) {
    run_class(vec![Box::new(rule)], class);
}

#[test]
fn specific_assert_contains() {
    check(NoAssertContainsOnString, "SpecificAssertContainsRector");
}

#[test]
fn specific_assert_contains_without_identity() {
    check(
        PreferAssertContainsEquals,
        "SpecificAssertContainsWithoutIdentityRector",
    );
}

#[test]
fn assert_equals_parameter_to_specific_methods_type() {
    check(
        PreferSpecificAssertEquals,
        "AssertEqualsParameterToSpecificMethodsTypeRector",
    );
}

#[test]
fn assert_reg_exp() {
    check(PreferAssertMatchesRegularExpression, "AssertRegExpRector");
}

#[test]
fn explicit_php_error_api() {
    check(PreferExpectPhpError, "ExplicitPhpErrorApiRector");
}

#[test]
fn property_exists_without_assert() {
    check(
        NoAssertClassHasAttribute,
        "PropertyExistsWithoutAssertRector",
    );
}

#[test]
fn remove_set_methods_method_call() {
    check(NoMockBuilderSetMethods, "RemoveSetMethodsMethodCallRector");
}

#[test]
fn public_data_provider_class_method() {
    check(
        RequirePublicDataProvider,
        "PublicDataProviderClassMethodRector",
    );
}

#[test]
fn remove_data_provider_test_prefix() {
    check(
        NoDataProviderTestPrefix,
        "RemoveDataProviderTestPrefixRector",
    );
}

#[test]
fn scenario_to_test_annotation() {
    check(NoScenarioAnnotation, "ScenarioToTestAnnotationRector");
}

#[test]
fn add_prophecy_trait() {
    check(AddProphecyTrait, "AddProphecyTraitRector");
}

#[test]
fn parent_test_class_constructor() {
    check(AddParentTestConstructor, "ParentTestClassConstructorRector");
}

#[test]
fn test_listener_to_hooks() {
    check(NoTestListener, "TestListenerToHooksRector");
}

#[test]
fn add_does_not_perform_assertion_to_non_asserting_test() {
    let class = "AddDoesNotPerformAssertionToNonAssertingTestRector";
    check(AddDoesNotPerformAssertions, class);
    // without PHPUnit 10's attribute class, the `@doesNotPerformAssertions` tag
    let stubs = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rector")
        .join(class)
        .join("AnnotationStubs");
    crate::common::run_dirs(
        vec![Box::new(AddDoesNotPerformAssertions)],
        class,
        &["Annotation"],
        80400,
        &[&stubs],
    );
}

#[test]
fn with_consecutive() {
    check(NoWithConsecutive, "WithConsecutiveRector");
}

/// Rector's configured core rules that the PHPUnit sets configure, with its fixtures (rector-src's rules-tests at
/// 2.8.4) and the configuration of their config/configured_rule.php, each PHPStan type as `pun migrate` writes it.
#[test]
fn add_param_type_declaration() {
    let src = "Rector\\Tests\\TypeDeclaration\\Rector\\ClassMethod\\AddParamTypeDeclarationRector\\Source\\";
    let item = |class: &str, method: &str, t: &str| json!({"class-name": format!("{src}{class}"), "method-name": method, "position": 0, "param-type": t});
    // Oracle/: what Rector 2.8.4 (`.oracle/rector-ecosystem`, the rule alone with these `Lib\Base` items,
    // `phpVersion(80400)`) wrote
    let lib = |method: &str, position: u32, t: &str| json!({"class-name": "Lib\\Base", "method-name": method, "position": position, "param-type": t});
    let mut rule = AddParamTypeDeclaration::default();
    rule.configure(&json!({"items": [
        item("Contract\\ParentInterfaceWithChangeTypeInterface", "process", "string"),
        item("Contract\\ParentInterfaceWithChangeTypeInterface", "run", "string"),
        item("ParserInterface", "parse", "string"),
        item("ClassMetadataFactory", "setEntityManager", "\\Doctrine\\ORM\\EntityManagerInterface"),
        item("Contract\\ParentTypeToMixed", "process", "mixed"),
        lib("make", 0, "int"),
        lib("make", 5, "int"),
        lib("build", 0, "(\\Lib\\Base | null)"),
        lib("load", 0, "(int | string)"),
    ]}))
    .unwrap();
    crate::common::run_class_at(vec![Box::new(rule)], "AddParamTypeDeclarationRector", 80000);
}

#[test]
fn add_return_type_declaration() {
    let ns =
        "Rector\\Tests\\TypeDeclaration\\Rector\\ClassMethod\\AddReturnTypeDeclarationRector\\";
    let item = |class: &str, method: &str, t: &str| json!({"class": format!("{ns}{class}"), "method": method, "return-type": t});
    let file = format!("\\{ns}Source\\FileInterface");
    let lib =
        |method: &str, t: &str| json!({"class": "Lib\\Base", "method": method, "return-type": t});
    let mut rule = AddReturnTypeDeclaration::default();
    rule.configure(&json!({"items": [
        item("Source\\PHPUnitTestCase", "tearDown", "void"),
        item("Fixture\\ReturnTheMixed", "create", "mixed"),
        item("Fixture\\ReturnOfStatic", "create", "static"),
        item("Source\\DataTransformerInterface", "transform", "mixed"),
        item("Source\\FormTypeInterface", "getParent", "(string | null)"),
        item("Source\\FolderInterface", "create", &format!("({file} | null)")),
        item("Fixture\\SkipAlreadyStaticReturn", "transform", "static"),
        item("Fixture\\ReturnDifferentOverStatic", "transform", "static"),
        lib("make", "\\Lib\\Base"),
        lib("build", "(\\Lib\\Base | null)"),
        lib("load", "(int | string)"),
        lib("none", "(string | null)"),
    ]}))
    .unwrap();
    crate::common::run_class(vec![Box::new(rule)], "AddReturnTypeDeclarationRector");
}

/// Pun declares only native types: a generic, an intersection or a nullable union is a config error, as is a negative
/// position.
#[test]
fn type_declaration_options() {
    let bad = |t: &str| {
        format!(
            "\"{t}\" is not a type Pun can declare; use a native type such as `string`, `?\\Foo\\Bar` or `int|string`"
        )
    };
    for t in [
        "array<int, string>",
        "\\A&\\B",
        "?int|string",
        "int|",
        "Not-A-Class",
    ] {
        let mut rule = AddReturnTypeDeclaration::default();
        let options = json!({"items": [{"class": "A", "method": "m", "return-type": t}]});
        assert_eq!(rule.configure(&options).unwrap_err().to_string(), bad(t));
    }
    let mut rule = AddParamTypeDeclaration::default();
    let options = json!({"items": [{"class-name": "A", "method-name": "m", "position": -1, "param-type": "int"}]});
    assert_eq!(
        rule.configure(&options).unwrap_err().to_string(),
        "`position` must be 0 or more"
    );
}
