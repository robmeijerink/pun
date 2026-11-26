//! Rector 2.9.2's `ArrayKeysToArrayKeyFirstLastRector`.

use mago_codex::ttype::atomic::TAtomic;
use mago_syntax::cst::{Argument, Expression, Identifier, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::{unpacked, value};
use crate::rules::support::expressions::{call_to, text};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// `PREVIOUS_TO_NEW_FUNCTIONS`.
const PREVIOUS_TO_NEW_FUNCTIONS: &[(&str, &str)] = &[
    ("current", "array_key_first"),
    ("reset", "array_key_first"),
    ("end", "array_key_last"),
];

pub struct PreferArrayKeyFirstLast;

impl Rule for PreferArrayKeyFirstLast {
    fn id(&self) -> &'static str {
        "lint/php-7.3/prefer-array-key-first-last"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        // `isset(self::PREVIOUS_TO_NEW_FUNCTIONS[$funcName])`: the name as written, case-sensitive.
        let written: &[u8] = match call.function {
            Expression::Identifier(Identifier::Local(id)) => id.value,
            Expression::Identifier(Identifier::FullyQualified(id)) => &id.value[1..],
            _ => return,
        };
        let Some((_, new)) = PREVIOUS_TO_NEW_FUNCTIONS
            .iter()
            .find(|(old, _)| old.as_bytes() == written)
        else {
            return;
        };
        let [arg] = call.argument_list.arguments.as_slice() else {
            return;
        };
        if skipped(arg) {
            return;
        }
        let Some(keys) = call_to(value(arg), "array_keys") else {
            return;
        };
        // `array_keys()` with a search value returns only the matching keys.
        let [array] = keys.argument_list.arguments.as_slice() else {
            return;
        };
        if skipped(array) {
            return;
        }
        // Rector 2.9.2: on an empty array, `current()`/`reset()`/`end()` return false, while `array_key_first()` and
        // `array_key_last()` return null (`isIterableAtLeastOnce()->yes()`).
        let non_empty = crate::rules::types::facts::facts(ctx)
            .full_type(node_span(crate::syntax::unparenthesize(value(array))))
            .is_some_and(|t| {
                !t.types.is_empty()
                    && t.types
                        .iter()
                        .all(|a| matches!(a, TAtomic::Array(a) if a.is_non_empty()))
            });
        if !non_empty {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            format!("Use `{new}()` instead of reading a key off `array_keys()`."),
            Safety::Safe,
            vec![Edit::new(
                node_span(&node),
                format!("{new}({})", text(ctx, value(array))),
            )],
        ));
    }
}

/// A named, unpacked or first-class-callable `...` argument, which Rector leaves alone.
fn skipped(arg: &Argument<'_>) -> bool {
    matches!(arg, Argument::Named(_)) || unpacked(arg)
}
