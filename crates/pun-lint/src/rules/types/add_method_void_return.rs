use mago_syntax::cst::{MethodBody, Modifier, Node, NodeKind};

use super::facts::facts;
use super::{
    MAGIC, VOID_TYPE, add_return_type, class_scope, has_exclusive_void, is_return_vendor_locked,
    is_throw_statement, statement_count,
};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct AddMethodVoidReturn;

impl Rule for AddMethodVoidReturn {
    fn id(&self) -> &'static str {
        "lint/types/add-method-void-return"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(VOID_TYPE)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Method(method) = node else {
            return;
        };
        let (None, MethodBody::Concrete(body)) = (&method.return_type_hint, &method.body) else {
            return;
        };
        let Some(scope) = class_scope(ancestors) else {
            return;
        };
        let count = statement_count(ctx.source, body);
        // Rector 2.9.2: a final method cannot be overridden by a child.
        let can_be_overridden = !scope.is_final
            && !method
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Final(_)));
        if can_be_overridden {
            let only_throw = count == 1 && body.statements.first().is_some_and(is_throw_statement);
            if only_throw || count == 0 {
                return;
            }
        }
        if can_be_overridden
            && method
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Protected(_)))
        {
            return;
        }
        if scope.is_interface || !has_exclusive_void(body) {
            return;
        }
        let name = method.name.value.to_ascii_lowercase();
        if name != b"__invoke" && MAGIC.iter().any(|m| m.as_bytes() == name) {
            return;
        }
        let private = method
            .modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Private(_)));
        if !private && is_return_vendor_locked(&facts(ctx), &scope, method.name.value) {
            return;
        }
        out.push(add_return_type(
            node_span(&method.name),
            node_span(&method.parameter_list).end,
            "void",
        ));
    }
}
