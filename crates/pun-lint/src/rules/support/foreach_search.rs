use crate::problem::{Edit, Span};
use mago_syntax::cst::{
    Expression, Foreach, ForeachBody, ForeachTarget, Node, Statement, Variable,
};

use crate::Context;
use crate::rules::support::expressions::{equal, is_constant, operand, text};
use crate::rules::support::phpdoc_writer::native_expr_type;
use crate::rules::support::side_effects::writes_variable;
use crate::rules::support::statement_edits::replace_run;
use crate::rules::support::statement_edits::{List, body_list, expression, holder_list, if_view};
use crate::rules::support::values::is_native_array;
use crate::rules::support::variable_usage::used_in;
use crate::syntax::{node_span, unparenthesize};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    All,
    Any,
    Find,
    FindKey,
}

impl Kind {
    fn function(self) -> &'static str {
        match self {
            Kind::All => "array_all",
            Kind::Any => "array_any",
            Kind::Find => "array_find",
            Kind::FindKey => "array_find_key",
        }
    }
}

pub(crate) fn refactor(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    list: &List<'_>,
    kind: Kind,
) -> Option<(Vec<Edit>, Span)> {
    let assignment = assignment_pattern(ctx, ancestors, list, kind);
    if assignment.is_some() || matches!(kind, Kind::Find | Kind::FindKey) {
        return assignment;
    }
    early_return_pattern(ctx, ancestors, list, kind)
}

struct Loop<'a> {
    subject: &'a Expression<'a>,
    key: Option<&'a Expression<'a>>,
    value: &'a Expression<'a>,
    condition: &'a Expression<'a>,
    body: Vec<&'a Statement<'a>>,
}

fn parts<'a>(ctx: &Context<'_>, foreach: &'a Foreach<'a>) -> Option<Loop<'a>> {
    let stmts = match &foreach.body {
        ForeachBody::Statement(s) => body_list(ctx.source, s),
        ForeachBody::ColonDelimited(b) => {
            holder_list(ctx.source, Node::ForeachColonDelimitedBody(b), &[])?
        }
    };
    if stmts.len() != 1 {
        return None;
    }
    let Statement::If(r#if) = stmts.stmts[0] else {
        return None;
    };
    let view = if_view(ctx.source, r#if);
    // Rector 2.9.2: the arrow function captures by value, so a write in the condition would be lost.
    if view.else_.is_some()
        || !view.elseifs.is_empty()
        || view.stmts.nop
        || writes_variable(view.condition)
    {
        return None;
    }
    let (key, value) = match &foreach.target {
        ForeachTarget::Value(t) => (None, t.value),
        ForeachTarget::KeyValue(t) => (Some(t.key), t.value),
    };
    Some(Loop {
        subject: foreach.expression,
        key,
        value,
        condition: view.condition,
        body: view.stmts.stmts.clone(),
    })
}

/// Rector's `getNativeType($foreach->expr)->isArray()->yes()`: an array the code shows (a hint, a literal), or a
/// native type that is an array, such as `$this->items()` of a method declared `: array`. A variable keeps the first
/// test only: Pun/any_referenced leaves one with a reference alias alone.
fn array_subject(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], l: &Loop<'_>, at: usize) -> bool {
    is_native_array(ancestors, l.subject, at)
        || (!matches!(l.subject, Expression::Variable(_))
            && native_expr_type(ctx, l.subject).is_some_and(|t| t.is_array().yes()))
}

fn is_variable(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::Variable(Variable::Direct(_)))
}

fn params(ctx: &Context<'_>, l: &Loop<'_>) -> String {
    let mut params = text(ctx, l.value).to_string();
    if let Some(key) = l.key
        && let Expression::Variable(Variable::Direct(k)) = key
        && used_in(Node::Expression(l.condition), None, &k.name[1..])
    {
        params.push_str(", ");
        params.push_str(text(ctx, key));
    }
    params
}

fn call(ctx: &Context<'_>, kind: Kind, l: &Loop<'_>, negate: bool) -> String {
    let body = if negate {
        match unparenthesize(l.condition) {
            Expression::UnaryPrefix(u) if u.operator.is_not() => {
                text(ctx, unparenthesize(u.operand)).to_string()
            }
            c => format!("!{}", operand(ctx, c, 30, None)),
        }
    } else {
        text(ctx, unparenthesize(l.condition)).to_string()
    };
    format!(
        "{}({}, fn({}) => {body})",
        kind.function(),
        text(ctx, unparenthesize(l.subject)),
        params(ctx, l)
    )
}

