//! spaze/phpstan-disallowed-calls 4.15.1's rules: the `analyse/disallowed/*` rules, which run with
//! `[extensions] disallowed-calls = "on"` (`pun_config::extensions::DISALLOWED_CALLS`, which owns the `disallowed`
//! group). Each rule reads one or two of the package's parameters (`disallowedFunctionCalls`, ...), which pun.toml
//! sets as the rule's options.
//!
//! The config entries are read here (upstream's `Disallowed*Factory`, `AllowedConfigFactory` and `Normalizer`), the
//! allow directives are checked in `allowed.rs` (`Allowed\Allowed`), and the rules walk the file in `calls.rs`,
//! `usages.rs` and `keywords.rs`. An entry's `errorIdentifier` goes into `Project::extension_identifiers`, so
//! `Checks::keep_reported` keeps the errors that carry it.

mod allowed;
mod calls;
mod keywords;
mod usages;

use std::collections::HashSet;

use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;
use mago_syntax::walker::MutWalker;
use pun_phpstan::ignore::Error;
use pun_phpstan::types::Type;
use pun_phpstan::{Array, Key, Value};
use pun_tokens::php_compat;

use super::Checker;

/// `src/RuleErrors/ErrorIdentifiers.php`.
pub(crate) const IDENTIFIERS: [&str; 41] = [
    "disallowed.attribute",
    "disallowed.backtick",
    "disallowed.break",
    "disallowed.class",
    "disallowed.classConstant",
    "disallowed.constant",
    "disallowed.continue",
    "disallowed.declare",
    "disallowed.die",
    "disallowed.doWhile",
    "disallowed.echo",
    "disallowed.else",
    "disallowed.elseIf",
    "disallowed.empty",
    "disallowed.enum",
    "disallowed.eval",
    "disallowed.exit",
    "disallowed.for",
    "disallowed.foreach",
    "disallowed.function",
    "disallowed.global",
    "disallowed.goto",
    "disallowed.if",
    "disallowed.include",
    "disallowed.includeOnce",
    "disallowed.interface",
    "disallowed.isset",
    "disallowed.match",
    "disallowed.method",
    "disallowed.namespace",
    "disallowed.new",
    "disallowed.print",
    "disallowed.property",
    "disallowed.require",
    "disallowed.requireOnce",
    "disallowed.return",
    "disallowed.switch",
    "disallowed.trait",
    "disallowed.unset",
    "disallowed.variable",
    "disallowed.while",
];

/// The package's parameters, as entries (`extension.neon`'s `parameters`).
#[derive(Clone, Default)]
pub struct Disallowed {
    functions: Vec<Entry>,
    methods: Vec<Entry>,
    statics: Vec<Entry>,
    namespaces: Vec<Entry>,
    classes: Vec<Entry>,
    constants: Vec<Entry>,
    enums: Vec<Entry>,
    superglobals: Vec<Entry>,
    attributes: Vec<Entry>,
    structures: Vec<Entry>,
    keywords: Vec<Entry>,
    properties: Vec<Entry>,
    /// `methods` then `statics`, which callable arguments are checked against.
    callables: Vec<Entry>,
    /// `filesRootDir`, absolute: the directory `allowIn` and `definedIn` paths are relative to.
    root: Option<String>,
    cwd: String,
}

#[derive(Clone)]
struct Entry {
    name: String,
    excludes: Vec<String>,
    exclude_with_attributes: Vec<String>,
    defined_in: Vec<String>,
    message: Option<String>,
    allowed: AllowedConfig,
    allow_in_use: bool,
    identifier: Option<String>,
    tips: Vec<String>,
    /// A keyword's `control structure` or `keyword`.
    description: &'static str,
}

#[derive(Clone, Default)]
struct AllowedConfig {
    allow_in: Vec<String>,
    allow_except_in: Vec<String>,
    allow_in_calls: Vec<String>,
    allow_except_in_calls: Vec<String>,
    allow_in_instance_of: Vec<String>,
    allow_except_in_instance_of: Vec<String>,
    allow_in_class_with_attributes: Vec<String>,
    allow_except_in_class_with_attributes: Vec<String>,
    allow_in_calls_with_attributes: Vec<String>,
    allow_except_in_calls_with_attributes: Vec<String>,
    allow_in_class_with_method_attributes: Vec<String>,
    allow_except_in_class_with_method_attributes: Vec<String>,
    params_in_allowed: Vec<(Key, Param)>,
    params_anywhere: Vec<(Key, Param)>,
    except_params_in_allowed: Vec<(Key, Param)>,
    except_params: Vec<(Key, Param)>,
    in_param_types: bool,
    except_in_param_types: bool,
    in_return_type: bool,
    except_in_return_type: bool,
}

#[derive(Clone, PartialEq, Debug)]
struct Param {
    position: Option<i64>,
    name: Option<String>,
    kind: ParamKind,
}

#[derive(Clone, PartialEq, Debug)]
enum ParamKind {
    Specific(Type),
    Except(Type),
    Any,
    ExceptAny,
    FlagSpecific(Type),
    FlagExcept(Type),
    CaseInsensitiveExcept(Type),
    ClassPattern(String),
    ClassPatternExcept(String),
}

/// `UsagePosition`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    Param,
    Return,
}

const SUPERGLOBALS: [&str; 9] = [
    "$GLOBALS",
    "$_SERVER",
    "$_GET",
    "$_POST",
    "$_FILES",
    "$_COOKIE",
    "$_SESSION",
    "$_REQUEST",
    "$_ENV",
];

/// `DisallowedKeywordFactory::KEYWORDS`.
const KEYWORDS: [&str; 19] = [
    "if",
    "else",
    "elseif",
    "while",
    "do-while",
    "for",
    "foreach",
    "break",
    "continue",
    "switch",
    "match",
    "declare",
    "return",
    "require",
    "include",
    "require_once",
    "include_once",
    "goto",
    "global",
];

