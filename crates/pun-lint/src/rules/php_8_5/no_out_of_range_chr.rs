use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::{BinaryOperator, Expression, Node, NodeKind, UnaryPrefixOperator};

use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{args, named};
use crate::rules::support::values::{Constant, Known, constant, known};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoOutOfRangeChr;

impl Rule for NoOutOfRangeChr {
    fn id(&self) -> &'static str {
        "lint/php-8.5/no-out-of-range-chr"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80500)
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
        let Node::FunctionCall(c) = node else { return };
        let call = c;
        if !named(call, "chr") {
            return;
        }
        let Some(first) = args(call).first().map(|a| value(a)) else {
            return;
        };
        if let Expression::Binary(b) = first
            && matches!(b.operator, BinaryOperator::Modulo(_))
        {
            return;
        }
        let at = node_span(call).start;
        if let Some(Known::Value(v)) = known(ancestors, first, at)
            && let Expression::ConstantAccess(_) = unsigned(v)
        {
            return;
        }
        let Some(Constant::Int(v)) = constant(ancestors, first, at) else {
            return;
        };
        if (0..=255).contains(&v) {
            return;
        }
        let inner = node_span(unparenthesize(first));
        // `%` keeps the dividend's sign, so Rector 2.9.2 adds 256 back to a negative remainder.
        let wrap = if v % 256 < 0 { " + 256" } else { "" };
        let edits = vec![Edit::new(
            inner,
            format!("{} % 256{wrap}", &ctx.source[inner.start..inner.end]),
        )];
        out.push(Report {
            span: node_span(first),
            message: "Wrap the out-of-range `chr()` argument in `% 256`.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

fn unsigned<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    match unparenthesize(expr) {
        Expression::UnaryPrefix(u)
            if matches!(
                u.operator,
                UnaryPrefixOperator::Negation(_) | UnaryPrefixOperator::Plus(_)
            ) =>
        {
            unsigned(u.operand)
        }
        e => e,
    }
}
