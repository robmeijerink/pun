use mago_syntax::cst::{Block, Expression, Node, NodeKind, UnaryPrefixOperator};

use super::facts::facts;
use super::silent_void::{Place, has_silent_void};
use super::{add_return_type, any_scoped, class_scope, untyped};
use crate::{Context, Report, Rule};

pub struct AddReturnTypeFromCast;

impl Rule for AddReturnTypeFromCast {
    fn id(&self) -> &'static str {
        "lint/types/add-return-type-from-cast"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(super::SCALAR_TYPES)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Function, NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(f) = untyped(node, ancestors, ctx) else {
            return;
        };
        let body = f.body;
        let Some(r#type) = cast_type(body) else {
            return;
        };
        if r#type == "object" && ctx.php_version.id() < 70200 {
            return;
        }
        let scope = class_scope(ancestors);
        let place = Place {
            facts: &facts(ctx),
            ancestors,
            scope: scope.as_ref(),
        };
        if has_silent_void(&place, body.statements.iter()) {
            return;
        }
        out.push(add_return_type(f.span, f.at, r#type));
    }
}

fn cast_type(body: &Block<'_>) -> Option<&'static str> {
    let mut types = Vec::new();
    let mut other = false;
    any_scoped(Node::Block(body), &mut |n| {
        if let Node::Yield(_) = n {
            other = true;
        }
        if let Node::Return(r) = n {
            match r.value.map(cast_of) {
                Some(Some(t)) => types.push(t),
                _ => other = true,
            }
        }
        other
    });
    if other {
        return None;
    }
    let first = *types.first()?;
    types.iter().all(|t| *t == first).then_some(first)
}

fn cast_of(expr: &Expression<'_>) -> Option<&'static str> {
    let expr = match expr {
        Expression::Parenthesized(p) => p.expression,
        other => other,
    };
    let Expression::UnaryPrefix(prefix) = expr else {
        return None;
    };
    let scalar = |t| (!matches!(prefix.operand, Expression::Literal(_))).then_some(t);
    match prefix.operator {
        UnaryPrefixOperator::IntCast(..) | UnaryPrefixOperator::IntegerCast(..) => scalar("int"),
        UnaryPrefixOperator::FloatCast(..)
        | UnaryPrefixOperator::DoubleCast(..)
        | UnaryPrefixOperator::RealCast(..) => scalar("float"),
        UnaryPrefixOperator::StringCast(..) | UnaryPrefixOperator::BinaryCast(..) => {
            scalar("string")
        }
        UnaryPrefixOperator::BoolCast(..) | UnaryPrefixOperator::BooleanCast(..) => scalar("bool"),
        UnaryPrefixOperator::ArrayCast(..) => Some("array"),
        // Rector 2.9.2: `(object)` passes an object through unchanged, so only `object` is safe.
        UnaryPrefixOperator::ObjectCast(..) => Some("object"),
        _ => None,
    }
}