impl Disallowed {
    /// The entries of the package's parameters; `cwd` is PHPStan's current working directory, which relative paths
    /// resolve against without `filesRootDir`. Upstream throws on an entry without its name or with an unknown
    /// superglobal or keyword; that entry is left out here.
    pub(crate) fn from_params(params: &Array, cwd: &str) -> Disallowed {
        let list = |k: &str| -> Vec<&Array> {
            params
                .get_str(k)
                .and_then(Value::as_array)
                .map(|a| a.values().filter_map(Value::as_array).collect())
                .unwrap_or_default()
        };
        let calls = |k: &str| calls(&list(k));
        let root = (params.get_str("filesRootDir").and_then(Value::as_str))
            .map(|r| pun_phpstan::php::normalize_path(&pun_phpstan::php::absolutize_path(cwd, r)));
        let methods = calls("disallowedMethodCalls");
        let statics = calls("disallowedStaticCalls");
        let callables =
            self::calls(&[list("disallowedMethodCalls"), list("disallowedStaticCalls")].concat());
        Disallowed {
            functions: calls("disallowedFunctionCalls"),
            methods,
            statics,
            callables,
            namespaces: namespaces(&list("disallowedNamespaces")),
            classes: namespaces(&list("disallowedClasses")),
            constants: constants(&list("disallowedConstants")),
            enums: constants(&list("disallowedEnums")),
            superglobals: entries(&list("disallowedSuperglobals"), &["superglobal"], |e| {
                SUPERGLOBALS.contains(&e.as_str()).then(|| e.clone())
            }),
            attributes: entries(&list("disallowedAttributes"), &["attribute"], |a| {
                Some(normalize_attribute(a))
            }),
            structures: keywords(&list("disallowedControlStructures")),
            keywords: keywords(&list("disallowedKeywords")),
            properties: entries(&list("disallowedProperties"), &["property"], |p| {
                normalize_property(p)
            }),
            root,
            cwd: cwd.to_string(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.all().all(<[Entry]>::is_empty)
    }

    fn all(&self) -> impl Iterator<Item = &[Entry]> {
        [
            &self.functions,
            &self.methods,
            &self.statics,
            &self.namespaces,
            &self.classes,
            &self.constants,
            &self.enums,
            &self.superglobals,
            &self.attributes,
            &self.structures,
            &self.keywords,
            &self.properties,
        ]
        .into_iter()
        .map(Vec::as_slice)
    }

    /// Every entry's `errorIdentifier`.
    pub(crate) fn identifiers(&self) -> HashSet<String> {
        (self.all().flatten())
            .filter_map(|e| e.identifier.clone())
            .collect()
    }

    /// `FilePath::fnMatch`: `path` (relative to `filesRootDir`) matches `file`.
    fn path_matches(&self, path: &str, file: &str) -> bool {
        let absolute = |p: &str, root: Option<&str>| {
            if p.starts_with('*') {
                return p.to_string();
            }
            let p = match root {
                Some(r) => format!("{}/{}", r.trim_end_matches('/'), p.trim_start_matches('/')),
                None => p.to_string(),
            };
            pun_phpstan::php::normalize_path(&pun_phpstan::php::absolutize_path(&self.cwd, &p))
        };
        php_compat::fnmatch_flags(
            absolute(path, self.root.as_deref()).as_bytes(),
            absolute(file, None).as_bytes(),
            true,
            false,
        )
    }
}

fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Str(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a
            .values()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn first<'v>(e: &'v Array, keys: &[&str]) -> Option<&'v Value> {
    keys.iter()
        .find_map(|k| e.get_str(k).filter(|v| **v != Value::Null))
}

fn bool_of(e: &Array, keys: &[&str]) -> bool {
    first(e, keys) == Some(&Value::Bool(true))
}

/// `Normalizer::normalizeNamespace`.
fn normalize(s: &str) -> String {
    s.trim_start_matches('\\').to_string()
}

/// `Normalizer::normalizeCall`.
fn normalize_call(s: &str) -> String {
    normalize(s.strip_suffix("()").unwrap_or(s))
}

/// `Normalizer::normalizeAttribute`.
fn normalize_attribute(s: &str) -> String {
    normalize_call(s.trim_start_matches(['#', '[']).trim_end_matches(']'))
}

/// `Normalizer::normalizeProperty`; upstream throws on a property without one `::`.
fn normalize_property(s: &str) -> Option<String> {
    if s.matches("::").count() != 1 {
        return None;
    }
    Some(normalize(&if s.contains("::$") {
        s.to_string()
    } else {
        s.replace("::", "::$")
    }))
}

/// The entries of one list, one per name, keyed as the factories key them (a later entry with the same key replaces
/// the earlier one in its place).
fn entries(
    config: &[&Array],
    names: &[&str],
    name: impl Fn(&String) -> Option<String>,
) -> Vec<Entry> {
    let mut out: Vec<(String, Entry)> = Vec::new();
    for e in config {
        let excludes = strings(e.get_str("exclude"));
        for n in strings(first(e, names)) {
            let Some(n) = name(&n) else { continue };
            let Some(allowed) = allowed_config(e) else {
                continue;
            };
            let entry = Entry {
                name: n,
                excludes: excludes.clone(),
                exclude_with_attributes: strings(e.get_str("excludeWithAttribute"))
                    .iter()
                    .map(|s| normalize(s))
                    .collect(),
                defined_in: strings(e.get_str("definedIn")),
                message: e
                    .get_str("message")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                allowed,
                allow_in_use: bool_of(e, &["allowInUse"]),
                identifier: (e.get_str("errorIdentifier").and_then(Value::as_str))
                    .map(str::to_string),
                tips: strings(e.get_str("errorTip")),
                description: "",
            };
            let key = entry.name.clone();
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = entry,
                None => out.push((key, entry)),
            }
        }
    }
    out.into_iter().map(|(_, e)| e).collect()
}

/// `DisallowedCallFactory`: keyed by the call and its `allowExceptParams`.
fn calls(config: &[&Array]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for e in config {
        for mut entry in entries(&[e], &["function", "method"], |c| Some(normalize_call(c))) {
            entry.excludes = entry.excludes.iter().map(|x| normalize_call(x)).collect();
            match (out.iter_mut()).find(|o| {
                o.name == entry.name && o.allowed.except_params == entry.allowed.except_params
            }) {
                Some(slot) => *slot = entry,
                None => out.push(entry),
            }
        }
    }
    out
}

