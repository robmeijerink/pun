use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::{BinaryOperator, Expression, Node, NodeKind};

use crate::rules::support::expressions::{Prec, in_parent, infix};
use crate::rules::support::syntax_types::is_bool;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoNegatedComparison {
    /// `not-identical` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoNegatedComparison {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-simple-condition"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !super::prefer_simple_condition::checks(options, "not-identical")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary, NodeKind::UnaryPrefix])
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
        let (left, right) = match node {
            Node::UnaryPrefix(not) if not.operator.is_not() => {
                let Expression::Binary(identical) = unparenthesize(not.operand) else {
                    return;
                };
                if !matches!(identical.operator, BinaryOperator::Identical(_))
                    || !is_bool(ancestors, identical.lhs)
                    || !is_bool(ancestors, identical.rhs)
                {
                    return;
                }
                (identical.lhs, identical.rhs)
            }
            Node::Binary(identical)
                if matches!(identical.operator, BinaryOperator::Identical(_)) =>
            {
                let Expression::UnaryPrefix(not) = unparenthesize(identical.lhs) else {
                    return;
                };
                // Rector 2.9.2 checks the negated operand: `!$a === $b` is `$a !== $b` only for a bool `$a`.
                if !not.operator.is_not()
                    || !is_bool(ancestors, not.operand)
                    || !is_bool(ancestors, identical.rhs)
                {
                    return;
                }
                (not.operand, identical.rhs)
            }
            _ => return,
        };
        let span = node_span(&node);
        let code = infix(ctx, left, "!==", right, (80, 80));
        out.push(Report {
            span,
            message: "Use `!==` instead of negating an `===` comparison.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(
                    span,
                    in_parent(span, ancestors, code, Prec::Infix(80, 80, 80)),
                )],
            }),
        });
    }
}
