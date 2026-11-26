use crate::problem::{Fix, Safety};
use mago_syntax::cst::*;

use crate::rules::support::rewrites::replace_with;
use crate::rules::support::statement_edits::if_view;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoDeadPhpVersionCheck;

impl Rule for NoDeadPhpVersionCheck {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-dead-php-version-check"
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
        let Some(runs) = decided(r#if.condition, i64::from(ctx.php_version.id())) else {
            return;
        };
        let source = ctx.source;
        let view = if_view(source, r#if);
        if !runs && (view.else_.is_some() || !view.elseifs.is_empty()) {
            return;
        }
        let version = i64::from(ctx.php_version.id());
        let edits = replace_with(ctx, s, ancestors, runs.then_some(&view.stmts), |other| {
            let Statement::If(i) = other else {
                return false;
            };
            let view = if_view(source, i);
            decided(i.condition, version)
                .is_some_and(|runs| runs || (view.else_.is_none() && view.elseifs.is_empty()))
        });
        out.push(Report {
            span: node_span(s),
            message: "This `PHP_VERSION_ID` check is decided by the project's minimum PHP version."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

fn decided(cond: &Expression<'_>, version: i64) -> Option<bool> {
    let Expression::Binary(b) = unparenthesize(cond) else {
        return None;
    };
    let (left, right) = (unparenthesize(b.lhs), unparenthesize(b.rhs));
    let on_left = is_php_version_id(left);
    if !on_left && !is_php_version_id(right) {
        return None;
    }
    let value = integer(if on_left { right } else { left })?;
    // Rector 2.9.2 keeps a check that can still fail on the minimum version (`80000 < PHP_VERSION_ID` on 8.0).
    match (&b.operator, on_left) {
        (BinaryOperator::LessThan(_), true) | (BinaryOperator::GreaterThan(_), false) => {
            (version >= value).then_some(false)
        }
        (BinaryOperator::GreaterThanOrEqual(_), false) => (version > value).then_some(false),
        (BinaryOperator::LessThan(_), false) | (BinaryOperator::GreaterThan(_), true) => {
            (version > value).then_some(true)
        }
        (BinaryOperator::GreaterThanOrEqual(_), true) => (version >= value).then_some(true),
        _ => None,
    }
}

fn is_php_version_id(expr: &Expression<'_>) -> bool {
    let Expression::ConstantAccess(c) = expr else {
        return false;
    };
    let name = c.name.value();
    name.strip_prefix(b"\\")
        .unwrap_or(name)
        .eq_ignore_ascii_case(b"PHP_VERSION_ID")
}

fn integer(expr: &Expression<'_>) -> Option<i64> {
    match expr {
        Expression::Literal(Literal::Integer(i)) => i64::try_from(i.value?).ok(),
        _ => None,
    }
}
