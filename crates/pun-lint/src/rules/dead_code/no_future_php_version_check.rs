use crate::problem::{Fix, Safety};
use mago_syntax::cst::*;
use pun_tokens::php_compat::float_to_string;

use crate::rules::support::expressions::call_to;
use crate::rules::support::rewrites::replace_with;
use crate::rules::support::statement_edits::if_view;
use crate::rules::support::values::{Value, int_version, value};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoFuturePhpVersionCheck;

impl Rule for NoFuturePhpVersionCheck {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-future-php-version-check"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Statement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Statement(s @ Statement::If(r#if)) = node else {
            return;
        };
        let source = ctx.source;
        let view = if_view(source, r#if);
        if !view.elseifs.is_empty() {
            return;
        }
        let version = i64::from(ctx.php_version.id());
        let Some(holds) = evaluate(r#if.condition, version) else {
            return;
        };
        let stmts = if holds {
            Some(&view.stmts)
        } else {
            view.else_.as_ref().map(|(list, _)| list)
        };
        let edits = replace_with(
            ctx,
            s,
            ancestors,
            stmts,
            |other| matches!(other, Statement::If(i) if if_view(source, i).elseifs.is_empty() && evaluate(i.condition, version).is_some()),
        );
        out.push(Report {
            span: node_span(s),
            message: "This PHP version check is decided by the project's minimum PHP version."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Compared {
    Bool(bool),
    Int(i64),
}

fn evaluate(cond: &Expression<'_>, version: i64) -> Option<bool> {
    let cond = unparenthesize(cond);
    let result = if let Some(call) = call_to(cond, "version_compare") {
        compare(call, version)?
    } else {
        let Expression::Binary(b) = cond else {
            return None;
        };
        let (call, other) = match (
            call_to(b.lhs, "version_compare"),
            call_to(b.rhs, "version_compare"),
        ) {
            (Some(call), _) => (call, b.rhs),
            (None, Some(call)) => (call, b.lhs),
            _ => return None,
        };
        let negated = match b.operator {
            BinaryOperator::Identical(_) | BinaryOperator::Equal(_) => false,
            BinaryOperator::NotIdentical(_)
            | BinaryOperator::NotEqual(_)
            | BinaryOperator::AngledNotEqual(_) => true,
            _ => return None,
        };
        let loose = matches!(
            b.operator,
            BinaryOperator::Equal(_)
                | BinaryOperator::NotEqual(_)
                | BinaryOperator::AngledNotEqual(_)
        );
        let result = compare(call, version)?;
        let expected = value(other)?;
        // Rector 2.9.2's ConditionEvaluator compares `==` and `!=` loosely: `true == 1`.
        let same = match (expected, result) {
            (Value::Bool(e), Compared::Bool(r)) => e == r,
            (Value::Int(e), Compared::Int(r)) => e == r,
            (Value::Int(e), Compared::Bool(r)) if loose => (e != 0) == r,
            (Value::Bool(e), Compared::Int(r)) if loose => e == (r != 0),
            _ => false,
        };
        Compared::Bool(same != negated)
    };
    Some(match result {
        Compared::Bool(b) => b,
        Compared::Int(i) => i > 0,
    })
}

fn compare(call: &FunctionCall<'_>, version: i64) -> Option<Compared> {
    let args: Vec<&Expression> = call
        .argument_list
        .arguments
        .iter()
        .map(|a| match a {
            Argument::Positional(a) => a.value,
            Argument::Named(a) => a.value,
        })
        .collect();
    let first = argument_version(args.first()?, version)?;
    let second = argument_version(args.get(1)?, version)?;
    let Some(sign) = args.get(2) else {
        return Some(Compared::Int(
            i64::from(first > second) - i64::from(first < second),
        ));
    };
    let Some(Value::Str(sign)) = value(sign) else {
        return None;
    };
    if sign == "<" && version < second {
        return None;
    }
    Some(Compared::Bool(match sign.as_str() {
        "<" | "lt" => first < second,
        "<=" | "le" => first <= second,
        ">" | "gt" => first > second,
        ">=" | "ge" => first >= second,
        "==" | "eq" => first == second,
        "!=" | "<>" | "ne" => first != second,
        _ => return None,
    }))
}

fn argument_version(expr: &Expression<'_>, version: i64) -> Option<i64> {
    match value(expr)? {
        Value::Constant(name) | Value::Str(name)
            if name == "PHP_VERSION" || name == "PHP_VERSION_ID" =>
        {
            Some(version)
        }
        Value::Str(s) => Some(int_version(&s)),
        Value::Float(f) => Some(int_version(&float_to_string(f))),
        Value::Int(i) => Some(i),
        _ => None,
    }
}
