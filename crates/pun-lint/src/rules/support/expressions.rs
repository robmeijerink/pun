use crate::problem::{Edit, Span};
use mago_span::HasSpan;
use mago_syntax::cst::{
    Argument, BinaryOperator, Call, Construct, Expression, ExpressionStatement, FunctionCall,
    Identifier, Literal, Node, UnaryPrefixOperator, Yield,
};

use crate::Context;
use crate::syntax::{node_span, same_code, unparenthesize};

pub(crate) fn call_to<'a>(expr: &'a Expression<'a>, name: &str) -> Option<&'a FunctionCall<'a>> {
    match unparenthesize(expr) {
        Expression::Call(Call::Function(call)) if named(call, name) => Some(call),
        _ => None,
    }
}

pub(crate) fn named(call: &FunctionCall<'_>, name: &str) -> bool {
    let called = match call.function {
        Expression::Identifier(Identifier::Local(id)) => id.value,
        Expression::Identifier(Identifier::FullyQualified(id)) => &id.value[1..],
        _ => return false,
    };
    called.eq_ignore_ascii_case(name.as_bytes())
}

pub(crate) fn args<'a>(call: &'a FunctionCall<'a>) -> Vec<&'a Argument<'a>> {
    call.argument_list.arguments.iter().collect()
}

pub(crate) fn equal(ctx: &Context<'_>, a: &Expression<'_>, b: &Expression<'_>) -> bool {
    same_code(
        ctx.source,
        node_span(unparenthesize(a)),
        node_span(unparenthesize(b)),
    )
}

pub(crate) fn is_constant(expr: &Expression<'_>, name: &str) -> bool {
    let constant: &[u8] = match unparenthesize(expr) {
        Expression::Literal(Literal::True(_)) => b"true",
        Expression::Literal(Literal::False(_)) => b"false",
        Expression::Literal(Literal::Null(_)) => b"null",
        Expression::ConstantAccess(c) => c.name.value(),
        _ => return false,
    };
    let constant = constant.strip_prefix(b"\\").unwrap_or(constant);
    constant.eq_ignore_ascii_case(name.as_bytes())
}

pub(crate) fn inverse(op: &BinaryOperator<'_>) -> Option<&'static str> {
    Some(match op {
        BinaryOperator::Identical(_) => "!==",
        BinaryOperator::NotIdentical(_) => "===",
        BinaryOperator::Equal(_) => "!=",
        BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_) => "==",
        BinaryOperator::GreaterThan(_) => "<=",
        BinaryOperator::LessThan(_) => ">=",
        BinaryOperator::GreaterThanOrEqual(_) => "<",
        BinaryOperator::LessThanOrEqual(_) => ">",
        _ => return None,
    })
}

/// Rector 2.9.2's `BinaryOpManipulator::resolveSafeInversedClass()`: [`inverse`], but an ordering comparison inverts
/// only when both operands' native types are `int` (NAN, arrays and objects are neither smaller nor greater).
pub(crate) fn safe_inverse(
    ctx: &Context<'_>,
    op: &BinaryOperator<'_>,
    lhs: &Expression<'_>,
    rhs: &Expression<'_>,
) -> Option<&'static str> {
    let inverse = inverse(op)?;
    let ordering = matches!(
        op,
        BinaryOperator::GreaterThan(_)
            | BinaryOperator::LessThan(_)
            | BinaryOperator::GreaterThanOrEqual(_)
            | BinaryOperator::LessThanOrEqual(_)
    );
    let int = |e: &Expression<'_>| {
        crate::rules::types::facts::facts(ctx)
            .native_type(node_span(unparenthesize(e)))
            .is_some_and(|t| !t.types.is_empty() && t.is_int())
    };
    (!ordering || (int(lhs) && int(rhs))).then_some(inverse)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Prec {
    Infix(i32, i32, i32),
    Prefix(i32),
    None,
}

pub(crate) fn prec(expr: &Expression<'_>) -> Prec {
    match unparenthesize(expr) {
        Expression::Binary(b) => {
            let (p, l, r) = binary(&b.operator);
            Prec::Infix(p, l, r)
        }
        Expression::UnaryPrefix(u) => unary(&u.operator).map_or(Prec::None, Prec::Prefix),
        Expression::Pipe(_) => Prec::Infix(65, 66, 65),
        Expression::Conditional(_) => Prec::Infix(150, 150, 150),
        Expression::Assignment(_) => Prec::Prefix(160),
        Expression::Yield(Yield::From(_)) => Prec::Prefix(170),
        Expression::Yield(_) => Prec::Prefix(175),
        Expression::Construct(Construct::Print(_)) => Prec::Prefix(180),
        Expression::Construct(
            Construct::Include(_)
            | Construct::IncludeOnce(_)
            | Construct::Require(_)
            | Construct::RequireOnce(_),
        ) => Prec::Prefix(220),
        Expression::ArrowFunction(_) => Prec::Prefix(230),
        Expression::Throw(_) => Prec::Prefix(240),
        _ => Prec::None,
    }
}

