//! Rector 2.9.2's `VersionCompareFuncCallArgumentRector`.

use mago_syntax::cst::{Expression, Literal, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::named;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

/// `OPERATOR_MAP`: the aliases PHP 8.0 no longer accepts, and the operator each one meant.
const OPERATOR_MAP: &[(&[u8], &str)] = &[
    (b"gte", "ge"),
    (b"lte", "le"),
    (b"", "<"),
    (b"!", "!="),
    (b"g", "gt"),
    (b"l", "lt"),
    (b"n", "ne"),
];

pub struct NoVersionCompareOperatorAlias;

impl Rule for NoVersionCompareOperatorAlias {
    fn id(&self) -> &'static str {
        "lint/php-8.0/no-version-compare-operator-alias"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        if !named(call, "version_compare") {
            return;
        }
        let Some(operator) = call.argument_list.arguments.get(2) else {
            return;
        };
        let operator = value(operator);
        let Expression::Literal(Literal::String(string)) = unparenthesize(operator) else {
            return;
        };
        let Some(alias) = string.value else {
            return;
        };
        let Some((_, replacement)) = OPERATOR_MAP.iter().find(|(from, _)| *from == alias) else {
            return;
        };
        out.push(Report::fixed(
            node_span(string),
            format!("Use the `{replacement}` operator of `version_compare()`: PHP 8.0 removed this alias."),
            Safety::Safe,
            vec![Edit::new(node_span(string), format!("'{replacement}'"))],
        ));
    }
}