fn assignment_pattern(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    list: &List<'_>,
    kind: Kind,
) -> Option<(Vec<Edit>, Span)> {
    for (key, stmt) in list.stmts.iter().enumerate() {
        let Statement::Foreach(foreach) = stmt else {
            continue;
        };
        let Some(prev) = key.checked_sub(1).and_then(|k| expression(list.stmts[k])) else {
            continue;
        };
        let Expression::Assignment(prev_assign) = prev else {
            continue;
        };
        if !crate::rules::support::statement_edits::is_plain_assign(prev_assign) {
            continue;
        }
        let init = match kind {
            Kind::All => "true",
            Kind::Any => "false",
            Kind::Find | Kind::FindKey => "null",
        };
        if !is_constant(prev_assign.rhs, init) || !is_variable(unparenthesize(prev_assign.lhs)) {
            continue;
        }
        let Some(l) = parts(ctx, foreach) else {
            continue;
        };
        if !valid_assignment(ctx, ancestors, foreach, &l, prev_assign.lhs, kind) {
            continue;
        }
        let Expression::Variable(Variable::Direct(value)) = l.value else {
            continue;
        };
        if list.stmts[key + 1..]
            .iter()
            .any(|s| used_in(Node::Statement(s), None, &value.name[1..]))
        {
            continue;
        }
        let new = format!(
            "{} = {};",
            text(ctx, unparenthesize(prev_assign.lhs)),
            call(ctx, kind, &l, kind == Kind::All)
        );
        let edit = replace_run(ctx, list, key - 1, key, &new);
        return Some((
            vec![edit],
            Span::new(node_span(list.stmts[key - 1]).start, node_span(*stmt).end),
        ));
    }
    None
}

fn valid_assignment(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    foreach: &Foreach<'_>,
    l: &Loop<'_>,
    assigned: &Expression<'_>,
    kind: Kind,
) -> bool {
    if kind == Kind::FindKey && l.key.is_none() {
        return false;
    }
    let [assignment, brk] = l.body[..] else {
        return false;
    };
    let Some(Expression::Assignment(a)) = expression(assignment) else {
        return false;
    };
    if !crate::rules::support::statement_edits::is_plain_assign(a)
        || !matches!(brk, Statement::Break(_))
        || !equal(ctx, a.lhs, assigned)
    {
        return false;
    }
    let found = match kind {
        Kind::All => is_constant(a.rhs, "false"),
        Kind::Any => is_constant(a.rhs, "true"),
        Kind::Find => equal(ctx, a.rhs, l.value),
        Kind::FindKey => equal(ctx, a.rhs, l.key.unwrap()),
    };
    found && array_subject(ctx, ancestors, l, node_span(foreach).start)
}

fn early_return_pattern(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    list: &List<'_>,
    kind: Kind,
) -> Option<(Vec<Edit>, Span)> {
    let (inside, after) = if kind == Kind::All {
        ("false", "true")
    } else {
        ("true", "false")
    };
    for (key, stmt) in list.stmts.iter().enumerate() {
        let Statement::Foreach(foreach) = stmt else {
            continue;
        };
        let Some(Statement::Return(next)) = list.stmts.get(key + 1) else {
            continue;
        };
        if !next.value.is_some_and(|v| is_constant(v, after)) {
            continue;
        }
        let Some(l) = parts(ctx, foreach) else {
            continue;
        };
        let [Statement::Return(r)] = l.body[..] else {
            continue;
        };
        if !r.value.is_some_and(|v| is_constant(v, inside))
            || !is_variable(l.value)
            || !array_subject(ctx, ancestors, &l, node_span(foreach).start)
        {
            continue;
        }
        let new = format!("return {};", call(ctx, kind, &l, kind == Kind::All));
        let edit = replace_run(ctx, list, key, key + 1, &new);
        return Some((
            vec![edit],
            Span::new(node_span(*stmt).start, node_span(next).end),
        ));
    }
    None
}
