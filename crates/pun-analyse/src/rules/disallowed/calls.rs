//! `analyse/disallowed/function` (`FunctionCalls`, `FunctionFirstClassCallables`, `EvalCalls`, `EchoCalls`,
//! `PrintCalls`, `EmptyCalls`, `IssetCalls`, `UnsetCalls`, `ExitDieCalls`, `ShellExecCalls`),
//! `analyse/disallowed/method` (`MethodCalls`, `NewCalls`, `MethodFirstClassCallables`) and
//! `analyse/disallowed/static-method` (`StaticCalls`, `StaticFirstClassCallables`), with
//! `RuleErrors/Disallowed{Calls,Function,Method,CallableParameter}RuleErrors`.

use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::metadata::function_like::FunctionLikeMetadata;
use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;
use pun_phpstan::types::Type;

use super::super::members::{self, Method};
use super::{Arg, Entry, Site, Walker, args, format_identifier, format_message, matches};

/// One call a rule checks: `DisallowedCallsRuleErrors::get`'s arguments.
struct Call<'s, 'e, 'arena> {
    at: Span,
    args: Option<&'s [Arg<'e, 'arena>]>,
    name: String,
    display: Option<String>,
    defined_in: Option<String>,
    builtin: bool,
    identifier: &'static str,
}

impl<'a> Walker<'_, 'a, '_> {
    /// `DisallowedCallsRuleErrors::get`: the first entry that forbids the call.
    fn forbid(&mut self, call: &Call, entries: &'a [Entry], format: Option<&str>) -> bool {
        for e in entries {
            if !matches(&e.name, &call.name, &e.excludes)
                || !self.defined_in_matches(e, call)
                || self.is_allowed(&Site::default(), call.args, e, true, None)
            {
                continue;
            }
            let name = &call.name;
            let shown = match &call.display {
                Some(d) if d != name => format!("{name}() (as {d}())"),
                _ => format!("{name}()"),
            };
            let suffix = if e.name == *name {
                String::new()
            } else {
                format!(" [{name}() matches {}()]", e.name)
            };
            let message = format_message(e.message.as_deref());
            let text = match format {
                Some(f) => format!("{f}{message}{suffix}"),
                None => format!("Calling {shown} is forbidden{message}{suffix}"),
            };
            self.report(call.at, e, call.identifier, text);
            return true;
        }
        false
    }

    fn defined_in_matches(&self, e: &Entry, call: &Call) -> bool {
        if e.defined_in.is_empty() {
            return true;
        }
        if call.builtin {
            return false;
        }
        let Some(file) = &call.defined_in else {
            return true;
        };
        e.defined_in.iter().any(|p| self.d.path_matches(p, file))
    }

    pub(super) fn file_of(&self, span: Span) -> Option<String> {
        (self.c.files.get(&span.file_id)).map(|p| p.to_string_lossy().into_owned())
    }

    /// `TypeResolver::getNames` for a function call's name, with `originalName` for the message.
    fn function_names(&self, function: &Expression) -> Vec<(String, Option<String>)> {
        match function {
            Expression::Identifier(id) => {
                let written = String::from_utf8_lossy(id.value())
                    .trim_start_matches('\\')
                    .to_string();
                match id {
                    Identifier::Local(l) if !self.c.names.is_imported(&l.span) => {
                        let mut names = Vec::new();
                        // the written name is PHP-Parser's `originalName`, the display of both candidates
                        if let Some(ns) = &self.namespace {
                            names.push((format!("{ns}\\{written}"), Some(written.clone())));
                        }
                        names.push((written, None));
                        names
                    }
                    Identifier::FullyQualified(_) => vec![(written, None)],
                    _ => vec![(self.resolved(id), Some(written))],
                }
            }
            e => self
                .string_names(e)
                .into_iter()
                .map(|n| (n, None))
                .collect(),
        }
    }

    /// A dynamic name's constant string values, without a leading backslash.
    fn string_names(&self, e: &Expression) -> Vec<String> {
        let Some(t) = self.type_at(e) else {
            return Vec::new();
        };
        let strings = t.constant_strings();
        let first = if matches!(e, Expression::Variable(_)) {
            strings.into_iter().take(1).collect()
        } else {
            strings
        };
        (first.iter())
            .map(|s| super::normalize(&String::from_utf8_lossy(s)))
            .collect()
    }

    /// `DisallowedFunctionRuleErrors::getErrors`.
    fn function_errors(
        &mut self,
        at: Span,
        name: &str,
        display: Option<String>,
        args: Option<&[Arg]>,
    ) -> bool {
        let Some(f) = self
            .c
            .codebase
            .get_function(name.to_ascii_lowercase().as_bytes())
        else {
            return false;
        };
        let call = Call {
            at,
            args,
            name: name.to_string(),
            display: Some(display.unwrap_or_else(|| name.to_string())),
            defined_in: self.file_of(f.span),
            builtin: f.flags.is_built_in(),
            identifier: "disallowed.function",
        };
        self.forbid(&call, &{ self.d }.functions, None)
    }

