//! Rector 2.9.2's own fixtures for its configured rules in no set, copied verbatim from rector-src's rules-tests at
//! 2.9.2 into tests/rector/<Rector class>/Fixture (and its Source/), each run with the configuration of its
//! config/configured_rule.php. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the rule
//! alone with the configuration below, `phpVersion(80400)`) wrote for them; the classes they name were on
//! `withAutoloadPaths` (WrapReturnRector/Source/OracleLib.php). A fixture is `input -----\n expected`, or only the
//! input when the rule must not change it.
//!
//! Pun/ holds the documented cases where Pun differs. StaticCallToNewRector/Pun/multiline_args: Pun keeps a multiline
//! argument list as written, where Rector reprints it on one line. RemoveInterfacesRector/Pun/anonymous_class: Pun
//! removes only the `implements` clause of an anonymous class, where Rector reprints the class and moves its `{` to a
//! line of its own.

use crate::common::run_class;
use pun_lint::Rule;
use pun_lint::rules::{
    ChangeMethodVisibility, RemoveInterfaces, RenameAttribute, RenameString, StaticCallToNew,
    WrapReturn,
};
use serde_json::{Value, json};

fn check(mut rule: impl Rule + 'static, options: &Value, class: &str) {
    rule.configure(options).unwrap();
    run_class(vec![Box::new(rule)], class);
}

fn rejects(mut rule: impl Rule, bad: &[(Value, &str)]) {
    for (options, error) in bad {
        assert_eq!(
            rule.configure(options).unwrap_err().to_string(),
            *error,
            "{options}"
        );
    }
}

#[test]
fn rename_string() {
    check(
        RenameString::default(),
        &json!({"ROLE_PREVIOUS_ADMIN": "IS_IMPERSONATOR", "OLD_\\": "it's\\"}),
        "RenameStringRector",
    );
    rejects(
        RenameString::default(),
        &[(json!({"a": 1}), "`a` must be a string")],
    );
}

#[test]
fn static_call_to_new() {
    let source = "Rector\\Tests\\Transform\\Rector\\StaticCall\\StaticCallToNewRector\\Source\\";
    check(
        StaticCallToNew::default(),
        &json!({"items": [
            {"class": format!("{source}SomeJsonResponse"), "method": "create"},
            {"class": "Lib\\Response", "method": "create"},
        ]}),
        "StaticCallToNewRector",
    );
    rejects(
        StaticCallToNew::default(),
        &[
            (
                json!({"items": [{"class": "A"}]}),
                "each of `items` needs `method`",
            ),
            (
                json!({"items": [{"class": "A", "method": "a-b"}]}),
                "\"a-b\" is not a valid method name",
            ),
        ],
    );
}

#[test]
fn remove_interfaces() {
    check(
        RemoveInterfaces::default(),
        &json!({"items": [
            "Rector\\Tests\\Removing\\Rector\\Class_\\RemoveInterfacesRector\\Source\\SomeInterface",
            "Lib\\Gone",
            "Lib\\Gone2",
        ]}),
        "RemoveInterfacesRector",
    );
    rejects(
        RemoveInterfaces::default(),
        &[(json!({"items": [1]}), "each of `items` must be a string")],
    );
}

#[test]
fn rename_attribute() {
    let source = "Rector\\Tests\\Renaming\\Rector\\Class_\\RenameAttributeRector\\Source\\";
    let rename = |old: String, new: String| json!({"old-attribute": old, "new-attribute": new});
    check(
        RenameAttribute::default(),
        &json!({"items": [
            rename(format!("{source}SimpleRoute"), format!("{source}NextRoute")),
            rename(format!("{source}SimpleParamAttribute"), format!("{source}NextParamAttribute")),
            rename("Lib\\OldAttr".into(), "Lib\\NewAttr".into()),
        ]}),
        "RenameAttributeRector",
    );
    rejects(
        RenameAttribute::default(),
        &[(
            json!({"items": [{"old-attribute": "A", "new": "B"}]}),
            "unknown key `new` in `items`; the keys are old-attribute, new-attribute",
        )],
    );
}

#[test]
fn change_method_visibility() {
    let parent = "Rector\\Tests\\Visibility\\Rector\\Class_\\ChangeMethodVisibilityRector\\Source\\ParentObject";
    let change = |class: &str, method: &str, visibility: i64| json!({"class": class, "method": method, "visibility": visibility});
    check(
        ChangeMethodVisibility::default(),
        &json!({"items": [
            change(parent, "toBePublicMethod", 1),
            change(parent, "toBeProtectedMethod", 2),
            change(parent, "toBePrivateMethod", 4),
            change(parent, "toBePublicStaticMethod", 1),
            change("Lib\\ParentObject", "makePrivate", 4),
            change("Lib\\ParentObject", "makePublic", 1),
            change("Lib\\ParentObject", "makeStatic", 8),
            change("Lib\\ParentObject", "makePublic2", 1),
        ]}),
        "ChangeMethodVisibilityRector",
    );
    rejects(
        ChangeMethodVisibility::default(),
        &[
            (
                json!({"items": [change("A", "a", 64)]}),
                "`visibility` must be one of 1, 2, 4, 8, 16 and 32 (Rector's Visibility constants)",
            ),
            (
                json!({"items": [{"class": "A", "method": "a", "visibility": "public"}]}),
                "`visibility` must be an integer",
            ),
        ],
    );
}

#[test]
fn wrap_return() {
    let wrap =
        |class: &str, method: &str| json!({"type": class, "method": method, "is-array-wrap": true});
    check(
        WrapReturn::default(),
        &json!({"items": [
            wrap("Rector\\Tests\\Transform\\Rector\\ClassMethod\\WrapReturnRector\\Source\\SomeReturnClass", "getItem"),
            wrap("Lib\\Base", "getItem"),
            wrap("Lib\\Base", "getItem2"),
            {"type": "Lib\\Base", "method": "other", "is-array-wrap": false},
        ]}),
        "WrapReturnRector",
    );
    rejects(
        WrapReturn::default(),
        &[(
            json!({"items": [{"type": "A", "method": "a", "is-array-wrap": 1}]}),
            "`is-array-wrap` must be a boolean",
        )],
    );
}
