//! Rector 2.8.4's `ArrowFunctionAndClosureFirstClassCallableGuard`, which `ArrowFunctionDelegatingCallToFirstClassCallableRector`
//! and `ClosureDelegatingCallToFirstClassCallableRector` share, with the closure context `CallLikeReflectionNodeVisitor`
//! and `AssignedToNodeVisitor` decorate, and the rewrite both make.

use mago_codex::metadata::function_like::FunctionLikeMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{
    Argument, ArgumentList, AssignmentOperator, Call, ClassLikeMemberSelector, Expression,
    FunctionLikeParameterList, Node, Variable,
};

use crate::problem::{Edit, Safety, Span};
use crate::rules::complexity::require_named_argument_order::{Target, callee};
use crate::rules::support::arguments::{unpacked, value};
use crate::rules::support::statements::line_indent;
use crate::rules::types::class_scope;
use crate::rules::types::facts::{Facts, facts};
use crate::rules::types::names::resolve_class_name;
use crate::syntax::{node_span, same_code, unparenthesize};
use crate::{Context, Report};

/// The delegated call of a closure or arrow function, and the report that turns it into a first-class callable.
pub(super) fn refactor(
    ctx: &Context<'_>,
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
    params: &FunctionLikeParameterList<'_>,
    call: &Expression<'_>,
    out: &mut Vec<Report>,
) {
    let call = unparenthesize(call);
    let (list, target) = match call {
        Expression::Call(Call::Function(c)) => {
            // dynamic name? skip
            if !matches!(c.function, Expression::Identifier(_)) {
                return;
            }
            (&c.argument_list, Target::Function(c.function))
        }
        Expression::Call(Call::Method(c)) => {
            if !matches!(c.method, ClassLikeMemberSelector::Identifier(_)) {
                return;
            }
            (&c.argument_list, Target::Method(c.object, &c.method))
        }
        Expression::Call(Call::StaticMethod(c)) => {
            if !matches!(c.method, ClassLikeMemberSelector::Identifier(_)) {
                return;
            }
            (&c.argument_list, Target::Static(c.class, &c.method))
        }
        _ => return,
    };
    if should_skip(ctx, node, ancestors, params, call, list, target) {
        return;
    }
    let source = ctx.source;
    let span = node_span(&node);
    let call_span = node_span(call);
    let args = &list.arguments;
    let code = match (args.first(), args.last()) {
        (Some(first), Some(last)) => {
            let mut end = node_span(last).end;
            if let Some(comma) = list.arguments.get_trailing_token() {
                end = comma.start.offset as usize + comma.value.len();
            }
            format!(
                "{}...{}",
                &source[call_span.start..node_span(first).start],
                &source[end..call_span.end]
            )
        }
        _ => format!(
            "{}(...)",
            &source[call_span.start..list.left_parenthesis.start.offset as usize]
        ),
    };
    // The call moves to the closure's place: its other lines keep their indentation relative to it.
    let (from, to) = (
        line_indent(source, call_span.start),
        line_indent(source, span.start),
    );
    let code = code.replace(&format!("\n{from}"), &format!("\n{to}"));
    out.push(Report::fixed(
        span,
        "Use a first-class callable instead of a closure that only delegates the call.",
        Safety::Unsafe,
        vec![Edit::new(span, code)],
    ));
}