    fn function(&mut self, at: Span, function: &Expression, args: Option<&[Arg]>) {
        for (name, display) in self.function_names(function) {
            if self.function_errors(at, &name, display, args) {
                return;
            }
        }
    }
}

pub(super) fn function_call<'arena>(w: &mut Walker<'_, '_, 'arena>, call: &FunctionCall<'arena>) {
    let list = args(&call.argument_list);
    w.function(call.span(), call.function, Some(&list));
    let functions: Vec<_> = (w.function_names(call.function).into_iter())
        .filter_map(|(n, _)| w.c.codebase.get_function(n.to_ascii_lowercase().as_bytes()))
        .collect();
    for f in functions {
        callable_parameters(w, call.span(), f, &list);
    }
}

pub(super) fn function_callable<'arena>(
    w: &mut Walker<'_, '_, 'arena>,
    f: &FunctionPartialApplication<'arena>,
) {
    w.function(f.span(), f.function, None);
}

/// `EvalCalls`, `PrintCalls`, `EmptyCalls`, `IssetCalls` and `ExitDieCalls`.
pub(super) fn construct(w: &mut Walker, construct: &Construct) {
    let (name, identifier) = match construct {
        Construct::Eval(_) => ("eval", "disallowed.eval"),
        Construct::Print(_) => ("print", "disallowed.print"),
        Construct::Empty(_) => ("empty", "disallowed.empty"),
        Construct::Isset(_) => ("isset", "disallowed.isset"),
        Construct::Exit(_) => ("exit", "disallowed.exit"),
        Construct::Die(_) => ("die", "disallowed.die"),
        _ => return,
    };
    language(w, construct.span(), name, identifier);
}

/// A language construct checked as a builtin function without arguments.
pub(super) fn language(w: &mut Walker, at: Span, name: &str, identifier: &'static str) {
    let call = Call {
        at,
        args: None,
        name: name.to_string(),
        display: Some(name.to_string()),
        defined_in: None,
        builtin: true,
        identifier,
    };
    w.forbid(&call, &{ w.d }.functions, None);
}

/// `ShellExecCalls`.
pub(super) fn backtick(w: &mut Walker, at: Span) {
    let call = Call {
        at,
        args: None,
        name: "shell_exec".to_string(),
        display: None,
        defined_in: None,
        builtin: true,
        identifier: "disallowed.backtick",
    };
    w.forbid(
        &call,
        &{ w.d }.functions,
        Some("Using the backtick operator (`...`) is forbidden because shell_exec() is forbidden"),
    );
}

