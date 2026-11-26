//! Rector 2.8.4's `ClosureFromCallableToFirstClassCallableRector`.

use mago_syntax::cst::{
    ArrayElement, ClassLikeConstantSelector, ClassLikeMemberSelector, Expression, Literal, Node,
    NodeKind,
};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::{unpacked, value};
use crate::rules::support::expressions::text;
use crate::rules::support::names::class_name;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoClosureFromCallable;

impl Rule for NoClosureFromCallable {
    fn id(&self) -> &'static str {
        "lint/php-8.1/no-closure-from-callable"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::StaticMethodCall(call) = node else {
            return;
        };
        // `shouldSkip`.
        let Expression::Identifier(class) = call.class else {
            return;
        };
        let ClassLikeMemberSelector::Identifier(method) = &call.method else {
            return;
        };
        if !class_name(ancestors, class).eq_ignore_ascii_case("Closure")
            || method.value != b"fromCallable"
        {
            return;
        }
        let [arg] = call.argument_list.arguments.as_slice() else {
            return;
        };
        if unpacked(arg) {
            return;
        }
        let code = match value(arg) {
            Expression::Literal(Literal::String(s)) => {
                let Some(name) = s.value else {
                    return;
                };
                let name = String::from_utf8_lossy(name);
                format!("\\{}(...)", name.trim_start_matches('\\'))
            }
            Expression::Array(array) => {
                let elements = array.elements.as_slice();
                let item = |i: usize| match elements.get(i)? {
                    ArrayElement::KeyValue(e) => Some(e.value),
                    ArrayElement::Value(e) => Some(e.value),
                    ArrayElement::Variadic(_) | ArrayElement::Missing(_) => None,
                };
                let (Some(target), Some(Expression::Literal(Literal::String(method)))) =
                    (item(0), item(1))
                else {
                    return;
                };
                let Some(method) = method.value.map(String::from_utf8_lossy) else {
                    return;
                };
                match target {
                    // Rector 2.9.2: a string variable holds a static callable, e.g. 'SomeClass'.
                    Expression::Variable(_) => {
                        let object = crate::rules::types::facts::facts(ctx)
                            .full_type(node_span(target))
                            .is_some_and(|t| !t.types.is_empty() && t.is_objecty());
                        if !object {
                            return;
                        }
                        format!("{}->{method}(...)", text(ctx, target))
                    }
                    Expression::Literal(Literal::String(class)) => {
                        let Some(class) = class.value.map(String::from_utf8_lossy) else {
                            return;
                        };
                        format!("\\{}::{method}(...)", class.trim_start_matches('\\'))
                    }
                    Expression::Access(mago_syntax::cst::Access::ClassConstant(fetch)) if matches!(&fetch.constant, ClassLikeConstantSelector::Identifier(c) if c.value.eq_ignore_ascii_case(b"class")) =>
                    {
                        let class = match fetch.class {
                            Expression::Self_(_)
                            | Expression::Static(_)
                            | Expression::Parent(_) => text(ctx, fetch.class).to_string(),
                            Expression::Identifier(id) => {
                                format!("\\{}", class_name(ancestors, id))
                            }
                            _ => return,
                        };
                        format!("{class}::{method}(...)")
                    }
                    _ => return,
                }
            }
            // Rector returns the node as it is.
            _ => return,
        };
        out.push(Report::fixed(
            node_span(&node),
            "Use a first-class callable instead of `Closure::fromCallable()`.",
            Safety::Safe,
            vec![Edit::new(node_span(&node), code)],
        ));
    }
}
