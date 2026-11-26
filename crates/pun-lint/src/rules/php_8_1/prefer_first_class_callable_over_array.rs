//! Rector 2.9.2's `ArrayToFirstClassCallableRector`, with `ArrayCallableMethodMatcher` and the array context the
//! `ContextNodeVisitor` and `CallLikeReflectionNodeVisitor` decorate.

use mago_codex::metadata::CodebaseMetadata;
use mago_codex::metadata::function_like::FunctionLikeMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{
    Access, Argument, ArrayElement, ClassLikeConstantSelector, ClassLikeMemberSelector, Expression,
    FunctionLikeReturnTypeHint, Hint, Literal, MagicConstant, Node, NodeKind, Statement, Variable,
};

use crate::problem::{Edit, Safety};
use crate::rules::complexity::require_named_argument_order::{Target, callee};
use crate::rules::laravel::is_a;
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{named, text};
use crate::rules::support::statement_edits::is_in_constant_expression;
use crate::rules::types::class_scope;
use crate::rules::types::facts::{Facts, facts};
use crate::rules::types::names::resolve_class_name;
use crate::syntax::{is_this, node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferFirstClassCallableOverArray;

impl Rule for PreferFirstClassCallableOverArray {
    fn id(&self) -> &'static str {
        "lint/php-8.1/prefer-first-class-callable-over-array"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Array, NodeKind::LegacyArray])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let elements = match node {
            Node::Array(array) => &array.elements,
            Node::LegacyArray(array) => &array.elements,
            _ => return,
        };
        let [ArrayElement::Value(first), ArrayElement::Value(second)] = elements.as_slice() else {
            return;
        };
        let span = node_span(&node);
        if skipped_context(ctx, span, ancestors) {
            return;
        }
        let (caller, method) = (unparenthesize(first.value), unparenthesize(second.value));
        let facts = facts(ctx);
        // `ArrayCallableMethodMatcher::match`.
        let Some(class) = caller_class(&facts, ancestors, caller) else {
            return;
        };
        let Expression::Literal(Literal::String(string)) = method else {
            return;
        };
        let Some(method) = string
            .value
            .map(|v| String::from_utf8_lossy(v).into_owned())
        else {
            return;
        };
        if method == "__construct" {
            return;
        }
        let codebase = &facts.codebase;
        // skip non-existing methods
        let Some(found) = codebase.get_declaring_method(class.as_bytes(), method.as_bytes()) else {
            return;
        };
        let code = match caller {
            Expression::Access(Access::ClassConstant(fetch)) => {
                if let Expression::Identifier(id) = unparenthesize(fetch.class) {
                    let current = class_scope(ancestors).and_then(|s| s.name);
                    let name = resolve_class_name(ancestors, id);
                    // `isNonStaticOtherObject`
                    if current.is_none_or(|c| !c.eq_ignore_ascii_case(&name))
                        && (!is_static(found) || !is_public(found))
                    {
                        return;
                    }
                }
                format!("{}::{method}(...)", text(ctx, fetch.class))
            }
            Expression::Variable(_) | Expression::Access(Access::Property(_)) => {
                // If the callable is scoped to another object / variable then it should only be converted if it
                // is public
                if !is_this(caller) && !is_public(found) {
                    return;
                }
                format!("{}->{method}(...)", text(ctx, caller))
            }
            _ => return,
        };
        out.push(Report::fixed(
            span,
            "Use a first-class callable instead of an array callable.",
            Safety::Unsafe,
            vec![Edit::new(span, code)],
        ));
    }
}

fn is_public(method: &FunctionLikeMetadata) -> bool {
    (method.method_metadata.as_ref()).is_none_or(|m| m.visibility.is_public())
}

fn is_static(method: &FunctionLikeMetadata) -> bool {
    (method.method_metadata.as_ref()).is_some_and(|m| m.is_static)
}

