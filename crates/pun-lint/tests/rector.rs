//! Rector's own fixtures for the ported rules, copied verbatim from rector-src 2.9.2 (`.oracle/rector`,
//! rules-tests/...) into tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4
//! (`.oracle/rector-bin`, the rule alone) wrote for them; Pun/ holds the cases where Pun keeps the meaning but not
//! Rector's bytes, its expected output derived by hand. A fixture is `input -----\n expected`, or only the input when
//! the rule must not change it (Rector's FixtureSplitter). Each one must pass as
//! `format(pun lint --fix) == format(expected)` (lint.md §6), with format being the laravel preset. The text edits
//! also reproduce Rector's bytes before format, which is checked too.

use std::path::{Path, PathBuf};

use pun_format::Pipeline;
use pun_lint::Linter;
use pun_lint::rules;
use pun_lint::rules::NoDirnameDirConcat;
use pun_lint::rules::NoDuplicateCaseBody;
use pun_lint::rules::NoElseAfterReturn;
use pun_lint::rules::NoIdenticalBranches;
use pun_lint::rules::NoNegatedInstanceofTernary;
use pun_lint::rules::NoOverriddenAssignBeforeIf;
use pun_lint::rules::NoRedundantPromotedAssign;
use pun_lint::rules::NoRedundantTypeCheck;
use pun_lint::rules::NoUnreachableCode;
use pun_lint::rules::PreferEarlyReturn;
use pun_lint::rules::PreferEarlyReturnOverPreparedValue;
use pun_lint::rules::PreferExplicitArrayComparison;
use pun_lint::rules::PreferIfStatement;
use pun_lint::rules::PreferInArray;
use pun_lint::rules::PreferNotInArray;
use pun_lint::rules::{
    NoAndAssignment, NoConcatAssignNewItem, NoNegatedAnd, NoNegatedBooleanLiteral, NoNegatedGroup,
    NoSingleItemInArray, NoTautologyTernary, NoUselessArrayValues, PreferEmptyArrayCompare,
    PreferFuncNumArgs, PreferNullCoalesce,
};

