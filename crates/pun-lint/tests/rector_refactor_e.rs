//! Rector 2.9.2's own fixtures for its configured rules in no set that edit a method's arguments, a property's name
//! or an attribute's value, copied verbatim from rector-src's rules-tests at 2.9.2 into
//! tests/rector/<Rector class>/Fixture (and its Source/), each run with the configuration of its
//! config/configured_rule.php. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the rule
//! alone with the configuration below, `phpVersion(80400)`, Source/ on `withAutoloadPaths`) wrote for them. A fixture
//! is `input -----\n expected`, or only the input when the rule must not change it.
//!
//! Pun/ holds the documented cases where Pun differs. ArgumentRemoverRector/Pun/unknown_value: Rector's
//! `ValueResolver::getValue()` reads an expression it cannot evaluate (`$x`, `strtoupper($x)`) as `null`, so a
//! configured `null` value removes any such argument; Pun removes only arguments whose value it knows.

use crate::common::run_class;
use pun_lint::Rule;
use pun_lint::rules::{RemoveMethodArgument, RenameProperty, ReplaceAttributeValueWithConstant};
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
fn argument_remover() {
    let source = "Rector\\Tests\\Removing\\Rector\\ClassMethod\\ArgumentRemoverRector\\Source\\";
    check(
        RemoveMethodArgument::default(),
        &json!({"items": [
            {"class": format!("{source}Persister"), "method": "getSelectJoinColumnSQL", "position": 4},
            {"class": format!("{source}Yaml"), "method": "parse", "position": 1,
             "value": ["Symfony\\Component\\Yaml\\Yaml::PARSE_KEYS_AS_STRINGS", "hey", 55, 5.5]},
            {"class": format!("{source}RemoveInTheMiddle"), "method": "run", "position": 1,
             "value": {"name": "second"}},
            {"class": "App\\Lib\\Client", "method": "send", "position": 1},
            {"class": "App\\Lib\\Client", "method": "flag", "position": 0, "value": [true, "yes", 3, null]},
            {"class": "App\\Lib\\Client", "method": "byName", "position": 0, "value": {"name": "first"}},
        ]}),
        "ArgumentRemoverRector",
    );
    rejects(
        RemoveMethodArgument::default(),
        &[
            (
                json!({"items": [{"class": "A", "method": "m", "position": -1}]}),
                "`position` must not be negative",
            ),
            (
                json!({"items": [{"class": "A", "method": "m", "position": 0, "value": 1}]}),
                "`value` must be a list of values or a table with `name`",
            ),
            (
                json!({"items": [{"class": "A", "method": "m", "position": 0, "value": {"other": 1}}]}),
                "a table `value` holds one key, `name`, a string",
            ),
        ],
    );
}

#[test]
fn rename_property() {
    let fixture = "Rector\\Tests\\Renaming\\Rector\\PropertyFetch\\RenamePropertyRector\\";
    let rename = |class: String, old: &str, new: &str| json!({"type": class, "old-property": old, "new-property": new});
    check(
        RenameProperty::default(),
        &json!({"items": [
            rename(format!("{fixture}Source\\ClassWithProperties"), "oldProperty", "newProperty"),
            rename(format!("{fixture}Source\\ClassWithProperties"), "anotherOldProperty", "anotherNewProperty"),
            rename(format!("{fixture}Fixture\\ClassWithOldProperty"), "oldProperty", "newProperty"),
            rename(format!("{fixture}Fixture\\ClassWithOldProperty2"), "oldProperty", "newProperty"),
            rename(format!("{fixture}Fixture\\DoNotChangeToPropertyExists"), "oldProperty", "newProperty"),
            rename(format!("{fixture}Source\\ParentClassWithOldProperty"), "oldProperty", "newProperty"),
            rename(format!("{fixture}Fixture\\MyClass"), "_config", "config"),
            rename(format!("{fixture}Fixture\\InstanceConfigTrait"), "defaultConfig", "defaultConfigNew"),
            rename("App\\Props\\Model".into(), "oldName", "newName"),
            rename("App\\Props\\Model".into(), "count", "total"),
            rename("App\\Props\\Model".into(), "kept", "taken"),
        ]}),
        "RenamePropertyRector",
    );
    rejects(
        RenameProperty::default(),
        &[(
            json!({"items": [{"type": "A", "old-property": "a"}]}),
            "each of `items` needs `new-property`",
        )],
    );
}

#[test]
fn attribute_key_to_class_const_fetch() {
    let source = "Rector\\Tests\\Transform\\Rector\\Attribute\\AttributeKeyToClassConstFetchRector\\Source\\";
    check(
        ReplaceAttributeValueWithConstant::default(),
        &json!({"items": [
            {"attribute-class": "Doctrine\\ORM\\Mapping\\Column", "attribute-key": "type",
             "constant-class": "Doctrine\\DBAL\\Types\\Types", "values-to-constants-map": {"string": "STRING"}},
            {"attribute-class": format!("{source}TestAttribute"), "attribute-key": "type",
             "constant-class": format!("{source}Constant"), "values-to-constants-map": {"value": "VALUE"}},
            {"attribute-class": "App\\Attr\\Column", "attribute-key": "type", "constant-class": "App\\Attr\\Types",
             "values-to-constants-map": {"string": "STRING", "5": "FIVE", "App\\Attr\\Types::TEXT": "TEXT_TYPE"}},
        ]}),
        "AttributeKeyToClassConstFetchRector",
    );
    rejects(
        ReplaceAttributeValueWithConstant::default(),
        &[(
            json!({"items": [{"attribute-class": "A", "attribute-key": "k", "constant-class": "C",
                              "values-to-constants-map": {"a": 1}}]}),
            "`values-to-constants-map.a` must be a string",
        )],
    );
}