impl<'a> Walker<'_, 'a, '_> {
    /// The classes a receiver's type has methods of (a class string's class too), with `getClassNames`.
    fn receiver_classes(&self, t: &Type) -> Vec<&'a ClassLikeMetadata> {
        let t = match t {
            Type::ClassString(Some(object)) => object,
            t => t,
        };
        (t.object_class_names().iter())
            .filter_map(|n| self.type_class(n))
            .collect()
    }

    /// `TypeResolver::getNames` for a member selector.
    pub(super) fn member_names(&self, selector: &ClassLikeMemberSelector) -> Vec<String> {
        match selector {
            ClassLikeMemberSelector::Identifier(id) => {
                vec![String::from_utf8_lossy(id.value).into_owned()]
            }
            ClassLikeMemberSelector::Variable(v) => (self.c.phpstan_type_at(v.span()).into_iter())
                .flat_map(|t| t.constant_strings().into_iter().take(1))
                .map(|s| String::from_utf8_lossy(&s).into_owned())
                .collect(),
            ClassLikeMemberSelector::Expression(e) => (self.type_at(e.expression).into_iter())
                .flat_map(|t| t.constant_strings())
                .map(|s| String::from_utf8_lossy(&s).into_owned())
                .collect(),
            ClassLikeMemberSelector::Missing(_) => Vec::new(),
        }
    }

    /// `DisallowedMethodRuleErrors::getErrors`.
    fn method_errors(
        &mut self,
        at: Span,
        classes: &[&'a ClassLikeMetadata],
        method: &str,
        args: Option<&[Arg]>,
        entries: &'a [Entry],
    ) {
        if classes.is_empty() {
            return;
        }
        let found: Vec<_> = classes
            .iter()
            .filter_map(|k| members::method(self.c, k, method.as_bytes()))
            .collect();
        if found.len() != classes.len() {
            return;
        }
        let m = &found[0];
        let method_name = m.name();
        let names: Vec<String> = classes.iter().map(|k| super::class_display(k)).collect();
        let called_as = format!("{}::{method_name}", format_identifier(&names));
        let declaring_class = match m {
            Method::User { declaring: k, .. } => Some(*k),
            Method::Builtin { declaring: k, .. } => {
                self.c.codebase.get_class_like(k.name.as_bytes())
            }
        };
        let codebase = self.c.codebase;
        // the declaring class, then its traits and interfaces
        let declaring: Vec<&ClassLikeMetadata> = (declaring_class.into_iter())
            .flat_map(|k| {
                std::iter::once(k).chain(
                    (k.used_traits.iter().chain(&k.all_parent_interfaces))
                        .filter_map(|n| codebase.get_class_like(n.as_bytes())),
                )
            })
            .collect();
        for k in declaring {
            if members::method(self.c, k, method.as_bytes()).is_none()
                && !self.trait_has_method(k, method)
            {
                continue;
            }
            let call = Call {
                at,
                args,
                name: format!("{}::{method_name}", super::class_display(k)),
                display: Some(called_as.clone()),
                defined_in: self.file_of(k.span),
                builtin: k.flags.is_built_in(),
                identifier: "disallowed.method",
            };
            if self.forbid(&call, entries, None) {
                return;
            }
        }
    }

    fn trait_has_method(&self, k: &ClassLikeMetadata, method: &str) -> bool {
        k.methods
            .contains(&mago_word::word(method.to_ascii_lowercase()))
    }

    fn method(
        &mut self,
        at: Span,
        object: &Expression,
        selector: &ClassLikeMemberSelector,
        args: Option<&[Arg]>,
    ) {
        let Some(t) = self.type_at(object) else {
            return;
        };
        let classes = self.receiver_classes(&t);
        for name in self.member_names(selector) {
            self.method_errors(at, &classes, &name, args, &{ self.d }.methods);
        }
    }

    fn static_method(
        &mut self,
        at: Span,
        class: &Expression,
        selector: &ClassLikeMemberSelector,
        args: Option<&[Arg]>,
    ) {
        let classes = self.named_classes_of(class);
        for name in self.member_names(selector) {
            self.method_errors(at, &classes, &name, args, &{ self.d }.statics);
        }
    }
}

pub(super) fn method_call<'arena>(w: &mut Walker<'_, '_, 'arena>, call: &MethodCall<'arena>) {
    let list = args(&call.argument_list);
    w.method(call.span(), call.object, &call.method, Some(&list));
    if let Some(t) = w.type_at(call.object) {
        let classes = w.receiver_classes(&t);
        for name in w.member_names(&call.method) {
            method_parameters(w, call.span(), &classes, &name, &list);
        }
    }
}

pub(super) fn method_callable<'arena>(
    w: &mut Walker<'_, '_, 'arena>,
    m: &MethodPartialApplication<'arena>,
) {
    w.method(m.span(), m.object, &m.method, None);
}

pub(super) fn static_call<'arena>(w: &mut Walker<'_, '_, 'arena>, call: &StaticMethodCall<'arena>) {
    let list = args(&call.argument_list);
    w.static_method(call.span(), call.class, &call.method, Some(&list));
    let classes = w.named_classes_of(call.class);
    for name in w.member_names(&call.method) {
        method_parameters(w, call.span(), &classes, &name, &list);
    }
}

pub(super) fn static_callable<'arena>(
    w: &mut Walker<'_, '_, 'arena>,
    m: &StaticMethodPartialApplication<'arena>,
) {
    w.static_method(m.span(), m.class, &m.method, None);
}

/// `NewCalls`.
pub(super) fn instantiation<'arena>(w: &mut Walker<'_, '_, 'arena>, new: &Instantiation<'arena>) {
    let class_names: Vec<String> = match new.class {
        Expression::Identifier(id) => vec![w.resolved(id)],
        Expression::Self_(_) | Expression::Static(_) | Expression::Parent(_) => {
            let keyword = w.c.written(new.class.span());
            w.class_named(&keyword)
                .map(super::class_display)
                .into_iter()
                .collect()
        }
        e => w.string_names(e),
    };
    let list = new.argument_list.as_ref().map(args).unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for class_name in class_names {
        let k = w.c.codebase.get_class_like(class_name.as_bytes());
        names.push(class_name.clone());
        if let Some(k) = k {
            for p in &k.all_parent_classes {
                names.push(w.original_class_name(p.as_bytes()));
            }
            for i in &k.all_parent_interfaces {
                names.push(w.original_class_name(i.as_bytes()));
            }
        }
        for name in names.clone() {
            let call = Call {
                at: new.span(),
                args: Some(&list),
                name: format!("{name}::__construct"),
                display: Some(format!("{class_name}::__construct")),
                defined_in: k.and_then(|k| w.file_of(k.span)),
                builtin: k.is_some_and(|k| k.flags.is_built_in()),
                identifier: "disallowed.new",
            };
            w.forbid(&call, &{ w.d }.methods, None);
            let classes: Vec<_> =
                w.c.codebase
                    .get_class_like(name.as_bytes())
                    .into_iter()
                    .collect();
            method_parameters(w, new.span(), &classes, "__construct", &list);
        }
    }
}