fn fixtures(dir: &str) -> Vec<(PathBuf, String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rector")
        .join(dir);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no fixtures in {}", dir.display());
    paths
        .into_iter()
        .map(|path| {
            let content = std::fs::read_to_string(&path)
                .unwrap()
                .replace("\r\n", "\n");
            let (input, expected) = match content.split_once("-----\n") {
                Some((i, e)) => (i.to_string(), e.to_string()),
                None => (content.clone(), content),
            };
            (path, input, expected)
        })
        .collect()
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

fn laravel() -> Pipeline {
    let rules = pun_config::rule_set(pun_config::preset_rules("laravel").unwrap()).unwrap();
    let pipeline = Pipeline::new(&rules, &pun_fixers::all).unwrap();
    assert!(pipeline.not_ported.is_empty(), "{:?}", pipeline.not_ported);
    pipeline
}

fn run(linter: &Linter, dir: &str) {
    let pipeline = laravel();
    for (path, input, expected) in fixtures(dir) {
        let name = path.file_name().unwrap().to_string_lossy();
        let fixed = linter.fix(&path, &input).unwrap();
        assert_eq!(fixed.source, expected, "{name}: lint --fix");
        assert_eq!(
            format(&pipeline, &fixed.source),
            format(&pipeline, &expected),
            "{name}: format(lint --fix)"
        );
        if input != expected {
            assert_ne!(
                format(&pipeline, &input),
                format(&pipeline, &expected),
                "{name}"
            );
            assert!(!fixed.applied.is_empty(), "{name}");
        } else {
            assert!(fixed.applied.is_empty(), "{name}");
        }
    }
}

#[test]
fn and_assigns_to_separate_lines() {
    let linter = Linter::new(
        vec![Box::new(NoAndAssignment)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "AndAssignsToSeparateLinesRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "AndAssignsToSeparateLinesRector/Oracle");
    run(&linter, "AndAssignsToSeparateLinesRector/Pun");
}

#[test]
fn array_key_exists_ternary_then_value_to_coalescing() {
    let linter = Linter::new(
        vec![Box::new(PreferNullCoalesce::default())],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(
        &linter,
        "ArrayKeyExistsTernaryThenValueToCoalescingRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(
        &linter,
        "ArrayKeyExistsTernaryThenValueToCoalescingRector/Oracle",
    );
}

#[test]
fn negated_ands_to_positive_ors() {
    let linter = Linter::new(
        vec![Box::new(NoNegatedAnd::default())],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "NegatedAndsToPositiveOrsRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "NegatedAndsToPositiveOrsRector/Oracle");
    run(&linter, "NegatedAndsToPositiveOrsRector/Pun");
}

#[test]
fn new_array_item_concat_assign_to_assign() {
    let linter = Linter::new(
        vec![Box::new(NoConcatAssignNewItem)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "NewArrayItemConcatAssignToAssignRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "NewArrayItemConcatAssignToAssignRector/Oracle");
}

#[test]
fn replace_constant_boolean_not() {
    let linter = Linter::new(
        vec![Box::new(NoNegatedBooleanLiteral)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "ReplaceConstantBooleanNotRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "ReplaceConstantBooleanNotRector/Oracle");
}

#[test]
fn simplify_de_morgan_binary() {
    let linter = Linter::new(
        vec![Box::new(NoNegatedGroup::default())],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "SimplifyDeMorganBinaryRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "SimplifyDeMorganBinaryRector/Oracle");
    run(&linter, "SimplifyDeMorganBinaryRector/Pun");
}

#[test]
fn simplify_empty_array_check() {
    let linter = Linter::new(
        vec![Box::new(PreferEmptyArrayCompare::default())],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "SimplifyEmptyArrayCheckRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "SimplifyEmptyArrayCheckRector/Oracle");
}

#[test]
fn simplify_func_get_args_count() {
    let linter = Linter::new(
        vec![Box::new(PreferFuncNumArgs)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "SimplifyFuncGetArgsCountRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "SimplifyFuncGetArgsCountRector/Oracle");
}

#[test]
fn simplify_in_array_values() {
    let linter = Linter::new(
        vec![Box::new(NoUselessArrayValues)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "SimplifyInArrayValuesRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "SimplifyInArrayValuesRector/Oracle");
    run(&linter, "SimplifyInArrayValuesRector/Pun");
}

#[test]
fn simplify_tautology_ternary() {
    let linter = Linter::new(
        vec![Box::new(NoTautologyTernary::default())],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "SimplifyTautologyTernaryRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "SimplifyTautologyTernaryRector/Oracle");
}

#[test]
fn single_in_array_to_compare() {
    let linter = Linter::new(
        vec![Box::new(NoSingleItemInArray)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "SingleInArrayToCompareRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "SingleInArrayToCompareRector/Oracle");
    run(&linter, "SingleInArrayToCompareRector/Pun");
}

#[test]
fn every_rule_id_is_a_lint_rule_of_the_mapping() {
    let mapping = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../pun-config/rules/mapping.toml"),
    )
    .unwrap();
    let lint = &mapping[mapping.find("[rules.lint]").unwrap()..];
    let lint = &lint[..lint[1..].find("\n[").unwrap()];
    for rule in pun_lint::rules::all() {
        let key = format!("\n\"{}\" = {{", rule.id().strip_prefix("lint/").unwrap());
        assert!(lint.contains(&key), "{} is not in [rules.lint]", rule.id());
    }
}

fn run_with_whitespace_exceptions(linter: &Linter, dir: &str, whitespace_only: &[&str]) {
    let pipeline = laravel();
    let squeeze = |s: &str| s.split_whitespace().collect::<String>();
    for (path, input, expected) in fixtures(dir) {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let fixed = linter.fix(&path, &input).unwrap();
        if whitespace_only.contains(&name.as_str()) {
            assert_ne!(fixed.source, expected, "{name}: no longer an exception");
            assert_eq!(
                squeeze(&fixed.source),
                squeeze(&expected),
                "{name}: lint --fix"
            );
        } else {
            assert_eq!(fixed.source, expected, "{name}: lint --fix");
        }
        assert_eq!(
            format(&pipeline, &fixed.source),
            format(&pipeline, &expected),
            "{name}: format(lint --fix)"
        );
        assert_eq!(input != expected, !fixed.applied.is_empty(), "{name}");
    }
}

fn only(rule: impl pun_lint::Rule + 'static, php_version: u32) -> Linter {
    Linter::new(
        vec![Box::new(rule)],
        pun_lint::PhpVersion::from_id(php_version),
    )
}

#[test]
fn remove_and_true() {
    run(
        &only(rules::NoAndTrue, 80400),
        "RemoveAndTrueRector/Fixture",
    );
}

#[test]
fn remove_concat_autocast() {
    run(
        &only(rules::NoCastInConcat, 80400),
        "RemoveConcatAutocastRector/Fixture",
    );
}

#[test]
fn remove_dead_continue() {
    run(
        &only(rules::NoUselessContinue, 80400),
        "RemoveDeadContinueRector/Fixture",
    );
}

#[test]
fn remove_dead_if_foreach_for() {
    run(
        &only(rules::NoEmptyBlock::default(), 80400),
        "RemoveDeadIfForeachForRector/Fixture",
    );
}

#[test]
fn remove_dead_try_catch() {
    run(
        &only(rules::NoUselessCatch, 80400),
        "RemoveDeadTryCatchRector/Fixture",
    );
}

#[test]
fn remove_double_self_assign() {
    run(
        &only(rules::NoDoubleSelfAssignment, 80400),
        "RemoveDoubleSelfAssignRector/Fixture",
    );
}

#[test]
fn remove_duplicated_array_key() {
    run(
        &only(rules::NoDuplicateArrayKey, 80400),
        "RemoveDuplicatedArrayKeyRector/Fixture",
    );
}

#[test]
fn remove_final_from_const() {
    run(
        &only(rules::NoRedundantFinalConst, 80100),
        "RemoveFinalFromConstRector/Fixture",
    );
}

#[test]
fn remove_unused_foreach_key() {
    run(
        &only(rules::NoUnusedForeachKey, 80400),
        "RemoveUnusedForeachKeyRector/Fixture",
    );
}

#[test]
fn remove_useless_return_expr_in_construct() {
    run(
        &only(rules::NoReturnInConstructor, 80400),
        "RemoveUselessReturnExprInConstructRector/Fixture",
    );
}

#[test]
fn remove_unreachable_statement() {
    let linter = Linter::new(
        vec![Box::new(NoUnreachableCode)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "RemoveUnreachableStatementRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "RemoveUnreachableStatementRector/Oracle");
    run(&linter, "RemoveUnreachableStatementRector/Pun");
}

#[test]
fn remove_useless_assign_from_property_promotion() {
    let linter = Linter::new(
        vec![Box::new(NoRedundantPromotedAssign)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(
        &linter,
        "RemoveUselessAssignFromPropertyPromotionRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(
        &linter,
        "RemoveUselessAssignFromPropertyPromotionRector/Oracle",
    );
}

#[test]
fn remove_duplicated_case_in_switch() {
    let linter = Linter::new(
        vec![Box::new(NoDuplicateCaseBody)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "RemoveDuplicatedCaseInSwitchRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "RemoveDuplicatedCaseInSwitchRector/Oracle");
}

#[test]
fn simplify_if_else_with_same_content() {
    let linter = Linter::new(
        vec![Box::new(NoIdenticalBranches)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "SimplifyIfElseWithSameContentRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone). Rector reprints a
    // single-statement body its own way, so only the formatted code is compared.
    run_formatted(&linter, "SimplifyIfElseWithSameContentRector/OracleFormat");
}

#[test]
fn remove_always_else() {
    let linter = Linter::new(
        vec![Box::new(NoElseAfterReturn)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "RemoveAlwaysElseRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "RemoveAlwaysElseRector/Oracle");
    run_formatted(&linter, "RemoveAlwaysElseRector/OracleFormat");
}

fn run_formatted(linter: &Linter, dir: &str) {
    let pipeline = laravel();
    for (path, input, expected) in fixtures(dir) {
        let name = path.file_name().unwrap().to_string_lossy();
        let fixed = linter.fix(&path, &input).unwrap();
        assert_eq!(
            format(&pipeline, &fixed.source),
            format(&pipeline, &expected),
            "{name}: format(lint --fix)"
        );
        assert_eq!(fixed.applied.is_empty(), input == expected, "{name}");
    }
}

#[test]
fn change_if_else_value_assign_to_early_return() {
    let linter = Linter::new(
        vec![Box::new(PreferEarlyReturn)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(
        &linter,
        "ChangeIfElseValueAssignToEarlyReturnRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "ChangeIfElseValueAssignToEarlyReturnRector/Oracle");
}

#[test]
fn prepared_value_to_early_return() {
    let linter = Linter::new(
        vec![Box::new(PreferEarlyReturnOverPreparedValue)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "PreparedValueToEarlyReturnRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "PreparedValueToEarlyReturnRector/Oracle");
}

#[test]
fn repeated_or_equal_to_in_array() {
    let linter = Linter::new(
        vec![Box::new(PreferInArray::default())],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "RepeatedOrEqualToInArrayRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "RepeatedOrEqualToInArrayRector/Oracle");
}

#[test]
fn repeated_and_not_equal_to_not_in_array() {
    let linter = Linter::new(
        vec![Box::new(PreferNotInArray)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "RepeatedAndNotEqualToNotInArrayRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "RepeatedAndNotEqualToNotInArrayRector/Oracle");
}

#[test]
fn simplify_if_not_null_return() {
    let linter = only(rules::NoRedundantNullCheck::default(), 80400);
    run(&linter, "SimplifyIfNotNullReturnRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "SimplifyIfNotNullReturnRector/Oracle");
}

#[test]
fn simplify_conditions() {
    let linter = only(rules::PreferSimpleCondition::default(), 80400);
    run(&linter, "SimplifyConditionsRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "SimplifyConditionsRector/Oracle");
    run(&linter, "SimplifyConditionsRector/Pun");
}

#[test]
fn simplify_array_search() {
    let linter = only(rules::PreferInArrayOverSearch::default(), 80400);
    run(&linter, "SimplifyArraySearchRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "SimplifyArraySearchRector/Oracle");
}

#[test]
fn ternary_condition_variable_assignment() {
    let linter = only(rules::NoAssignInTernaryBranch, 80400);
    run(&linter, "TernaryConditionVariableAssignmentRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "TernaryConditionVariableAssignmentRector/Oracle");
    run(&linter, "TernaryConditionVariableAssignmentRector/Pun");
}

#[test]
fn singular_switch_to_if() {
    let linter = only(rules::NoSingleCaseSwitch, 80400);
    run(&linter, "SingularSwitchToIfRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "SingularSwitchToIfRector/Oracle");
    run(&linter, "SingularSwitchToIfRector/Pun");
}

#[test]
fn ternary_implode_to_implode() {
    let linter = only(rules::NoImplodeTernary, 80400);
    run(&linter, "TernaryImplodeToImplodeRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "TernaryImplodeToImplodeRector/Oracle");
}

#[test]
fn new_static_to_new_self() {
    let linter = only(rules::PreferNewSelf, 80400);
    run_with_whitespace_exceptions(&linter, "NewStaticToNewSelfRector/Fixture", &[]);
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run_with_whitespace_exceptions(&linter, "NewStaticToNewSelfRector/Oracle", &[]);
}

#[test]
fn array_merge_of_non_arrays_to_simple_array() {
    let linter = only(rules::PreferArrayLiteral, 80400);
    run(&linter, "ArrayMergeOfNonArraysToSimpleArrayRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "ArrayMergeOfNonArraysToSimpleArrayRector/Oracle");
    run(&linter, "ArrayMergeOfNonArraysToSimpleArrayRector/Pun");
}

#[test]
fn absolutize_require_and_include_path() {
    let linter = only(rules::PreferAbsoluteInclude, 80400);
    run(&linter, "AbsolutizeRequireAndIncludePathRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "AbsolutizeRequireAndIncludePathRector/Oracle");
    run(&linter, "AbsolutizeRequireAndIncludePathRector/Pun");
}

#[test]
fn flip_negated_ternary_instanceof() {
    let linter = Linter::new(
        vec![Box::new(NoNegatedInstanceofTernary)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "FlipNegatedTernaryInstanceofRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "FlipNegatedTernaryInstanceofRector/Oracle");
    run(&linter, "FlipNegatedTernaryInstanceofRector/Pun");
}

#[test]
fn dirname_dir_concat_string_to_direct_string_path() {
    let linter = Linter::new(
        vec![Box::new(NoDirnameDirConcat)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(
        &linter,
        "DirnameDirConcatStringToDirectStringPathRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(
        &linter,
        "DirnameDirConcatStringToDirectStringPathRector/Oracle",
    );
}

#[test]
fn array_explicit_bool_compare() {
    let linter = Linter::new(
        vec![Box::new(PreferExplicitArrayComparison)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "ArrayExplicitBoolCompareRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "ArrayExplicitBoolCompareRector/Oracle");
    run(&linter, "ArrayExplicitBoolCompareRector/Pun");
}

#[test]
fn inline_if_to_explicit_if() {
    let linter = Linter::new(
        vec![Box::new(PreferIfStatement)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "InlineIfToExplicitIfRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "InlineIfToExplicitIfRector/Oracle");
    run(&linter, "InlineIfToExplicitIfRector/Pun");
}

#[test]
fn remove_overridden_assign_before_if_else() {
    let linter = Linter::new(
        vec![Box::new(NoOverriddenAssignBeforeIf)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "RemoveOverriddenAssignBeforeIfElseRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "RemoveOverriddenAssignBeforeIfElseRector/Oracle");
}

#[test]
fn remove_redundant_type_check() {
    let linter = Linter::new(
        vec![Box::new(NoRedundantTypeCheck)],
        pun_lint::PhpVersion::from_id(80400),
    );
    run(&linter, "RemoveRedundantTypeCheckRector/Fixture");
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone).
    run(&linter, "RemoveRedundantTypeCheckRector/Oracle");
}