/// `resolveCallerType`, as a class name (`TypeWithClassName`).
fn caller_class(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    caller: &Expression<'_>,
) -> Option<String> {
    let class = match caller {
        // class context means self|static ::class or __CLASS__
        Expression::Access(Access::ClassConstant(fetch)) => {
            let ClassLikeConstantSelector::Identifier(constant) = &fetch.constant else {
                return None;
            };
            if !constant.value.eq_ignore_ascii_case(b"class") {
                return None;
            }
            match unparenthesize(fetch.class) {
                Expression::Identifier(id) => resolve_class_name(ancestors, id),
                Expression::Self_(_) | Expression::Static(_) => class_scope(ancestors)?.name?,
                _ => return None,
            }
        }
        Expression::MagicConstant(MagicConstant::Class(_)) => class_scope(ancestors)?.name?,
        _ => {
            if let Expression::Variable(Variable::Direct(v)) = caller
                && v.name == b"$this"
            {
                return class_scope(ancestors)?.name;
            }
            let t = facts.full_type(node_span(caller))?;
            return match t.types.as_ref() {
                [TAtomic::Object(TObject::Named(n))] => Some(n.name.to_string()),
                [TAtomic::Object(TObject::Enum(e))] => Some(e.name.to_string()),
                _ => None,
            };
        }
    };
    // `resolveClassContextType`
    let codebase = &facts.codebase;
    codebase.get_class_like(class.as_bytes())?;
    if let Some(constructor) = codebase.get_declaring_method(class.as_bytes(), b"__construct")
        && constructor
            .parameters
            .iter()
            .any(|p| !p.flags.has_default())
    {
        return None;
    }
    Some(class)
}

/// The array's context: what `ContextNodeVisitor` and `CallLikeReflectionNodeVisitor` mark as no callable.
fn skipped_context(
    ctx: &Context<'_>,
    span: crate::problem::Span,
    ancestors: &[Node<'_, '_>],
) -> bool {
    // a class constant, a property default or a parameter default
    if is_in_constant_expression(ancestors)
        || ancestors
            .iter()
            .any(|a| matches!(a, Node::Attribute(_) | Node::EnumCaseBackedItem(_)))
    {
        return true;
    }
    if is_config_value(ctx, ancestors) {
        return true;
    }
    let parent = ancestors
        .iter()
        .rev()
        .find(|a| !matches!(a, Node::Expression(_)));
    if is_return_not_accepting_closure(ctx, ancestors) {
        return true;
    }
    match parent {
        // `processArrayInSerializedCallableKey`
        Some(Node::KeyValueArrayElement(element)) => {
            matches!(unparenthesize(element.key), Expression::Literal(Literal::String(s))
                if s.value.is_some_and(|k| [&b"callback"[..], b"factory", b"validation_groups"].contains(&k)))
        }
        Some(Node::PositionalArgument(_) | Node::NamedArgument(_)) => {
            argument_skipped(ctx, span, ancestors)
        }
        _ => false,
    }
}

/// `IS_RETURN_NOT_ACCEPTING_CLOSURE`: the array is returned (`return [...]`, or an arrow function's body) from a
/// function-like whose native return type takes an array but not a `Closure`.
fn is_return_not_accepting_closure(ctx: &Context<'_>, ancestors: &[Node<'_, '_>]) -> bool {
    let mut up = ancestors
        .iter()
        .rev()
        .filter(|a| !matches!(a, Node::Expression(_) | Node::Parenthesized(_)));
    let hint = match up.next() {
        Some(Node::ArrowFunction(f)) => f.return_type_hint.as_ref(),
        Some(Node::Return(_) | Node::Statement(Statement::Return(_))) => {
            match ancestors.iter().rev().find(|a| {
                matches!(
                    a,
                    Node::Function(_) | Node::Method(_) | Node::Closure(_) | Node::ArrowFunction(_)
                )
            }) {
                Some(Node::Function(f)) => f.return_type_hint.as_ref(),
                Some(Node::Method(m)) => m.return_type_hint.as_ref(),
                Some(Node::Closure(c)) => c.return_type_hint.as_ref(),
                _ => None,
            }
        }
        _ => return false,
    };
    let Some(FunctionLikeReturnTypeHint { hint, .. }) = hint else {
        return false;
    };
    let hint = match hint {
        Hint::Nullable(n) => n.hint,
        h => h,
    };
    let mut names = Vec::new();
    let mut stack = vec![hint];
    while let Some(h) = stack.pop() {
        match h {
            Hint::Union(u) => stack.extend([u.right, u.left]),
            Hint::Intersection(_) | Hint::Parenthesized(_) => return false,
            Hint::Identifier(id) => names.push(
                resolve_class_name(ancestors, id)
                    .trim_start_matches('\\')
                    .to_ascii_lowercase(),
            ),
            other => names.push(text(ctx, other).to_ascii_lowercase()),
        }
    }
    let has = |list: &[&str]| names.iter().any(|n| list.contains(&n.as_str()));
    !has(&["callable", "closure", "mixed", "object"]) && has(&["array", "iterable"])
}