/// `DisallowedNamespaceFactory`.
fn namespaces(config: &[&Array]) -> Vec<Entry> {
    let mut out = entries(config, &["namespace", "class"], |n| Some(normalize(n)));
    for e in &mut out {
        e.excludes = e.excludes.iter().map(|x| normalize(x)).collect();
    }
    out
}

/// `DisallowedConstantFactory`: `class::constant`, or an enum's `enum::case`.
fn constants(config: &[&Array]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for e in config {
        let class = first(e, &["class", "enum"]).and_then(Value::as_str);
        for entry in entries(&[e], &["constant", "case"], |c| {
            Some(normalize(
                &class.map_or_else(|| c.clone(), |k| format!("{k}::{c}")),
            ))
        }) {
            match out.iter_mut().find(|o| o.name == entry.name) {
                Some(slot) => *slot = entry,
                None => out.push(entry),
            }
        }
    }
    out
}

/// `DisallowedKeywordFactory`.
fn keywords(config: &[&Array]) -> Vec<Entry> {
    let mut out = entries(config, &["controlStructure", "structure", "keyword"], |k| {
        KEYWORDS.contains(&k.as_str()).then(|| k.clone())
    });
    for e in &mut out {
        e.description = if e.name == "global" {
            "keyword"
        } else {
            "control structure"
        };
    }
    out
}

/// `AllowedConfigFactory::getConfig`; `None` where upstream throws on the entry's params.
fn allowed_config(e: &Array) -> Option<AllowedConfig> {
    let list = |keys: &[&str], f: fn(&str) -> String| -> Vec<String> {
        strings(first(e, keys)).iter().map(|s| f(s)).collect()
    };
    let plain = |s: &str| s.to_string();
    let mut c = AllowedConfig {
        allow_in: list(&["allowIn"], plain),
        allow_except_in: list(&["allowExceptIn", "disallowIn"], plain),
        allow_in_calls: list(&["allowInFunctions", "allowInMethods"], normalize_call),
        allow_except_in_calls: list(
            &[
                "allowExceptInFunctions",
                "allowExceptInMethods",
                "disallowInFunctions",
                "disallowInMethods",
            ],
            normalize_call,
        ),
        allow_in_instance_of: list(&["allowInInstanceOf"], normalize),
        allow_except_in_instance_of: list(
            &["allowExceptInInstanceOf", "disallowInInstanceOf"],
            normalize,
        ),
        allow_in_class_with_attributes: list(&["allowInClassWithAttributes"], normalize_attribute),
        allow_except_in_class_with_attributes: list(
            &[
                "allowExceptInClassWithAttributes",
                "disallowInClassWithAttributes",
            ],
            normalize_attribute,
        ),
        allow_in_calls_with_attributes: list(
            &[
                "allowInFunctionsWithAttributes",
                "allowInMethodsWithAttributes",
            ],
            normalize_attribute,
        ),
        allow_except_in_calls_with_attributes: list(
            &[
                "allowExceptInFunctionsWithAttributes",
                "allowExceptInMethodsWithAttributes",
                "disallowInFunctionsWithAttributes",
                "disallowInMethodsWithAttributes",
            ],
            normalize_attribute,
        ),
        allow_in_class_with_method_attributes: list(
            &["allowInClassWithMethodAttributes"],
            normalize_attribute,
        ),
        allow_except_in_class_with_method_attributes: list(
            &[
                "allowExceptInClassWithMethodAttributes",
                "disallowInClassWithMethodAttributes",
            ],
            normalize_attribute,
        ),
        in_param_types: bool_of(e, &["allowInParamTypes"]),
        except_in_param_types: bool_of(e, &["allowExceptInParamTypes", "disallowInParamTypes"]),
        in_return_type: bool_of(e, &["allowInReturnType"]),
        except_in_return_type: bool_of(e, &["allowExceptInReturnType", "disallowInReturnType"]),
        ..AllowedConfig::default()
    };
    use ParamKind as K;
    type Make = fn(Type) -> ParamKind;
    let groups: [(&[&str], Make, bool, u8); 12] = [
        (&["allowParamsInAllowed"], K::Specific, false, 0),
        (&["allowParamsInAllowedAnyValue"], |_| K::Any, false, 0),
        (&["allowParamFlagsInAllowed"], K::FlagSpecific, false, 0),
        (&["allowParamsAnywhere"], K::Specific, false, 1),
        (&["allowParamsAnywhereAnyValue"], |_| K::Any, false, 1),
        (&["allowParamFlagsAnywhere"], K::FlagSpecific, false, 1),
        (
            &["allowExceptParamsInAllowed", "disallowParamsInAllowed"],
            K::Except,
            true,
            2,
        ),
        (
            &[
                "allowExceptParamFlagsInAllowed",
                "disallowParamFlagsInAllowed",
            ],
            K::FlagExcept,
            true,
            2,
        ),
        (
            &[
                "allowExceptParamsAnywhere",
                "disallowParamsAnywhere",
                "allowExceptParams",
                "disallowParams",
            ],
            K::Except,
            true,
            3,
        ),
        (
            &["allowExceptParamsAnyValue", "disallowParamsAnyValue"],
            |_| K::ExceptAny,
            true,
            3,
        ),
        (
            &["allowExceptParamFlags", "disallowParamFlags"],
            K::FlagExcept,
            true,
            3,
        ),
        (
            &[
                "allowExceptCaseInsensitiveParams",
                "disallowCaseInsensitiveParams",
            ],
            K::CaseInsensitiveExcept,
            true,
            3,
        ),
    ];
    for (keys, make, except, target) in groups {
        let Some(Value::Array(params)) = first(e, keys) else {
            continue;
        };
        let any = matches!(make(Type::Null), K::Any | K::ExceptAny);
        let flags = matches!(make(Type::Null), K::FlagSpecific(_) | K::FlagExcept(_));
        for (key, value) in params.iter() {
            let param = param_without_type(key, value, except)
                .or_else(|| param(key, value, make, any, flags))?;
            let list = match target {
                0 => &mut c.params_in_allowed,
                1 => &mut c.params_anywhere,
                2 => &mut c.except_params_in_allowed,
                _ => &mut c.except_params,
            };
            match list.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = param,
                None => list.push((key.clone(), param)),
            }
        }
    }
    Some(c)
}

