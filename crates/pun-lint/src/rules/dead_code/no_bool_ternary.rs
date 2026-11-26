use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::*;

use crate::rules::complexity::no_boolean_literal_compare::truth;
use crate::rules::support::expressions::safe_inverse;
use crate::rules::support::syntax_types::is_bool;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoBoolTernary {
    /// `bool` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoBoolTernary {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-useless-ternary"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !super::no_useless_ternary::checks(options, "bool")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Conditional])
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
        let Node::Conditional(ternary) = node else {
            return;
        };
        let Some(then) = ternary.then else {
            return;
        };
        let (Some(then), Some(r#else)) = (truth(then), truth(ternary.r#else)) else {
            return;
        };
        let source = ctx.source;
        let text = |e: &Expression<'_>| {
            let s = node_span(e);
            source[s.start..s.end].to_string()
        };
        let condition = unparenthesize(ternary.condition);
        let replacement = match condition {
            Expression::Binary(b) if !matches!(b.operator, BinaryOperator::Instanceof(_)) => {
                if then && !r#else {
                    text(condition)
                } else {
                    let Some(inverse) = safe_inverse(ctx, &b.operator, b.lhs, b.rhs) else {
                        return;
                    };
                    format!("{} {inverse} {}", text(b.lhs), text(b.rhs))
                }
            }
            Expression::Pipe(_) if then && !r#else => text(condition),
            Expression::Pipe(_) => return,
            _ if then && !r#else => cast(ancestors, condition, &text),
            _ if !then && r#else => match condition {
                Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Not(_)) => {
                    cast(ancestors, u.operand, &text)
                }
                // Rector 2.9.2 drops the `(bool)` cast: `!` already gives a bool.
                _ => format!("!{}", wrapped(condition, &text)),
            },
            _ => return,
        };
        out.push(Report {
            span: node_span(ternary),
            message:
                "This ternary only turns its condition into `true` or `false`; use the condition."
                    .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(node_span(ternary), replacement)],
            }),
        });
    }
}

fn cast(
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
    text: &dyn Fn(&Expression<'_>) -> String,
) -> String {
    if is_bool(ancestors, expr) {
        text(unparenthesize(expr))
    } else {
        format!("(bool) {}", wrapped(expr, text))
    }
}

fn wrapped(expr: &Expression<'_>, text: &dyn Fn(&Expression<'_>) -> String) -> String {
    let expr = unparenthesize(expr);
    match expr {
        Expression::Binary(_)
        | Expression::Assignment(_)
        | Expression::Conditional(_)
        | Expression::Yield(_)
        | Expression::Throw(_)
        | Expression::Pipe(_)
        | Expression::ArrowFunction(_)
        | Expression::Closure(_)
        | Expression::Construct(
            Construct::Print(_)
            | Construct::Include(_)
            | Construct::IncludeOnce(_)
            | Construct::Require(_)
            | Construct::RequireOnce(_),
        ) => {
            format!("({})", text(expr))
        }
        _ => text(expr),
    }
}