fn unary(op: &UnaryPrefixOperator<'_>) -> Option<i32> {
    match op {
        UnaryPrefixOperator::Not(_) => Some(30),
        UnaryPrefixOperator::PreIncrement(_)
        | UnaryPrefixOperator::PreDecrement(_)
        | UnaryPrefixOperator::Reference(_) => None,
        _ => Some(10),
    }
}

pub(crate) fn binary(op: &BinaryOperator<'_>) -> (i32, i32, i32) {
    match op {
        BinaryOperator::Exponentiation(_) => (0, 0, 1),
        BinaryOperator::Instanceof(_) => (20, 20, 20),
        BinaryOperator::Multiplication(_)
        | BinaryOperator::Division(_)
        | BinaryOperator::Modulo(_) => (40, 41, 40),
        BinaryOperator::Addition(_)
        | BinaryOperator::Subtraction(_)
        | BinaryOperator::StringConcat(_) => (50, 51, 50),
        BinaryOperator::LeftShift(_) | BinaryOperator::RightShift(_) => (60, 61, 60),
        BinaryOperator::LessThan(_)
        | BinaryOperator::LessThanOrEqual(_)
        | BinaryOperator::GreaterThan(_)
        | BinaryOperator::GreaterThanOrEqual(_) => (70, 70, 70),
        BinaryOperator::Equal(_)
        | BinaryOperator::NotEqual(_)
        | BinaryOperator::AngledNotEqual(_)
        | BinaryOperator::Identical(_)
        | BinaryOperator::NotIdentical(_)
        | BinaryOperator::Spaceship(_) => (80, 80, 80),
        BinaryOperator::BitwiseAnd(_) => (90, 91, 90),
        BinaryOperator::BitwiseXor(_) => (100, 101, 100),
        BinaryOperator::BitwiseOr(_) => (110, 111, 110),
        BinaryOperator::And(_) => (120, 121, 120),
        BinaryOperator::Or(_) => (130, 131, 130),
        BinaryOperator::NullCoalesce(_) => (140, 140, 141),
        BinaryOperator::LowAnd(_) => (190, 191, 190),
        BinaryOperator::LowXor(_) => (200, 201, 200),
        BinaryOperator::LowOr(_) => (210, 211, 210),
    }
}

pub(crate) fn needs_parens(child: Prec, limit: i32, lhs: Option<i32>) -> bool {
    match child {
        Prec::Infix(p, ..) => p >= limit,
        Prec::Prefix(p) => lhs.is_some_and(|l| p >= l),
        Prec::None => false,
    }
}

pub(crate) fn wrap(code: &str, p: Prec, limit: i32, lhs: Option<i32>) -> String {
    if needs_parens(p, limit, lhs) {
        format!("({code})")
    } else {
        code.to_string()
    }
}

pub(crate) fn operand(
    ctx: &Context<'_>,
    expr: &Expression<'_>,
    limit: i32,
    lhs: Option<i32>,
) -> String {
    let inner = unparenthesize(expr);
    wrap(text(ctx, inner), prec(inner), limit, lhs)
}

pub(crate) fn infix(
    ctx: &Context<'_>,
    left: &Expression<'_>,
    op: &str,
    right: &Expression<'_>,
    (l, r): (i32, i32),
) -> String {
    format!(
        "{} {op} {}",
        operand(ctx, left, l, Some(l)),
        operand(ctx, right, r, None)
    )
}

pub(crate) fn inverse_node(
    ctx: &Context<'_>,
    expr: &Expression<'_>,
    lhs: Option<i32>,
) -> (String, Prec) {
    match unparenthesize(expr) {
        Expression::Binary(b) if safe_inverse(ctx, &b.operator, b.lhs, b.rhs).is_some() => {
            let (p, l, r) = binary(&b.operator);
            let inverse = safe_inverse(ctx, &b.operator, b.lhs, b.rhs).unwrap();
            let code = infix(ctx, b.lhs, inverse, b.rhs, (l, r));
            (code, Prec::Infix(p, l, r))
        }
        Expression::UnaryPrefix(u) if u.operator.is_not() => {
            let inner = unparenthesize(u.operand);
            (text(ctx, inner).to_string(), prec(inner))
        }
        inner => (
            format!("!{}", operand(ctx, inner, 30, lhs)),
            Prec::Prefix(30),
        ),
    }
}

pub(crate) fn de_morgan(
    ctx: &Context<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    op: &str,
    (l, r): (i32, i32),
) -> String {
    let (left, lp) = inverse_node(ctx, left, Some(l));
    let (right, rp) = inverse_node(ctx, right, None);
    format!(
        "{} {op} {}",
        wrap(&left, lp, l, Some(l)),
        wrap(&right, rp, r, None)
    )
}

pub(crate) fn text<'s>(ctx: &Context<'s>, node: &impl HasSpan) -> &'s str {
    let s = node_span(node);
    &ctx.source[s.start..s.end]
}