fn numeric(key: &Key) -> Option<i64> {
    match key {
        Key::Int(i) => Some(*i),
        Key::Str(_) => None,
    }
}

/// A param table's `position`, which upstream's schema requires to be an integer (pun.toml's options skip the
/// schema; the entry is left out instead).
fn position(v: &Array) -> Option<i64> {
    match v.get_str("position") {
        Some(Value::Int(i)) => Some(*i),
        _ => None,
    }
}

/// `AllowedConfigFactory::paramWithoutTypeFactory`: a `classPattern` param.
fn param_without_type(key: &Key, value: &Value, except: bool) -> Option<Param> {
    numeric(key)?;
    let v = value.as_array()?;
    let pattern = normalize(v.get_str("classPattern")?.as_str()?);
    Some(Param {
        position: Some(position(v)?),
        name: v
            .get_str("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        kind: if except {
            ParamKind::ClassPatternExcept(pattern)
        } else {
            ParamKind::ClassPattern(pattern)
        },
    })
}

/// `AllowedConfigFactory::paramFactory`; `None` where it throws.
fn param(
    key: &Key,
    value: &Value,
    make: fn(Type) -> ParamKind,
    any: bool,
    flags: bool,
) -> Option<Param> {
    let (position, name, value, type_string) = match (numeric(key), value) {
        (Some(_), Value::Array(v)) => (
            Some(position(v)?),
            v.get_str("name")
                .and_then(Value::as_str)
                .map(str::to_string),
            v.get_str("value").cloned().unwrap_or(Value::Null),
            v.get_str("typeString").cloned(),
        ),
        (Some(_), Value::Int(i)) if any => (Some(*i), None, Value::Null, None),
        (Some(_), Value::Str(s)) if any => (None, Some(s.clone()), Value::Null, None),
        (Some(k), v) => (Some(k), None, v.clone(), None),
        (None, v) => (None, Some(key.as_string()), v.clone(), None),
    };
    let t = match (type_string, value) {
        (Some(Value::Str(s)), _) if !s.is_empty() => {
            pun_phpstan::resolve::resolve(&pun_phpstan::phpdoc::parse(s.as_bytes()).ok()?, None)
        }
        (Some(_), _) => return None,
        (None, Value::Int(i)) => Type::ConstInt(i),
        (None, Value::Bool(b)) => Type::ConstBool(b),
        (None, Value::Str(s)) => Type::ConstString(s.into_bytes()),
        (None, Value::Null) => Type::Null,
        _ => return None,
    };
    if flags
        && !constant_scalars(&t)
            .iter()
            .all(|v| matches!(v, Type::ConstInt(_)))
    {
        return None;
    }
    Some(Param {
        position,
        name,
        kind: make(t),
    })
}

/// `Type::getConstantScalarValues`, as types.
fn constant_scalars(t: &Type) -> Vec<Type> {
    match t {
        Type::Union(types) => types.iter().flat_map(constant_scalars).collect(),
        Type::ConstInt(_)
        | Type::ConstBool(_)
        | Type::ConstString(_)
        | Type::ConstFloat(_)
        | Type::Null => {
            vec![t.clone()]
        }
        _ => Vec::new(),
    }
}

/// `Identifier::matches` without `excludeWithAttribute`.
fn matches(pattern: &str, value: &str, excludes: &[String]) -> bool {
    let fnmatch = |p: &str| php_compat::fnmatch_flags(p.as_bytes(), value.as_bytes(), true, true);
    (pattern == value || fnmatch(pattern)) && !excludes.iter().any(|x| fnmatch(x))
}

/// `Formatter::formatDisallowedMessage`.
fn format_message(message: Option<&str>) -> String {
    match message {
        None | Some("") => ".".to_string(),
        Some(m) if m.ends_with(['?', '!']) => format!(", {m}"),
        Some(m) => format!(", {}.", m.trim_end_matches('.')),
    }
}

/// `Formatter::formatIdentifier`.
fn format_identifier(names: &[String]) -> String {
    match names {
        [one] => normalize(one),
        many => format!(
            "{{{}}}",
            many.iter()
                .map(|n| normalize(n))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

/// An argument a param condition reads: its name when named, and its value.
struct Arg<'e, 'arena> {
    name: Option<&'arena [u8]>,
    value: &'e Expression<'arena>,
}

fn args<'e, 'arena>(list: &'e ArgumentList<'arena>) -> Vec<Arg<'e, 'arena>> {
    (list.arguments.iter())
        .map(|a| match a {
            Argument::Positional(p) => Arg {
                name: None,
                value: p.value,
            },
            Argument::Named(n) => Arg {
                name: Some(n.name.value),
                value: n.value,
            },
        })
        .collect()
}

fn partial_args<'e, 'arena>(list: &'e PartialArgumentList<'arena>) -> Vec<Arg<'e, 'arena>> {
    (list.arguments.iter())
        .filter_map(|a| {
            Some(Arg {
                name: match a {
                    PartialArgument::Named(n) => Some(n.name.value),
                    _ => None,
                },
                value: a.value()?,
            })
        })
        .collect()
}

/// What `Allowed::isAllowed` reads from the node besides the scope: the function a declaration's own checks run for
/// (a `ClassMethod` or `Function_` node), or the attributes of the function a type hint is in
/// (`TypeHintContextVisitor`).
#[derive(Default, Clone)]
struct Site {
    function: Option<String>,
    attributes: Option<Vec<String>>,
}

struct ClassScope<'a> {
    meta: Option<&'a ClassLikeMetadata>,
}

struct FunctionScope {
    /// `Allowed::callMatches`'s name: `Class::method` or the function's full name.
    name: String,
    attributes: Vec<String>,
}

/// The rules' walk over one file.
pub(super) struct Walker<'c, 'a, 'arena> {
    c: &'c Checker<'a, 'arena>,
    d: &'a Disallowed,
    namespace: Option<String>,
    classes: Vec<ClassScope<'a>>,
    functions: Vec<FunctionScope>,
    /// The `$name` of each `Class::$name`, which is no variable.
    static_properties: HashSet<Span>,
    /// The name (`$name`) of the property whose hooks are walked.
    property: Option<String>,
    errors: Vec<Error>,
}

/// Runs the rules on the file the checker walked, in its trait context if it has one.
pub(super) fn check<'arena>(c: &mut Checker<'_, 'arena>, program: &Program<'arena>) {
    let Some(d) = c.disallowed else {
        return;
    };
    let mut w = Walker {
        c,
        d,
        namespace: None,
        classes: Vec::new(),
        functions: Vec::new(),
        static_properties: HashSet::new(),
        property: None,
        errors: Vec::new(),
    };
    if w.c.trait_context.is_some() {
        traits(&mut w, &program.statements);
    } else {
        w.walk_program(program, &mut ());
    }
    let errors = w.errors;
    c.errors.extend(errors);
}