fn should_skip(
    ctx: &Context<'_>,
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
    params: &FunctionLikeParameterList<'_>,
    call: &Expression<'_>,
    list: &ArgumentList<'_>,
    target: Target<'_>,
) -> bool {
    let facts = facts(ctx);
    let span = node_span(&node);
    // use cheap checks first
    if has_closure_with_variadic_args(&facts, span, ancestors)
        || is_assigned_to(span, ancestors)
        || is_rebound(span, ancestors)
    {
        return true;
    }
    let params: Vec<_> = params.parameters.iter().collect();
    let args: Vec<&Argument<'_>> = list.arguments.iter().collect();
    if params.len() != args.len() {
        return true;
    }
    if is_chained_call(call) {
        return true;
    }
    if args.iter().any(|a| matches!(a, Argument::Named(_))) {
        return true;
    }
    if params.iter().any(|p| p.ampersand.is_some()) {
        return true;
    }
    // isNotUsingSameParamsForArgs
    if args.len() == 1 && unpacked(args[0]) {
        if params[0].ellipsis.is_none() {
            return true;
        }
    } else {
        for (key, arg) in args.iter().enumerate() {
            if !same_code(
                ctx.source,
                node_span(value(arg)),
                node_span(&params[key].variable),
            ) {
                return true;
            }
            if let Expression::Variable(Variable::Direct(v)) = value(arg)
                && params
                    .iter()
                    .any(|p| p.variable.name == v.name && p.ellipsis.is_some())
                && !unpacked(arg)
            {
                return true;
            }
        }
    }
    // isDependantMethod
    let receiver = match call {
        Expression::Call(Call::Method(c)) => Some(c.object),
        Expression::Call(Call::StaticMethod(c)) => Some(c.class),
        _ => None,
    };
    if let Some(receiver) = receiver
        && params
            .iter()
            .any(|p| contains_variable(Node::Expression(receiver), p.variable.name))
    {
        return true;
    }
    // isUsingThisInNonObjectContext
    if matches!(call, Expression::Call(Call::Method(_)))
        && !this_defined(node, ancestors)
        && contains_variable(Node::Expression(call), b"$this")
    {
        return true;
    }
    // does not exists, probably by magic method
    let Some(function) = resolve(&facts, target, ancestors) else {
        return true;
    };
    // phpstan reports first class callables that are not native methods
    if function.flags.is_magic_method() {
        return true;
    }
    // check if args require by reference
    if function
        .parameters
        .iter()
        .any(|p| p.flags.is_by_reference())
    {
        return true;
    }
    if function.flags.is_built_in() {
        return false;
    }
    function.parameters.len() != args.len()
}

/// `resolveFunctionLikeReflectionFromCall`: a method is read on the object's inferred type.
fn resolve<'f>(
    facts: &'f Facts,
    target: Target<'_>,
    ancestors: &[Node<'_, '_>],
) -> Option<&'f FunctionLikeMetadata> {
    if let Target::Method(object, ClassLikeMemberSelector::Identifier(method)) = target {
        let class = match unparenthesize(object) {
            Expression::Variable(Variable::Direct(v)) if v.name == b"$this" => {
                class_scope(ancestors)?.name?
            }
            object => {
                let t = facts.full_type(node_span(object))?;
                match t.types.as_ref() {
                    [TAtomic::Object(TObject::Named(n))] => n.name.to_string(),
                    _ => return None,
                }
            }
        };
        return facts
            .codebase
            .get_declaring_method(class.as_bytes(), method.value);
    }
    if let Target::Static(Expression::Identifier(id), ClassLikeMemberSelector::Identifier(method)) =
        target
    {
        let class = resolve_class_name(ancestors, id);
        return facts
            .codebase
            .get_declaring_method(class.as_bytes(), method.value);
    }
    callee(facts, target, ancestors)
}

