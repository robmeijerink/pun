use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::*;

use crate::rules::dead_code::no_redundant_property_default::is_property_assigned;
use crate::rules::support::side_effects::any_node;
use crate::rules::support::syntax_types::is_array_property;
use crate::rules::types::add_property_type_constructor::is_test_case;
use crate::rules::types::facts::{Facts, facts};
use crate::rules::types::property_type::ClassView;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct PreferEmptyArrayIdentical {
    /// `empty` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for PreferEmptyArrayIdentical {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-empty-array-compare"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !super::prefer_empty_array_compare::checks(options, "empty")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::EmptyConstruct, NodeKind::UnaryPrefix])
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
        let (span, empty, negated) = match node {
            Node::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Not(_)) => {
                let Expression::Construct(Construct::Empty(empty)) = unparenthesize(u.operand)
                else {
                    return;
                };
                (node_span(u), empty, true)
            }
            Node::EmptyConstruct(empty) => {
                let outer = ancestors.iter().rposition(|n| {
                    !matches!(
                        n,
                        Node::Expression(_) | Node::Construct(_) | Node::Parenthesized(_)
                    )
                });
                if let Some(Node::UnaryPrefix(u)) = outer.map(|i| ancestors[i])
                    && matches!(u.operator, UnaryPrefixOperator::Not(_))
                {
                    return;
                }
                (node_span(empty), empty, false)
            }
            _ => return,
        };
        let value = unparenthesize(empty.value);
        if !is_allowed(ctx, ancestors, value) {
            return;
        }
        let s = node_span(value);
        let text = &ctx.source[s.start..s.end];
        let operator = if negated { "!==" } else { "===" };
        out.push(Report {
            span,
            message: "Compare the array with `[]` instead of using `empty()`.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, format!("{text} {operator} []"))],
            }),
        });
    }
}

fn is_allowed(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], value: &Expression<'_>) -> bool {
    let facts = facts(ctx);
    let span = node_span(value);
    if !(facts.full_type(span)).is_some_and(|t| !t.types.is_empty() && t.is_array()) {
        return false;
    }
    match value {
        Expression::Variable(Variable::Direct(v)) => {
            !is_superglobal(v.name)
                && !is_non_typed_from_param(&facts, value)
                && !merged_in_loop(&facts, ancestors, v.name)
        }
        Expression::Access(Access::Property(_) | Access::StaticProperty(_)) => is_array_property(
            ancestors,
            value,
            &|v, _| is_non_typed_from_param(&facts, v),
            &|class, path, name| {
                ClassView::of(&facts, class, path).is_some_and(|view| {
                    is_property_assigned(ctx, &view, is_test_case(&facts, &view), name)
                })
            },
        ),
        _ => false,
    }
}

pub(crate) fn is_non_typed_from_param(facts: &Facts, variable: &Expression<'_>) -> bool {
    let span = node_span(variable);
    let full = facts.full_type(span);
    facts.native_type(span).is_none_or(|native| {
        native.is_mixed()
            || native.possibly_undefined()
            || native.possibly_undefined_from_try()
            || (full.is_some_and(mago_codex::ttype::union::TUnion::is_array) && !native.is_array())
            || (native.types.len() > 1 && full != Some(native))
    })
}

fn merged_in_loop(facts: &Facts, ancestors: &[Node<'_, '_>], name: &[u8]) -> bool {
    ancestors
        .iter()
        .rev()
        .take_while(|n| !matches!(n, Node::Function(_) | Node::Method(_) | Node::Closure(_)))
        .any(|n| {
            matches!(
                n,
                Node::Foreach(_) | Node::For(_) | Node::While(_) | Node::DoWhile(_)
            ) && any_node(*n, &|m| {
                matches!(m, Node::Assignment(a) if root_is(a.lhs, name) && {
                    let span = node_span(a.rhs);
                    facts.native_type(span).map(mago_codex::ttype::TType::get_id)
                        != facts.full_type(span).map(mago_codex::ttype::TType::get_id)
                })
            })
        })
}

fn root_is(lhs: &Expression<'_>, name: &[u8]) -> bool {
    match unparenthesize(lhs) {
        Expression::Variable(Variable::Direct(v)) => v.name == name,
        Expression::ArrayAccess(a) => root_is(a.array, name),
        Expression::ArrayAppend(a) => root_is(a.array, name),
        _ => false,
    }
}

fn is_superglobal(name: &[u8]) -> bool {
    matches!(
        name,
        b"$GLOBALS"
            | b"$_SERVER"
            | b"$_GET"
            | b"$_POST"
            | b"$_FILES"
            | b"$_COOKIE"
            | b"$_SESSION"
            | b"$_REQUEST"
            | b"$_ENV"
    )
}