impl Walker<'_, '_, '_> {
    fn original_class_name(&self, name: &[u8]) -> String {
        (self.c.codebase.get_class_like(name)).map_or_else(
            || String::from_utf8_lossy(name).into_owned(),
            |k| String::from_utf8_lossy(k.original_name.as_bytes()).into_owned(),
        )
    }
}

fn method_parameters<'arena>(
    w: &mut Walker<'_, '_, 'arena>,
    at: Span,
    classes: &[&ClassLikeMetadata],
    name: &str,
    list: &[Arg<'_, 'arena>],
) {
    for k in classes {
        match members::method(w.c, k, name.as_bytes()) {
            Some(Method::User { f, .. }) => callable_parameters(w, at, f, list),
            Some(Method::Builtin { f, .. }) => {
                // `getVariants()` of more than one variant reports nothing
                if let [v] = f.variants.as_slice() {
                    let params: Vec<(String, Type)> = (v.params.iter())
                        .map(|p| {
                            (
                                p.name.clone(),
                                pun_phpstan::resolve::resolve_str(&p.ty, None),
                            )
                        })
                        .collect();
                    callable_arguments(w, at, &params, list);
                }
            }
            None => {}
        }
    }
}

/// `DisallowedCallableParameterRuleErrors::getErrors`: the functions and methods a call's callable arguments name.
fn callable_parameters<'arena>(
    w: &mut Walker<'_, '_, 'arena>,
    at: Span,
    f: &FunctionLikeMetadata,
    list: &[Arg<'_, 'arena>],
) {
    let params: Vec<(String, Type)> = (f.parameters.iter())
        .filter_map(|p| {
            let t = p.type_metadata.as_ref()?;
            Some((
                String::from_utf8_lossy(&p.name.0.as_bytes()[1..]).into_owned(),
                w.c.to_phpstan(&t.type_union),
            ))
        })
        .collect();
    callable_arguments(w, at, &params, list);
}

/// Whether PHPStan's `isCallable()` is yes for a parameter type: a callable type, or constant strings that each
/// name a function.
fn is_callable(w: &Walker, t: &Type) -> bool {
    let t = t.clone().remove_null();
    if t.is_callable().yes() {
        return true;
    }
    let strings = t.constant_strings();
    !strings.is_empty()
        && (strings.iter()).all(|s| {
            w.c.codebase
                .get_function(s.to_ascii_lowercase().as_slice())
                .is_some()
        })
}

/// The callable arguments of a call to a function-like with parameters `params` (name without `$`, type).
fn callable_arguments<'arena>(
    w: &mut Walker<'_, '_, 'arena>,
    at: Span,
    params: &[(String, Type)],
    list: &[Arg<'_, 'arena>],
) {
    if w.d.functions.is_empty() && w.d.methods.is_empty() && w.d.statics.is_empty() {
        return;
    }
    let methods = &{ w.d }.callables;
    for (i, (pname, t)) in params.iter().enumerate() {
        if !is_callable(w, t) {
            continue;
        }
        let pname = pname.as_bytes();
        let arg = (list.iter())
            .find(|a| a.name == Some(pname))
            .or_else(|| list.get(i).filter(|a| a.name.is_none()));
        let Some(value) = arg.and_then(|a| w.type_at(a.value)) else {
            continue;
        };
        for s in value.constant_strings() {
            let name = String::from_utf8_lossy(&s).into_owned();
            if !name.is_empty() {
                w.function_errors(at, &name, None, None);
            }
        }
        for array in value.constant_arrays() {
            let [class, method] = array.values.as_slice() else {
                continue;
            };
            let Some(method) = method.constant_strings().into_iter().next() else {
                continue;
            };
            let method = String::from_utf8_lossy(&method).into_owned();
            let names = match class {
                Type::ConstString(s) => vec![String::from_utf8_lossy(s).into_owned()],
                t => t.object_class_names(),
            };
            for n in names {
                let classes: Vec<_> =
                    w.c.codebase
                        .get_class_like(n.as_bytes())
                        .into_iter()
                        .collect();
                w.method_errors(at, &classes, &method, None, methods);
            }
        }
    }
}
