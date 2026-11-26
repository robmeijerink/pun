//! Rector 2.9.2's own fixtures for its rector-preset rules that edit Rector rule classes, copied verbatim from
//! rector-src's rules-tests at 2.9.2 (DeadCode/Rector/Class_/..., TypeDeclarationDocblocks/Rector/Class_/...) into
//! tests/rector/<Rector class>/Fixture, with the php-parser and Rector classes they name declared in Stubs/. Oracle/
//! holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the rule alone, `phpVersion(80400)`, and
//! `importNames()` for AddParamTypeToRefactorMethodRector as its config/configured_rule.php has) wrote for them. A
//! fixture is `input -----\n expected`, or only the input when the rule must not change it.

use crate::common::run_class;
use pun_lint::rules::{AddRefactorParamPhpdoc, NoRedundantNodeInstanceof};

#[test]
fn remove_refactor_duplicated_node_instance_check() {
    run_class(
        vec![Box::new(NoRedundantNodeInstanceof)],
        "RemoveRefactorDuplicatedNodeInstanceCheckRector",
    );
}

#[test]
fn add_param_type_to_refactor_method() {
    run_class(
        vec![Box::new(AddRefactorParamPhpdoc)],
        "AddParamTypeToRefactorMethodRector",
    );
}
