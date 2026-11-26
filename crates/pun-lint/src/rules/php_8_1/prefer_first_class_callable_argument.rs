//! Rector 2.8.4's `FunctionFirstClassCallableRector`.

use mago_syntax::cst::{Argument, Expression, Literal, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::names::{
    function_call_matches_any, imported_function, scope_statements,
};
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// The parameters whose native type is `callable` (a `ReflectionNamedType` named `callable`, nullable or not), by
/// function and position: PHP 8.5's reflection of its built-in functions, which Rector reads at run time.
const CALLABLE_PARAMETERS: &[(&str, &[usize])] = &[
    ("array_all", &[1]),
    ("array_any", &[1]),
    ("array_filter", &[1]),
    ("array_find", &[1]),
    ("array_find_key", &[1]),
    ("array_map", &[0]),
    ("array_reduce", &[1]),
    ("array_walk", &[1]),
    ("array_walk_recursive", &[1]),
    ("call_user_func", &[0]),
    ("call_user_func_array", &[0]),
    ("forward_static_call", &[0]),
    ("forward_static_call_array", &[0]),
    ("header_register_callback", &[0]),
    ("iterator_apply", &[1]),
    ("libxml_set_external_entity_loader", &[0]),
    ("mb_ereg_replace_callback", &[1]),
    ("preg_replace_callback", &[1]),
    ("readline_callback_handler_install", &[1]),
    ("readline_completion_function", &[0]),
    ("register_shutdown_function", &[0]),
    ("register_tick_function", &[0]),
    ("session_set_save_handler", &[2, 3, 4, 5, 6, 7, 8]),
    ("set_error_handler", &[0]),
    ("set_exception_handler", &[0]),
    ("spl_autoload_register", &[0]),
    ("spl_autoload_unregister", &[0]),
    ("uasort", &[1]),
    ("uksort", &[1]),
    ("unregister_tick_function", &[0]),
    ("usort", &[1]),
];

pub struct PreferFirstClassCallableArgument;

impl Rule for PreferFirstClassCallableArgument {
    fn id(&self) -> &'static str {
        "lint/php-8.1/prefer-first-class-callable-argument"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        let names: Vec<&str> = CALLABLE_PARAMETERS.iter().map(|(n, _)| *n).collect();
        let Some(name) = function_call_matches_any(ancestors, call, &names) else {
            return;
        };
        let positions = CALLABLE_PARAMETERS
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap()
            .1;
        let mut edits = Vec::new();
        for (key, arg) in call.argument_list.arguments.iter().enumerate() {
            if !positions.contains(&key)
                || matches!(arg, Argument::Positional(a) if a.ellipsis.is_some())
            {
                continue;
            }
            let Expression::Literal(Literal::String(string)) = value(arg) else {
                continue;
            };
            let Some(callable) = string.value.map(String::from_utf8_lossy) else {
                continue;
            };
            let fully_qualified = callable.starts_with('\\');
            let callable = callable.trim_start_matches('\\');
            // Rector prints any string as a name; Pun leaves a string that is not one alone.
            if !is_name(callable) {
                continue;
            }
            let code =
                if fully_qualified || callable.contains('\\') || shadowed(ctx, ancestors, callable)
                {
                    format!("\\{callable}(...)")
                } else {
                    format!("{callable}(...)")
                };
            edits.push(Edit::new(node_span(value(arg)), code));
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            format!("Pass a first-class callable to `{name}()` instead of a string."),
            Safety::Safe,
            edits,
        ));
    }
}

/// A function name, or a `Class::method` name, with namespace separators.
fn is_name(callable: &str) -> bool {
    let (path, method) = match callable.split_once("::") {
        Some((path, method)) => (path, Some(method)),
        None => (callable, None),
    };
    let identifier = |s: &str| {
        let mut bytes = s.bytes();
        bytes
            .next()
            .is_some_and(|b| b == b'_' || b.is_ascii_alphabetic() || b >= 0x80)
            && bytes.all(|b| b == b'_' || b.is_ascii_alphanumeric() || b >= 0x80)
    };
    path.split('\\').all(identifier) && method.is_none_or(identifier)
}

/// Rector 2.9.2's `isShadowedFunction()`: a string callable always names the global function, while a bare name
/// resolves to a function of the namespace, or a `use function` import, first.
fn shadowed(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], name: &str) -> bool {
    if let Some(ns) = scope_statements(ancestors).0
        && facts(ctx)
            .codebase
            .function_exists(format!("{ns}\\{name}").as_bytes())
    {
        return true;
    }
    imported_function(ancestors, name.as_bytes()).is_some()
}