/// In a trait context, PHPStan analyses only the trait's declaration.
fn traits<'arena>(
    w: &mut Walker<'_, '_, 'arena>,
    statements: &Sequence<'arena, Statement<'arena>>,
) {
    for s in statements {
        match s {
            Statement::Namespace(n) => {
                w.walk_in_namespace(n, &mut ());
                traits(w, n.statements());
                w.walk_out_namespace(n, &mut ());
            }
            Statement::Trait(t) => w.walk_trait(t, &mut ()),
            _ => {}
        }
    }
}

impl<'a, 'arena> Walker<'_, 'a, 'arena> {
    fn report(&mut self, at: Span, entry: &Entry, identifier: &str, message: String) {
        let c = self.c;
        let tips: Vec<&String> = entry.tips.iter().filter(|t| !t.is_empty()).collect();
        let tip = match tips.as_slice() {
            [] => None,
            [one] => Some((*one).clone()),
            many => Some(
                many.iter()
                    .map(|t| format!("• {t}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        };
        let file = c.file_name();
        self.errors.push(Error {
            identifier: Some(
                entry
                    .identifier
                    .clone()
                    .unwrap_or_else(|| identifier.to_string()),
            ),
            tip,
            file_path: (c.trait_context.as_ref())
                .map_or_else(|| file.clone(), |t| t.class_file.clone()),
            trait_file_path: c.trait_context.as_ref().map(|_| file),
            ..Error::new(message, c.label.clone(), Some(c.line_of(at)))
        });
    }

    fn class(&self) -> Option<&'a ClassLikeMetadata> {
        self.classes.last().and_then(|k| k.meta)
    }

    fn resolved(&self, id: &Identifier) -> String {
        self.c.resolved(id)
    }

    /// The expression's type; `Name::class` is a constant string also where the analyser types nothing (attribute
    /// arguments).
    fn type_at(&self, e: &Expression) -> Option<Type> {
        if let Expression::Access(Access::ClassConstant(a)) = e
            && let (Expression::Identifier(id), ClassLikeConstantSelector::Identifier(k)) =
                (a.class, &a.constant)
            && k.value.eq_ignore_ascii_case(b"class")
        {
            return Some(Type::ConstString(self.resolved(id).into_bytes()));
        }
        literal(e).or_else(|| self.c.phpstan_type(e))
    }

    fn attribute_names(&self, lists: &Sequence<'arena, AttributeList<'arena>>) -> Vec<String> {
        (lists.iter())
            .flat_map(|l| l.attributes.iter())
            .map(|a| self.resolved(&a.name))
            .collect()
    }

    fn function_name(&self, name: &LocalIdentifier) -> String {
        let name = String::from_utf8_lossy(name.value);
        match &self.namespace {
            Some(ns) => format!("{ns}\\{name}"),
            None => name.into_owned(),
        }
    }

    fn method_name(&self, name: &LocalIdentifier) -> String {
        let class = self
            .class()
            .map_or_else(|| "class@anonymous".to_string(), class_display);
        format!("{class}::{}", String::from_utf8_lossy(name.value))
    }

    /// The class-like a name resolves to, through `self`, `static` and `parent`.
    fn class_named(&self, name: &str) -> Option<&'a ClassLikeMetadata> {
        let current = self.class();
        match name.to_ascii_lowercase().as_str() {
            "self" | "static" => current,
            "parent" => current
                .and_then(|k| k.direct_parent_class)
                .and_then(|p| self.c.codebase.get_class_like(p.as_bytes())),
            _ => self.c.codebase.get_class_like(name.as_bytes()),
        }
    }

    /// The function-like's checks (attributes, parameter and return types), then its body in its own scope.
    fn function_like(
        &mut self,
        attributes: &Sequence<'arena, AttributeList<'arena>>,
        params: &FunctionLikeParameterList<'arena>,
        returns: Option<&FunctionLikeReturnTypeHint<'arena>>,
        at: Span,
        name: Option<String>,
    ) {
        let names = self.attribute_names(attributes);
        let site = Site {
            attributes: name.is_some().then(|| names.clone()),
            function: name,
        };
        usages::attributes(self, at, attributes, &site);
        let hint_site = Site {
            function: None,
            attributes: Some(names),
        };
        for p in &params.parameters {
            usages::attributes(self, p.span(), &p.attribute_lists, &Site::default());
            if let Some(h) = &p.hint {
                usages::type_hint(self, h, Some(Position::Param), &hint_site);
            }
        }
        if let Some(r) = returns {
            usages::type_hint(self, &r.hint, Some(Position::Return), &hint_site);
        }
    }

    /// The class-like a type's object class name names; an anonymous class's is its display name.
    fn type_class(&self, name: &str) -> Option<&'a ClassLikeMetadata> {
        if name.contains("@anonymous/") {
            return (self.c.codebase.class_likes.values())
                .find(|k| {
                    k.name.as_bytes().starts_with(b"{anonymous-class:") && self.c.display(k) == name
                })
                .map(|k| &**k);
        }
        self.c.codebase.get_class_like(name.as_bytes())
    }

    fn enter_class(&mut self, meta: Option<&'a ClassLikeMetadata>) {
        self.classes.push(ClassScope { meta });
    }
}

