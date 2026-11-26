use crate::problem::{Fix, Safety};
use mago_span::HasSpan;
use mago_syntax::cst::*;

use crate::rules::support::side_effects::{has_side_effect, writes_variable};
use crate::rules::support::statement_edits::{
    HOLDERS, List, block_list, body_list, holder_list, is_plain_assign, open_list, remove,
};
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoEmptyLoop {
    /// `loop` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoEmptyLoop {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-empty-block"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !super::no_empty_block::checks(options, "loop")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::DoWhile,
            NodeKind::For,
            NodeKind::Foreach,
            NodeKind::While,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        _node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if self.off {
            return;
        }
        let source = ctx.source;
        let [.., grand, Node::Statement(stmt)] = ancestors else {
            return;
        };
        let Some((body, exprs)) = parts(source, stmt) else {
            return;
        };
        if !body.is_empty() || keeps_state(ctx, stmt) {
            return;
        }
        if exprs.iter().any(|&expr| {
            let expr = match crate::syntax::unparenthesize(expr) {
                Expression::Assignment(a) if is_plain_assign(a) => a.rhs,
                _ => expr,
            };
            has_side_effect(ancestors, expr)
        }) {
            return;
        }
        let edits = if HOLDERS.contains(&grand.kind()) {
            let list = match grand {
                Node::Block(block) => block_list(source, block),
                _ => holder_list(source, *grand, &ancestors[..ancestors.len() - 2]).unwrap(),
            };
            let key = list
                .stmts
                .iter()
                .position(|s| std::ptr::eq(*s, *stmt))
                .unwrap();
            remove(ctx, &list, &[key])
        } else {
            remove(ctx, &body_list(source, stmt), &[0])
        };
        out.push(Report {
            span: node_span(*stmt),
            message: "This loop has no body and no side effect; remove it.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

/// Rector 2.9.2 keeps a loop whose condition changes state, and a foreach over anything but an array (iterating a
/// generator or an `Iterator` runs its code).
fn keeps_state(ctx: &Context<'_>, stmt: &Statement<'_>) -> bool {
    let changes = |e: &Expression<'_>| writes_variable(e);
    match stmt {
        Statement::DoWhile(r#do) => changes(r#do.condition),
        Statement::While(r#while) => changes(r#while.condition),
        Statement::For(r#for) => r#for.conditions.iter().any(|c| changes(c)),
        Statement::Foreach(foreach) => !crate::rules::types::facts::facts(ctx)
            .full_type(node_span(crate::syntax::unparenthesize(foreach.expression)))
            .is_some_and(mago_codex::ttype::union::TUnion::is_array),
        _ => false,
    }
}

fn offset(span: mago_span::Span) -> (usize, usize) {
    (span.start.offset as usize, span.end.offset as usize)
}

fn parts<'a>(source: &str, stmt: &'a Statement<'a>) -> Option<(List<'a>, Vec<&'a Expression<'a>>)> {
    Some(match stmt {
        Statement::DoWhile(r#do) => (body_list(source, r#do.statement), vec![r#do.condition]),
        Statement::While(r#while) => {
            let body = match &r#while.body {
                WhileBody::Statement(s) => body_list(source, s),
                WhileBody::ColonDelimited(b) => {
                    colon(source, &b.statements, b.colon, b.end_while.span())
                }
            };
            (body, vec![r#while.condition])
        }
        Statement::For(r#for) => {
            let body = match &r#for.body {
                ForBody::Statement(s) => body_list(source, s),
                ForBody::ColonDelimited(b) => {
                    colon(source, &b.statements, b.colon, b.end_for.span())
                }
            };
            let exprs = r#for
                .initializations
                .iter()
                .chain(r#for.conditions.iter())
                .chain(r#for.increments.iter())
                .copied()
                .collect();
            (body, exprs)
        }
        Statement::Foreach(foreach) => {
            let body = match &foreach.body {
                ForeachBody::Statement(s) => body_list(source, s),
                ForeachBody::ColonDelimited(b) => {
                    colon(source, &b.statements, b.colon, b.end_foreach.span())
                }
            };
            (body, vec![foreach.expression, foreach.target.value()])
        }
        _ => return None,
    })
}

fn colon<'a>(
    source: &str,
    statements: &'a Sequence<'a, Statement<'a>>,
    colon: mago_span::Span,
    end: mago_span::Span,
) -> List<'a> {
    open_list(source, statements, offset(colon).1, offset(end).0)
}
