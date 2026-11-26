use crate::problem::{Edit, Fix, Safety, Span};
use mago_syntax::cst::{Expression, Node, NodeKind, Statement};

use crate::rules::dead_code::no_empty_block::inverted_condition;
use crate::rules::support::side_effects::has_side_effect;
use crate::rules::support::statement_edits::{
    IfView, List, braced, enclosing, first_comment, if_view, line_indent, previous_token_end,
    remove,
};
use crate::rules::support::statements::operand_precedence;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoConstantCondition;

impl Rule for NoConstantCondition {
    fn id(&self) -> &'static str {
        "lint/complexity/no-constant-condition"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Statement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Statement(s @ Statement::If(r#if)) = node else {
            return;
        };
        let source = ctx.source;
        let view = if_view(source, r#if);
        let empty_else = view.else_.as_ref().is_some_and(|(l, _)| l.is_empty());
        let else_ = view.else_.as_ref().filter(|_| !empty_else);
        // Rector 2.9.2: an empty elseif still stops later branches, so only trailing ones go, and only without an else.
        let mut k = view.elseifs.len();
        while else_.is_none() && k > 0 {
            let (condition, list, _) = &view.elseifs[k - 1];
            if !list.is_empty() || has_side_effect(ancestors, condition) {
                break;
            }
            k -= 1;
        }
        let kept: Vec<usize> = (0..k).collect();
        let elseifs_changed = kept.len() != view.elseifs.len();
        let (list, index) = enclosing(source, s, ancestors);
        let span = node_span(s);
        let indent = line_indent(source, span.start);
        let commented = list.with_comments(ctx, index).start != span.start;
        let dead =
            view.stmts.is_empty() && !commented && !has_side_effect(ancestors, view.condition);
        // An empty `if` merges into a single elseif after it; a trailing else, or later elseifs, would then run when
        // the original's empty `if` caught the case.
        let merge = dead && (kept.is_empty() || (kept.len() == 1 && else_.is_none()));
        let edits = if !merge {
            if !empty_else && !elseifs_changed {
                return;
            }
            if elseifs_changed && kept.is_empty() {
                let condition = &source[range(unparenthesize(view.condition))];
                let mut text = format!("if ({condition}) {}", braced(ctx, &view.stmts, indent));
                if let Some((else_list, else_span)) = else_ {
                    text.push(' ');
                    text.push_str(&else_text(ctx, &view, else_list, *else_span, indent));
                }
                vec![Edit::new(span, text)]
            } else {
                kept_edits(ctx, span.start, &view, &kept, empty_else)
            }
        } else if let Some(&first) = kept.first() {
            let (condition, first_list, _) = &view.elseifs[first];
            let condition = format!(
                "{} && {}",
                inverted_condition(source, view.condition),
                and_operand(source, condition)
            );
            let text = format!("if ({condition}) {}", braced(ctx, first_list, indent));
            vec![Edit::new(span, text)]
        } else if let Some((else_list, _)) = else_ {
            let condition = inverted_condition(source, view.condition);
            vec![Edit::new(
                span,
                format!("if ({condition}) {}", braced(ctx, else_list, indent)),
            )]
        } else {
            remove(ctx, &list, &[index])
        };
        out.push(Report {
            span,
            message: "This `if` has branches that do nothing; remove them.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

fn range(expr: &Expression<'_>) -> std::ops::Range<usize> {
    let span = node_span(expr);
    span.start..span.end
}

fn and_operand(source: &str, expr: &Expression<'_>) -> String {
    let inner = unparenthesize(expr);
    let text = &source[range(inner)];
    if !matches!(inner, Expression::Assignment(_)) && operand_precedence(inner) <= 7 {
        format!("({text})")
    } else {
        text.to_string()
    }
}

fn else_text(
    ctx: &crate::Context<'_>,
    view: &IfView<'_>,
    list: &List<'_>,
    span: Span,
    indent: &str,
) -> String {
    let source = ctx.source;
    if view.colon {
        return format!("else {}", braced(ctx, list, indent));
    }
    source[span.start..span.end].to_string()
}

fn kept_edits(
    ctx: &crate::Context<'_>,
    from: usize,
    view: &IfView<'_>,
    kept: &[usize],
    empty_else: bool,
) -> Vec<Edit> {
    let source = ctx.source;
    let starts: Vec<usize> = (view.elseifs.iter())
        .map(|(_, _, span)| {
            let before = previous_token_end(ctx, from, span.start);
            first_comment(source, before, span.start).unwrap_or(span.start)
        })
        .collect();
    let ends: Vec<usize> = view.elseifs.iter().map(|(_, _, span)| span.end).collect();
    let mut edits = Vec::new();
    if let Some(&first) = kept.first() {
        if first > 0 {
            edits.push(Edit::new(Span::new(starts[0], starts[first]), ""));
        }
        for i in first + 1..view.elseifs.len() {
            if !kept.contains(&i) {
                edits.push(Edit::new(Span::new(ends[i - 1], ends[i]), ""));
            }
        }
    }
    if empty_else && let Some((_, span)) = &view.else_ {
        let before = previous_token_end(ctx, from, span.start);
        edits.push(Edit::new(Span::new(before, span.end), ""));
    }
    edits
}