/// `ClassReflection::getDisplayName(false)`.
fn class_display(k: &ClassLikeMetadata) -> String {
    let name = String::from_utf8_lossy(k.original_name.as_bytes()).into_owned();
    if name.starts_with("{anonymous-class:") {
        "class@anonymous".to_string()
    } else {
        name
    }
}

/// A declaration's span from its first attribute, where PHP-Parser's node starts.
fn with_attributes(lists: &Sequence<'_, AttributeList<'_>>, span: Span) -> Span {
    lists.first().map_or(span, |l| l.span().join(span))
}

/// The constant type of a literal, which mago leaves untyped where no rule asked for it.
fn literal(e: &Expression) -> Option<Type> {
    match e {
        Expression::Literal(Literal::String(s)) => Some(Type::ConstString(s.value?.to_vec())),
        Expression::Literal(Literal::Integer(i)) => {
            Some(Type::ConstInt(i64::try_from(i.value?).ok()?))
        }
        Expression::Literal(Literal::True(_)) => Some(Type::ConstBool(true)),
        Expression::Literal(Literal::False(_)) => Some(Type::ConstBool(false)),
        Expression::Literal(Literal::Null(_)) => Some(Type::Null),
        Expression::Parenthesized(p) => literal(p.expression),
        _ => None,
    }
}

/// The expressions whose types the rules read: arguments, receivers and dynamic names.
pub(super) fn typed_expressions(on: bool, program: &Program) -> Vec<Span> {
    struct Typed(Vec<Span>);
    impl Typed {
        fn args(&mut self, list: Option<&ArgumentList>) {
            for a in list.into_iter().flat_map(|l| l.arguments.iter()) {
                self.0.push(a.value().span());
            }
        }
        fn selector(&mut self, s: &ClassLikeMemberSelector) {
            match s {
                ClassLikeMemberSelector::Variable(v) => self.0.push(v.span()),
                ClassLikeMemberSelector::Expression(e) => self.0.push(e.expression.span()),
                ClassLikeMemberSelector::Identifier(_) | ClassLikeMemberSelector::Missing(_) => {}
            }
        }
    }
    impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for Typed {
        fn walk_in_function_call(&mut self, call: &'ast FunctionCall<'arena>, (): &mut ()) {
            self.args(Some(&call.argument_list));
        }
        fn walk_in_method_call(&mut self, call: &'ast MethodCall<'arena>, (): &mut ()) {
            self.0.push(call.object.span());
            self.selector(&call.method);
            self.args(Some(&call.argument_list));
        }
        fn walk_in_method_partial_application(
            &mut self,
            m: &'ast MethodPartialApplication<'arena>,
            (): &mut (),
        ) {
            self.0.push(m.object.span());
        }
        fn walk_in_static_method_call(
            &mut self,
            call: &'ast StaticMethodCall<'arena>,
            (): &mut (),
        ) {
            self.0.push(call.class.span());
            self.selector(&call.method);
            self.args(Some(&call.argument_list));
        }
        fn walk_in_static_method_partial_application(
            &mut self,
            m: &'ast StaticMethodPartialApplication<'arena>,
            (): &mut (),
        ) {
            self.0.push(m.class.span());
        }
        fn walk_in_instantiation(&mut self, new: &'ast Instantiation<'arena>, (): &mut ()) {
            self.0.push(new.class.span());
            self.args(new.argument_list.as_ref());
        }
        fn walk_in_property_access(&mut self, p: &'ast PropertyAccess<'arena>, (): &mut ()) {
            self.0.push(p.object.span());
            self.selector(&p.property);
        }
        fn walk_in_static_property_access(
            &mut self,
            p: &'ast StaticPropertyAccess<'arena>,
            (): &mut (),
        ) {
            self.0.push(p.class.span());
            match &p.property {
                Variable::Nested(n) => self.0.push(n.variable.span()),
                Variable::Indirect(i) => self.0.push(i.expression.span()),
                Variable::Direct(_) => {}
            }
        }
        fn walk_in_class_constant_access(
            &mut self,
            a: &'ast ClassConstantAccess<'arena>,
            (): &mut (),
        ) {
            self.0.push(a.class.span());
            if let ClassLikeConstantSelector::Expression(e) = &a.constant {
                self.0.push(e.expression.span());
            }
        }
    }
    if !on {
        return Vec::new();
    }
    let mut typed = Typed(Vec::new());
    typed.walk_program(program, &mut ());
    typed.0
}

impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for Walker<'_, '_, 'arena> {
    fn walk_in_namespace(&mut self, n: &'ast Namespace<'arena>, (): &mut ()) {
        self.namespace = n.name.as_ref().map(|id| self.resolved(id));
    }
    fn walk_out_namespace(&mut self, _: &'ast Namespace<'arena>, (): &mut ()) {
        self.namespace = None;
    }
    fn walk_class(&mut self, class: &'ast Class<'arena>, ctx: &mut ()) {
        usages::attributes(self, class.span(), &class.attribute_lists, &Site::default());
        usages::class_hierarchy(
            self,
            class.span(),
            class.extends.as_ref(),
            class.implements.as_ref(),
        );
        let meta = self
            .c
            .codebase
            .get_class_like(self.c.resolved_span(class.name.span).as_bytes());
        self.enter_class(meta);
        for m in &class.members {
            self.walk_class_like_member(m, ctx);
        }
        self.classes.pop();
    }
    fn walk_interface(&mut self, i: &'ast Interface<'arena>, ctx: &mut ()) {
        usages::attributes(self, i.span(), &i.attribute_lists, &Site::default());
        usages::class_hierarchy(self, i.span(), i.extends.as_ref(), None);
        let meta = self
            .c
            .codebase
            .get_class_like(self.c.resolved_span(i.name.span).as_bytes());
        self.enter_class(meta);
        for m in &i.members {
            self.walk_class_like_member(m, ctx);
        }
        self.classes.pop();
    }
    fn walk_enum(&mut self, e: &'ast Enum<'arena>, ctx: &mut ()) {
        usages::attributes(self, e.span(), &e.attribute_lists, &Site::default());
        usages::class_hierarchy(self, e.span(), None, e.implements.as_ref());
        let meta = self
            .c
            .codebase
            .get_class_like(self.c.resolved_span(e.name.span).as_bytes());
        self.enter_class(meta);
        for m in &e.members {
            self.walk_class_like_member(m, ctx);
        }
        self.classes.pop();
    }
    fn walk_trait(&mut self, t: &'ast Trait<'arena>, ctx: &mut ()) {
        let Some(context) = &self.c.trait_context else {
            usages::attributes(self, t.span(), &t.attribute_lists, &Site::default());
            return;
        };
        if !(context.trait_name).eq_ignore_ascii_case(&self.c.resolved_span(t.name.span)) {
            return;
        }
        let meta = self.c.codebase.get_class_like(context.class.as_bytes());
        self.enter_class(meta);
        for m in &t.members {
            self.walk_class_like_member(m, ctx);
        }
        self.classes.pop();
    }
    fn walk_anonymous_class(&mut self, a: &'ast AnonymousClass<'arena>, ctx: &mut ()) {
        if let Some(arguments) = &a.argument_list {
            self.walk_partial_argument_list(arguments, ctx);
        }
        usages::attributes(self, a.span(), &a.attribute_lists, &Site::default());
        usages::class_hierarchy(self, a.span(), a.extends.as_ref(), a.implements.as_ref());
        let name = mago_codex::get_anonymous_class_name(self.c.file, a.span());
        let meta = self.c.codebase.get_class_like(name.as_bytes());
        self.enter_class(meta);
        for m in &a.members {
            self.walk_class_like_member(m, ctx);
        }
        self.classes.pop();
    }
    fn walk_function(&mut self, f: &'ast Function<'arena>, ctx: &mut ()) {
        let name = self.function_name(&f.name);
        self.function_like(
            &f.attribute_lists,
            &f.parameter_list,
            f.return_type_hint.as_ref(),
            f.span(),
            Some(name.clone()),
        );
        // a function declared in another one has no reflection, so no attributes (`getCallAttributes`)
        let attributes = if self.functions.is_empty() {
            self.attribute_names(&f.attribute_lists)
        } else {
            Vec::new()
        };
        self.walk_function_like_parameter_list(&f.parameter_list, ctx);
        self.functions.push(FunctionScope { name, attributes });
        self.walk_block(&f.body, ctx);
        self.functions.pop();
    }
    fn walk_method(&mut self, m: &'ast Method<'arena>, ctx: &mut ()) {
        let name = self.method_name(&m.name);
        self.function_like(
            &m.attribute_lists,
            &m.parameter_list,
            m.return_type_hint.as_ref(),
            m.span(),
            Some(name.clone()),
        );
        let attributes = self.attribute_names(&m.attribute_lists);
        self.walk_function_like_parameter_list(&m.parameter_list, ctx);
        self.functions.push(FunctionScope { name, attributes });
        if let MethodBody::Concrete(b) = &m.body {
            self.walk_block(b, ctx);
        }
        self.functions.pop();
    }
    fn walk_in_closure(&mut self, f: &'ast Closure<'arena>, (): &mut ()) {
        self.function_like(
            &f.attribute_lists,
            &f.parameter_list,
            f.return_type_hint.as_ref(),
            f.span(),
            None,
        );
    }
    fn walk_in_arrow_function(&mut self, f: &'ast ArrowFunction<'arena>, (): &mut ()) {
        self.function_like(
            &f.attribute_lists,
            &f.parameter_list,
            f.return_type_hint.as_ref(),
            f.span(),
            None,
        );
    }
    fn walk_in_property(&mut self, p: &'ast Property<'arena>, (): &mut ()) {
        let (lists, hint) = match p {
            Property::Plain(p) => (&p.attribute_lists, p.hint.as_ref()),
            Property::Hooked(p) => (&p.attribute_lists, p.hint.as_ref()),
        };
        usages::attributes(
            self,
            with_attributes(lists, p.span()),
            lists,
            &Site::default(),
        );
        if let Some(h) = hint {
            usages::type_hint(self, h, None, &Site::default());
        }
        self.property = Some(String::from_utf8_lossy(p.first_variable().name).into_owned());
    }
    fn walk_out_property(&mut self, _: &'ast Property<'arena>, (): &mut ()) {
        self.property = None;
    }
    /// A hook is a function-like: its attributes are checked in the class scope with the hook's own attributes, and
    /// its body runs as method `Class::$property::get`.
    fn walk_property_hook(&mut self, h: &'ast PropertyHook<'arena>, ctx: &mut ()) {
        let attributes = self.attribute_names(&h.attribute_lists);
        let class = self
            .class()
            .map_or_else(|| "class@anonymous".to_string(), class_display);
        let property = self.property.clone().unwrap_or_default();
        let name = format!(
            "{class}::{property}::{}",
            String::from_utf8_lossy(h.name.value)
        );
        let site = Site {
            function: None,
            attributes: Some(attributes.clone()),
        };
        usages::attributes(
            self,
            with_attributes(&h.attribute_lists, h.span()),
            &h.attribute_lists,
            &site,
        );
        if let Some(p) = &h.parameter_list {
            self.walk_function_like_parameter_list(p, ctx);
        }
        self.functions.push(FunctionScope { name, attributes });
        self.walk_property_hook_body(&h.body, ctx);
        self.functions.pop();
    }
    fn walk_in_class_like_constant(&mut self, k: &'ast ClassLikeConstant<'arena>, (): &mut ()) {
        let at = with_attributes(&k.attribute_lists, k.span());
        usages::attributes(self, at, &k.attribute_lists, &Site::default());
        if let Some(h) = &k.hint {
            usages::type_hint(self, h, None, &Site::default());
        }
    }
    fn walk_in_enum_case(&mut self, e: &'ast EnumCase<'arena>, (): &mut ()) {
        usages::attributes(self, e.span(), &e.attribute_lists, &Site::default());
    }
    fn walk_in_use(&mut self, u: &'ast Use<'arena>, (): &mut ()) {
        usages::use_statement(self, u);
    }
    fn walk_in_trait_use(&mut self, u: &'ast TraitUse<'arena>, (): &mut ()) {
        usages::trait_use(self, u);
    }
    fn walk_in_function_call(&mut self, call: &'ast FunctionCall<'arena>, (): &mut ()) {
        calls::function_call(self, call);
    }
    fn walk_in_function_partial_application(
        &mut self,
        f: &'ast FunctionPartialApplication<'arena>,
        (): &mut (),
    ) {
        calls::function_callable(self, f);
    }
    fn walk_in_method_call(&mut self, call: &'ast MethodCall<'arena>, (): &mut ()) {
        calls::method_call(self, call);
    }
    fn walk_in_method_partial_application(
        &mut self,
        m: &'ast MethodPartialApplication<'arena>,
        (): &mut (),
    ) {
        calls::method_callable(self, m);
    }
    fn walk_in_static_method_call(&mut self, call: &'ast StaticMethodCall<'arena>, (): &mut ()) {
        usages::static_class(self, call.span(), call.class);
        calls::static_call(self, call);
    }
    fn walk_in_static_method_partial_application(
        &mut self,
        m: &'ast StaticMethodPartialApplication<'arena>,
        (): &mut (),
    ) {
        usages::static_class(self, m.span(), m.class);
        calls::static_callable(self, m);
    }
    fn walk_in_instantiation(&mut self, new: &'ast Instantiation<'arena>, (): &mut ()) {
        usages::instantiation(self, new);
        calls::instantiation(self, new);
    }
    fn walk_in_binary(&mut self, b: &'ast Binary<'arena>, (): &mut ()) {
        if matches!(b.operator, BinaryOperator::Instanceof(_)) {
            usages::static_class(self, b.span(), b.rhs);
        }
    }
    fn walk_in_construct(&mut self, construct: &'ast Construct<'arena>, (): &mut ()) {
        calls::construct(self, construct);
        keywords::construct(self, construct);
    }
    fn walk_in_echo(&mut self, e: &'ast Echo<'arena>, (): &mut ()) {
        calls::language(self, e.span(), "echo", "disallowed.echo");
    }
    fn walk_in_unset(&mut self, u: &'ast Unset<'arena>, (): &mut ()) {
        calls::language(self, u.span(), "unset", "disallowed.unset");
    }
    fn walk_in_shell_execute_string(&mut self, s: &'ast ShellExecuteString<'arena>, (): &mut ()) {
        calls::backtick(self, s.span());
    }
    fn walk_in_constant_access(&mut self, a: &'ast ConstantAccess<'arena>, (): &mut ()) {
        usages::constant(self, a);
    }
    fn walk_in_class_constant_access(&mut self, a: &'ast ClassConstantAccess<'arena>, (): &mut ()) {
        usages::static_class(self, a.span(), a.class);
        usages::class_constant(self, a);
    }
    fn walk_in_variable(&mut self, v: &'ast Variable<'arena>, (): &mut ()) {
        usages::variable(self, v);
    }
    fn walk_in_property_access(&mut self, p: &'ast PropertyAccess<'arena>, (): &mut ()) {
        usages::property(self, p);
    }
    fn walk_in_static_property_access(
        &mut self,
        p: &'ast StaticPropertyAccess<'arena>,
        (): &mut (),
    ) {
        usages::static_property(self, p);
    }
    fn walk_in_if(&mut self, i: &'ast If<'arena>, (): &mut ()) {
        keywords::r#if(self, i);
    }
    fn walk_in_while(&mut self, w: &'ast While<'arena>, (): &mut ()) {
        keywords::keyword(self, w.span(), "while", "disallowed.while");
    }
    fn walk_in_do_while(&mut self, d: &'ast DoWhile<'arena>, (): &mut ()) {
        keywords::keyword(self, d.span(), "do-while", "disallowed.doWhile");
    }
    fn walk_in_for(&mut self, f: &'ast For<'arena>, (): &mut ()) {
        keywords::keyword(self, f.span(), "for", "disallowed.for");
    }
    fn walk_in_foreach(&mut self, f: &'ast Foreach<'arena>, (): &mut ()) {
        keywords::keyword(self, f.span(), "foreach", "disallowed.foreach");
    }
    fn walk_in_break(&mut self, b: &'ast Break<'arena>, (): &mut ()) {
        keywords::keyword(self, b.span(), "break", "disallowed.break");
    }
    fn walk_in_continue(&mut self, k: &'ast Continue<'arena>, (): &mut ()) {
        keywords::keyword(self, k.span(), "continue", "disallowed.continue");
    }
    fn walk_in_switch(&mut self, s: &'ast Switch<'arena>, (): &mut ()) {
        keywords::keyword(self, s.span(), "switch", "disallowed.switch");
    }
    fn walk_in_match(&mut self, m: &'ast Match<'arena>, (): &mut ()) {
        keywords::keyword(self, m.span(), "match", "disallowed.match");
    }
    fn walk_in_declare(&mut self, d: &'ast Declare<'arena>, (): &mut ()) {
        keywords::keyword(self, d.span(), "declare", "disallowed.declare");
    }
    fn walk_in_return(&mut self, r: &'ast Return<'arena>, (): &mut ()) {
        keywords::keyword(self, r.span(), "return", "disallowed.return");
    }
    fn walk_in_goto(&mut self, g: &'ast Goto<'arena>, (): &mut ()) {
        keywords::keyword(self, g.span(), "goto", "disallowed.goto");
    }
    fn walk_in_global(&mut self, g: &'ast Global<'arena>, (): &mut ()) {
        keywords::global(self, g.span());
    }
}
