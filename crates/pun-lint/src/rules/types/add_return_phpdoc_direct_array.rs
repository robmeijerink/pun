use mago_syntax::cst::{Expression, Node, NodeKind};
use pun_phpstan::types::Type;

use super::class_scope;
use super::docblock::{FunctionLike, doc_report, referenced_classes};
use super::facts::facts;
use super::never::has_parent_class_method;
use super::silent_void::Place;
use crate::rules::support::phpdoc_writer::type_mapper::{
    generalize_constant_array, is_useful_array_tag,
};
use crate::rules::support::phpdoc_writer::{TypeNode, expr_type};
use crate::{Context, Report, Rule};

pub struct AddReturnPhpdocDirectArray;

impl Rule for AddReturnPhpdocDirectArray {
    fn id(&self) -> &'static str {
        "lint/types/add-return-phpdoc-direct-array"
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
        let Some(f) = FunctionLike::of(node) else {
            return;
        };
        let Some(body) = f.body else {
            return;
        };
        // Only a single returned array literal gets a tag: rule the rest out before the doc block and the types.
        let returns = super::returns_scoped(body);
        let (value, empty) = match returns.as_slice() {
            [r] => match r.value {
                Some(value @ Expression::Array(a)) => (value, a.elements.is_empty()),
                Some(value @ Expression::LegacyArray(a)) => (value, a.elements.is_empty()),
                _ => return,
            },
            _ => return,
        };
        let facts = facts(ctx);
        let scope = class_scope(ancestors);
        if f.method.is_some()
            && !f.is_private()
            && scope
                .as_ref()
                .is_none_or(|s| has_parent_class_method(&facts, s, f.name_text))
        {
            return;
        }
        let target = f.target(ctx);
        let mut info = target.info(ctx.source);
        // Rector 2.9.2: skip an invalid @return tag, as adding a new one would never reach a fixed point.
        if info.has_invalid_tag("@return") || is_useful_array_tag(info.return_tag().map(|t| &t.ty))
        {
            return;
        }
        let place = Place {
            facts: &facts,
            ancestors,
            scope: scope.as_ref(),
        };
        if !super::has_only_return_with_expr(&place, body, &returns) {
            return;
        }
        if empty {
            if !matches!(info.return_tag().map(|t| &t.ty), Some(TypeNode::Identifier(name)) if name == "array")
            {
                return;
            }
            info.set_return_type(TypeNode::Array {
                inner: Box::new(TypeNode::ident("mixed")),
                generic_parent: false,
            });
            out.extend(doc_report(
                f.name,
                "Add the `@return` array type of the returned array.",
                ctx.source,
                &target,
                &info,
            ));
            return;
        }
        let Some(Type::ConstArray(returned)) = expr_type(ctx, ancestors, value) else {
            return;
        };
        let classes = referenced_classes(&Type::ConstArray(returned.clone()));
        if !classes.is_empty() && classes.len() == returned.values.len() {
            return;
        }
        info.set_return_type(generalize_constant_array(&returned));
        out.extend(doc_report(
            f.name,
            "Add the `@return` array type of the returned array.",
            ctx.source,
            &target,
            &info,
        ));
    }
}