/// `HAS_CLOSURE_WITH_VARIADIC_ARGS`: a closure passed to anything but a built-in function, or the first closure passed
/// to a built-in function that takes a variadic callable.
fn has_closure_with_variadic_args(facts: &Facts, span: Span, ancestors: &[Node<'_, '_>]) -> bool {
    let Some(at) = ancestors
        .iter()
        .rposition(|a| !matches!(a, Node::Expression(_) | Node::Parenthesized(_)))
    else {
        return false;
    };
    if !matches!(
        ancestors[at],
        Node::PositionalArgument(_) | Node::NamedArgument(_)
    ) {
        return false;
    }
    let Some(call_at) = ancestors[..at].iter().rposition(|a| {
        matches!(
            a,
            Node::FunctionCall(_)
                | Node::MethodCall(_)
                | Node::NullSafeMethodCall(_)
                | Node::StaticMethodCall(_)
                | Node::Instantiation(_)
        )
    }) else {
        return false;
    };
    let (target, list) = match ancestors[call_at] {
        Node::FunctionCall(c) => (Some(Target::Function(c.function)), &c.argument_list),
        Node::MethodCall(c) => (None, &c.argument_list),
        Node::NullSafeMethodCall(c) => (None, &c.argument_list),
        Node::StaticMethodCall(c) => (None, &c.argument_list),
        Node::Instantiation(n) => match &n.argument_list {
            Some(list) => (None, list),
            None => return false,
        },
        _ => return false,
    };
    let Some(function) = target.and_then(|t| callee(facts, t, &ancestors[..call_at])) else {
        return true;
    };
    if !function.flags.is_built_in() {
        return true;
    }
    let first = list.arguments.iter().find(|a| {
        matches!(
            unparenthesize(value(a)),
            Expression::Closure(_) | Expression::ArrowFunction(_)
        )
    });
    if first.is_none_or(|a| node_span(unparenthesize(value(a))) != span) {
        return false;
    }
    // PHPStan's reflection of the built-in function: a plain `callable` takes any arguments.
    pun_phpstan::symbols::function(&function.name.to_string()).is_some_and(|f| {
        (f.variants.iter().flat_map(|v| &v.params))
            .any(|p| p.ty == "callable" || (p.ty.starts_with("callable(") && p.ty.contains("...")))
    })
}

/// Rector 2.9.2's `IS_REBOUND_CLOSURE`: `(fn () => ...)->call($object)` or `->bindTo(...)` rebinds `$this`.
fn is_rebound(span: Span, ancestors: &[Node<'_, '_>]) -> bool {
    ancestors
        .iter()
        .rev()
        .find(|a| !matches!(a, Node::Expression(_) | Node::Parenthesized(_)))
        .is_some_and(|a| {
            matches!(a, Node::MethodCall(call)
                if node_span(unparenthesize(call.object)) == span
                    && matches!(&call.method, ClassLikeMemberSelector::Identifier(id)
                        if id.value.eq_ignore_ascii_case(b"call") || id.value.eq_ignore_ascii_case(b"bindto")))
        })
}

/// `IS_ASSIGNED_TO`: the value of a plain `=` assignment.
fn is_assigned_to(span: Span, ancestors: &[Node<'_, '_>]) -> bool {
    ancestors
        .iter()
        .rev()
        .find(|a| !matches!(a, Node::Expression(_) | Node::Parenthesized(_)))
        .is_some_and(|a| {
            matches!(a, Node::Assignment(assign)
                if matches!(assign.operator, AssignmentOperator::Assign(_))
                    && node_span(unparenthesize(assign.rhs)) == span)
        })
}

fn is_chained_call(call: &Expression<'_>) -> bool {
    let receiver = match call {
        Expression::Call(Call::Method(c)) => c.object,
        Expression::Call(Call::StaticMethod(c)) => c.class,
        _ => return false,
    };
    matches!(
        unparenthesize(receiver),
        Expression::Call(_) | Expression::Instantiation(_)
    )
}

fn contains_variable(node: Node<'_, '_>, name: &[u8]) -> bool {
    if let Node::DirectVariable(v) = node
        && v.name == name
    {
        return true;
    }
    let mut found = false;
    node.visit_children(|child| found = found || contains_variable(child, name));
    found
}

/// `in_array('this', $scope->getDefinedVariables())` in the closure: inside a non-static method, through closures and
/// arrow functions that are not static, the closure's own included.
fn this_defined(node: Node<'_, '_>, ancestors: &[Node<'_, '_>]) -> bool {
    for a in std::iter::once(&node).chain(ancestors.iter().rev()) {
        match a {
            Node::Closure(c) => {
                if c.r#static.is_some() {
                    return false;
                }
            }
            Node::ArrowFunction(f) => {
                if f.r#static.is_some() {
                    return false;
                }
            }
            Node::Method(m) => {
                return !m
                    .modifiers
                    .iter()
                    .any(|m| matches!(m, mago_syntax::cst::Modifier::Static(_)));
            }
            Node::Function(_) | Node::Program(_) => return false,
            _ => {}
        }
    }
    false
}
