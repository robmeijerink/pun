//! Rector 2.9.2's `RandomFunctionRector`.

use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{Argument, Expression, Node, NodeKind};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{named, text};
use crate::rules::support::side_effects::any_node;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

/// `OLD_TO_NEW_FUNCTION_NAMES`.
const OLD_TO_NEW_FUNCTION_NAMES: &[(&str, &str)] = &[
    ("getrandmax", "mt_getrandmax"),
    ("srand", "mt_srand"),
    ("rand", "random_int"),
];

pub struct PreferRandomInt;

impl Rule for PreferRandomInt {
    fn id(&self) -> &'static str {
        "lint/php-7.0/prefer-random-int"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70000)
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
        let Node::FunctionCall(call) = node else {
            return;
        };
        let arguments = call.argument_list.arguments.as_slice();
        if matches!(arguments, [Argument::Positional(a)] if a.ellipsis.is_some()) {
            return;
        }
        let Some((old, new)) = OLD_TO_NEW_FUNCTION_NAMES
            .iter()
            .find(|(old, _)| named(call, old))
        else {
            return;
        };
        let mut edits = vec![Edit::new(node_span(call.function), *new)];
        // random_int() ignores the seed and throws on min > max, unlike rand().
        if *new == "random_int" {
            if seeded(ancestors) {
                return;
            }
            match arguments {
                [] => edits.push(Edit::new(
                    Span::new(
                        call.argument_list.left_parenthesis.start.offset as usize,
                        call.argument_list.right_parenthesis.end.offset as usize,
                    ),
                    "(0, mt_getrandmax())",
                )),
                [min, max] => {
                    let facts = facts(ctx);
                    let int = |e: &Expression<'_>| {
                        (facts.full_type(node_span(unparenthesize(e))))
                            .and_then(TUnion::get_single_literal_int_value)
                    };
                    let (Some(low), Some(high)) = (int(value(min)), int(value(max))) else {
                        return;
                    };
                    if low > high {
                        edits.push(Edit::new(node_span(min), text(ctx, max)));
                        edits.push(Edit::new(node_span(max), text(ctx, min)));
                    }
                }
                _ => {}
            }
        }
        out.push(Report::fixed(
            node_span(&node),
            format!("Call `{new}()` instead of `{old}()`."),
            Safety::Unsafe,
            edits,
        ));
    }
}

/// `isSeededFile()`: the file calls `srand()` or `mt_srand()`.
fn seeded(ancestors: &[Node<'_, '_>]) -> bool {
    ancestors.first().is_some_and(|program| {
        any_node(
            *program,
            &|n| matches!(n, Node::FunctionCall(c) if named(c, "srand") || named(c, "mt_srand")),
        )
    })
}
