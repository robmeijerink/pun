use crate::problem::{Edit, Fix, Safety, Span};
use mago_syntax::cst::{BinaryOperator, Expression, Node, NodeKind};

use crate::rules::support::conditions::{is_binary_op, match_first_and_second};
use crate::rules::support::expressions::{
    Prec, binary, in_parent, infix, is_constant, moved, safe_inverse,
};
use crate::rules::support::options::part_on;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct PreferSimpleCondition {
    /// `conditions` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for PreferSimpleCondition {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-simple-condition"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !checks(options, "conditions")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::UnaryPrefix, NodeKind::Binary])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if self.off {
            return;
        }
        let (span, code) = match node {
            Node::UnaryPrefix(not) if not.operator.is_not() => {
                let Some(code) = boolean_not(ctx, not.operand) else {
                    return;
                };
                let span = node_span(not);
                (span, in_parent(span, ancestors, code.0, code.1))
            }
            Node::Binary(b) if matches!(b.operator, BinaryOperator::Identical(_)) => {
                let span = node_span(b);
                let Some(code) = identical(ctx, span, ancestors, b.lhs, b.rhs) else {
                    return;
                };
                (span, code)
            }
            _ => return,
        };
        out.push(Report {
            span,
            message: "Write the condition without the negation or the comparison with a boolean."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, code)],
            }),
        });
    }
}

fn boolean_not(ctx: &Context<'_>, operand: &Expression<'_>) -> Option<(String, Prec)> {
    let Expression::Binary(b) = unparenthesize(operand) else {
        return None;
    };
    if matches!(b.operator, BinaryOperator::Or(_)) || is_binary_op(b.lhs) || is_binary_op(b.rhs) {
        return None;
    }
    inversed(ctx, b.lhs, &b.operator, b.rhs)
}

fn identical(
    ctx: &Context<'_>,
    span: Span,
    ancestors: &[Node<'_, '_>],
    lhs: &Expression<'_>,
    rhs: &Expression<'_>,
) -> Option<String> {
    let (first, other) = match_first_and_second(
        lhs,
        rhs,
        |e| {
            matches!(e, Expression::Binary(b)
                if matches!(b.operator, BinaryOperator::Identical(_) | BinaryOperator::NotIdentical(_)))
        },
        |e| is_constant(e, "true") || is_constant(e, "false"),
    )?;
    if let (true, Expression::Binary(b)) = (is_constant(other, "false"), first) {
        let (code, prec) = inversed(ctx, b.lhs, &b.operator, b.rhs)?;
        return Some(in_parent(span, ancestors, code, prec));
    }
    Some(moved(ctx, span, ancestors, first))
}

fn inversed(
    ctx: &Context<'_>,
    lhs: &Expression<'_>,
    op: &BinaryOperator<'_>,
    rhs: &Expression<'_>,
) -> Option<(String, Prec)> {
    let inverse = safe_inverse(ctx, op, lhs, rhs)?;
    let (p, l, r) = binary(op);
    Some((infix(ctx, lhs, inverse, rhs, (l, r)), Prec::Infix(p, l, r)))
}

const CHECKS: [&str; 3] = ["conditions", "identical-true", "not-identical"];

/// Pun's `checks`: the ones that run, one for each of Rector's three rules (`identical-true` is
/// `SimplifyBoolIdenticalTrueRector`, in its dead-code set; `conditions` and `not-identical` are in its code-quality
/// set).
pub(super) fn checks(options: &serde_json::Value, check: &str) -> Result<bool, OptionsError> {
    part_on(options, "checks", &CHECKS, check)
}