fn parent<'n, 'ast, 'arena>(
    span: Span,
    ancestors: &'n [Node<'ast, 'arena>],
) -> Option<&'n Node<'ast, 'arena>> {
    ancestors.iter().rev().find(|a| node_span(*a) != span)
}

pub(crate) fn in_parent(span: Span, ancestors: &[Node<'_, '_>], code: String, new: Prec) -> String {
    let parent = parent(span, ancestors);
    let at_start = parent.is_some_and(|p| node_span(p).start == span.start);
    let wrap = match parent {
        Some(Node::Binary(b)) => {
            let (_, l, r) = binary(&b.operator);
            if at_start {
                needs_parens(new, l, Some(l))
            } else {
                needs_parens(new, r, None)
            }
        }
        Some(Node::UnaryPrefix(u)) => {
            needs_parens(new, unary(&u.operator).unwrap_or(i32::MAX), None)
        }
        Some(Node::Conditional(c)) => {
            c.then.is_none_or(|t| node_span(t) != span)
                && needs_parens(new, 150, at_start.then_some(150))
        }
        Some(Node::Assignment(_)) => needs_parens(new, 160, None),
        Some(
            Node::ArrayAccess(_)
            | Node::ArrayAppend(_)
            | Node::PropertyAccess(_)
            | Node::NullSafePropertyAccess(_)
            | Node::MethodCall(_)
            | Node::NullSafeMethodCall(_)
            | Node::StaticMethodCall(_)
            | Node::StaticPropertyAccess(_)
            | Node::ClassConstantAccess(_)
            | Node::FunctionCall(_),
        ) => at_start && new != Prec::None,
        _ => false,
    };
    if wrap { format!("({code})") } else { code }
}

pub(crate) fn statements_edit(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    statement: &ExpressionStatement<'_>,
    parts: &[String],
) -> Option<Edit> {
    let span = node_span(statement.expression);
    let indent = indent(ctx, span.start);
    if parts.len() == 1 || holds_statements(ancestors[ancestors.len() - 2]) {
        return Some(Edit::new(span, parts.join(&format!(";\n{indent}"))));
    }
    let span = node_span(statement);
    if !text(ctx, statement).ends_with(';') {
        return None;
    }
    let body: String = parts
        .iter()
        .map(|p| format!("{indent}    {p};\n"))
        .collect();
    Some(Edit::new(span, format!("{{\n{body}{indent}}}")))
}

pub(crate) fn moved(
    ctx: &Context<'_>,
    span: Span,
    ancestors: &[Node<'_, '_>],
    value: &Expression<'_>,
) -> String {
    let value = unparenthesize(value);
    let code = text(ctx, value).to_string();
    if let (Some(Node::Binary(parent)), Expression::Binary(b)) = (parent(span, ancestors), value)
        && std::mem::discriminant(&parent.operator) == std::mem::discriminant(&b.operator)
        && node_span(parent.rhs) == span
        && matches!(
            b.operator,
            BinaryOperator::StringConcat(_)
                | BinaryOperator::And(_)
                | BinaryOperator::Or(_)
                | BinaryOperator::BitwiseAnd(_)
                | BinaryOperator::BitwiseOr(_)
                | BinaryOperator::BitwiseXor(_)
        )
    {
        return code;
    }
    in_parent(span, ancestors, code, prec(value))
}

fn holds_statements(node: Node<'_, '_>) -> bool {
    matches!(
        node,
        Node::Program(_)
            | Node::Block(_)
            | Node::NamespaceImplicitBody(_)
            | Node::SwitchExpressionCase(_)
            | Node::SwitchDefaultCase(_)
            | Node::IfColonDelimitedBody(_)
            | Node::IfColonDelimitedBodyElseIfClause(_)
            | Node::IfColonDelimitedBodyElseClause(_)
            | Node::ForeachColonDelimitedBody(_)
            | Node::ForColonDelimitedBody(_)
            | Node::WhileColonDelimitedBody(_)
            | Node::DeclareColonDelimitedBody(_)
    )
}

fn indent<'s>(ctx: &Context<'s>, offset: usize) -> &'s str {
    let line = ctx.source[..offset].rfind('\n').map_or(0, |i| i + 1);
    let rest = &ctx.source[line..];
    &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
}

pub(crate) fn dereferenceable(source: &str, expr: &Expression<'_>) -> String {
    let expr = unparenthesize(expr);
    let code = &source[node_span(expr).start..node_span(expr).end];
    match expr {
        Expression::Variable(_)
        | Expression::Identifier(_)
        | Expression::ConstantAccess(_)
        | Expression::ArrayAccess(_)
        | Expression::Access(_)
        | Expression::Call(_)
        | Expression::Array(_)
        | Expression::LegacyArray(_)
        | Expression::Literal(Literal::String(_)) => code.to_string(),
        _ => format!("({code})"),
    }
}