/// Not in Rector: the value a file under a `config/` directory returns, outside any function or class. Laravel's
/// `config:cache` writes it out with `var_export()`, which takes an array callable but not the `Closure` a first-class
/// callable makes.
fn is_config_value(ctx: &Context<'_>, ancestors: &[Node<'_, '_>]) -> bool {
    ctx.path
        .parent()
        .is_some_and(|dir| dir.components().any(|c| c.as_os_str() == "config"))
        && ancestors
            .iter()
            .any(|a| matches!(a, Node::Return(_) | Node::Statement(Statement::Return(_))))
        && !ancestors.iter().any(|a| {
            matches!(
                a,
                Node::Function(_)
                    | Node::Method(_)
                    | Node::Closure(_)
                    | Node::ArrowFunction(_)
                    | Node::Class(_)
                    | Node::AnonymousClass(_)
                    | Node::Trait(_)
                    | Node::Enum(_)
            )
        })
}

fn argument_skipped(
    ctx: &Context<'_>,
    span: crate::problem::Span,
    ancestors: &[Node<'_, '_>],
) -> bool {
    let Some(at) = ancestors.iter().rposition(|a| {
        matches!(
            a,
            Node::FunctionCall(_)
                | Node::MethodCall(_)
                | Node::StaticMethodCall(_)
                | Node::Instantiation(_)
                | Node::NullSafeMethodCall(_)
        )
    }) else {
        return false;
    };
    let (target, list) = match ancestors[at] {
        Node::FunctionCall(call) => {
            if named(call, "register_shutdown_function") || named(call, "forward_static_call") {
                return true;
            }
            (Target::Function(call.function), &call.argument_list)
        }
        Node::MethodCall(call) => {
            // Symfony's Definition::setFactory() does not accept a first class callable
            if let ClassLikeMemberSelector::Identifier(m) = &call.method
                && m.value == b"setFactory"
                && call
                    .argument_list
                    .arguments
                    .first()
                    .is_some_and(|a| node_span(value(a)) == span)
            {
                return true;
            }
            (
                Target::Method(call.object, &call.method),
                &call.argument_list,
            )
        }
        Node::NullSafeMethodCall(call) => (
            Target::Method(call.object, &call.method),
            &call.argument_list,
        ),
        Node::StaticMethodCall(call) => (
            Target::Static(call.class, &call.method),
            &call.argument_list,
        ),
        Node::Instantiation(new) => match &new.argument_list {
            Some(list) => (Target::New(new.class), list),
            None => return false,
        },
        _ => return false,
    };
    let args: Vec<&Argument<'_>> = list.arguments.iter().collect();
    let Some(position) = args.iter().position(|a| node_span(value(a)) == span) else {
        return false;
    };
    if matches!(args[position], Argument::Positional(p) if p.ellipsis.is_some()) {
        // IS_UNPACKED_ARG_VALUE
        return true;
    }
    let facts = facts(ctx);
    let Some(function) = callee(&facts, target, &ancestors[..at]) else {
        return false;
    };
    let params = &function.parameters;
    let param = match args[position] {
        Argument::Named(n) => params
            .iter()
            .find(|p| p.name.0.as_bytes()[1..] == *n.name.value),
        Argument::Positional(_) => params
            .get(position)
            .or_else(|| params.last().filter(|p| p.flags.is_variadic())),
    };
    let Some(param) = param else {
        return false;
    };
    let Some(t) = param.type_metadata.as_ref().map(|t| &t.type_union) else {
        return false;
    };
    // `$parameterReflection->getType()->accepts(new ObjectType('Closure'), true)->no()`
    t.types
        .iter()
        .all(|a| rejects_closure(ctx, &facts.codebase, a))
}

fn rejects_closure(ctx: &Context<'_>, codebase: &CodebaseMetadata, atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::Null | TAtomic::Scalar(_) | TAtomic::Array(_) | TAtomic::Void | TAtomic::Never => {
            true
        }
        TAtomic::Object(TObject::Enum(_)) => true,
        TAtomic::Object(TObject::Named(n)) => {
            codebase.get_class_like(n.name.as_bytes()).is_some()
                && !is_a(ctx, "Closure", &[&n.name.to_string()])
        }
        _ => false,
    }
}
