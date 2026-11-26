use crate::problem::{Edit, Fix, Safety, Span};
use mago_syntax::cst::*;

use crate::rules::support::sequence_diff::{Elem, coalesce_replacements, diff};
use crate::rules::support::statement_edits::{
    List, first_comment, holder_list, line_indent, previous_token_end, printed, reindent,
    same_with_comments,
};
use crate::syntax::{node_span, same_code};
use crate::{Context, Report, Rule};

pub struct NoDuplicateCaseBody;

impl Rule for NoDuplicateCaseBody {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-duplicate-case-body"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Switch])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Switch(switch) = node else { return };
        let (cases, open) = match &switch.body {
            SwitchBody::BraceDelimited(b) => (b.cases.as_slice(), b.left_brace.end.offset as usize),
            SwitchBody::ColonDelimited(b) => (b.cases.as_slice(), b.colon.end.offset as usize),
        };
        if cases.len() < 2 {
            return;
        }
        let source = ctx.source;
        let lists: Vec<List<'_>> = cases
            .iter()
            .map(|c| holder_list(source, case_node(c), &[]).unwrap())
            .collect();
        let mut body: Vec<Option<usize>> = (0..cases.len())
            .map(|i| (!lists[i].is_empty()).then_some(i))
            .collect();
        // Rector 2.9.2's `resolveFallThroughCasesKeys()`: a case with code that the previous case with code runs into.
        let mut fall_through: Vec<usize> = Vec::new();
        let mut previous_falls = false;
        for (i, list) in lists.iter().enumerate() {
            let Some(last) = list.stmts.last() else {
                continue;
            };
            if previous_falls {
                fall_through.push(i);
            }
            previous_falls = !matches!(
                last,
                Statement::Break(_) | Statement::Return(_) | Statement::Continue(_)
            ) && !matches!(last, Statement::Expression(e) if matches!(e.expression, Expression::Throw(_)));
        }
        let mut result: Vec<usize> = Vec::new();
        let mut processed: Vec<usize> = Vec::new();
        let mut changed = false;
        for outer in 0..cases.len() {
            if processed.contains(&outer) {
                continue;
            }
            processed.push(outer);
            let Some(outer_body) = body[outer] else {
                result.push(outer);
                continue;
            };
            let mut without_stmts: Vec<usize> = Vec::new();
            let mut equal_cases: Vec<usize> = Vec::new();
            for (inner, slot) in body.iter_mut().enumerate() {
                if processed.contains(&inner) {
                    continue;
                }
                let Some(inner_body) = *slot else {
                    without_stmts.push(inner);
                    continue;
                };
                // Moving a case reached by fall-through would change what the previous case runs into.
                if fall_through.contains(&inner) {
                    without_stmts.clear();
                    continue;
                }
                if are_equal_and_with_break(ctx, &lists[outer_body], &lists[inner_body]) {
                    for &w in &without_stmts {
                        equal_cases.push(w);
                        processed.push(w);
                    }
                    *slot = None;
                    equal_cases.push(inner);
                    processed.push(inner);
                }
                without_stmts.clear();
            }
            if equal_cases.is_empty() {
                result.push(outer);
                continue;
            }
            changed = true;
            body[*equal_cases.last().unwrap()] = body[outer];
            body[outer] = None;
            result.push(outer);
            result.extend(equal_cases);
        }
        if !changed {
            return;
        }
        let mut starts = Vec::new();
        let mut previous = open;
        for case in cases {
            let span = node_span(case);
            let before = previous_token_end(ctx, previous, span.start);
            starts.push(first_comment(source, before, span.start).unwrap_or(span.start));
            previous = span.end;
        }
        let texts: Vec<String> = (0..cases.len())
            .map(|i| {
                let span = node_span(&cases[i]);
                if body[i] == (!lists[i].is_empty()).then_some(i) {
                    return source[span.start..span.end].to_string();
                }
                let head_end = match &cases[i] {
                    SwitchCase::Expression(c) => node_span(&c.separator).end,
                    SwitchCase::Default(c) => node_span(&c.separator).end,
                };
                let mut text = source[span.start..head_end].to_string();
                if text.ends_with(';') {
                    text.replace_range(text.len() - 1.., ":");
                }
                if let Some(from) = body[i] {
                    let stmts = printed(ctx, &lists[from]);
                    let indent = line_indent(
                        source,
                        lists[from]
                            .stmts
                            .first()
                            .map_or(stmts.start, |s| node_span(*s).start),
                    );
                    text.push('\n');
                    text.push_str(indent);
                    text.push_str(&reindent(source, stmts, indent, indent));
                }
                text
            })
            .collect();
        let comments = |i: usize| {
            let start = node_span(&cases[i]).start;
            (starts[i] < start).then(|| source[starts[i]..start].trim_end())
        };
        let nl = format!("\n{}", line_indent(source, node_span(&cases[0]).start));
        let mut text = String::new();
        let mut pos = open;
        let moves = diff(cases.len(), &result)
            .into_iter()
            .map(|elem| match elem {
                Elem::Keep(old, new) => Elem::Keep(old, result[new]),
                Elem::Add(new) => Elem::Add(result[new]),
                elem => elem,
            });
        for elem in coalesce_replacements(moves.collect()) {
            match elem {
                Elem::Keep(old, new) | Elem::Replace(old, new) => {
                    let item_start = node_span(&cases[old]).start;
                    let comment_start = starts[old];
                    text.push_str(&source[pos..comment_start]);
                    if old == new {
                        text.push_str(&source[comment_start..item_start]);
                    } else if let Some(c) = comments(new) {
                        text.push_str(c);
                        text.push_str(&nl);
                    }
                    text.push_str(&texts[new]);
                    pos = node_span(&cases[old]).end;
                }
                Elem::Add(new) => {
                    text.push_str(&nl);
                    if let Some(c) = comments(new) {
                        text.push_str(c);
                        text.push_str(&nl);
                    }
                    text.push_str(&texts[new]);
                }
                Elem::Remove(old) => pos = node_span(&cases[old]).end,
            }
        }
        let span = Span::new(open, pos);
        out.push(Report {
            span,
            message: "These cases run the same code; group them.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, text)],
            }),
        });
    }
}

fn case_node<'a>(case: &'a SwitchCase<'a>) -> Node<'a, 'a> {
    match case {
        SwitchCase::Expression(c) => Node::SwitchExpressionCase(c),
        SwitchCase::Default(c) => Node::SwitchDefaultCase(c),
    }
}

fn are_equal_and_with_break(ctx: &crate::Context<'_>, current: &List<'_>, next: &List<'_>) -> bool {
    let source = ctx.source;
    let is_break = |l: &List<'_>| matches!(l.stmts.first(), Some(Statement::Break(_)));
    if is_break(current) && is_break(next) {
        return same_with_comments(ctx, current, next);
    }
    if !same_code(source, printed(ctx, current), printed(ctx, next)) {
        return false;
    }
    current
        .stmts
        .iter()
        .any(|s| matches!(s, Statement::Break(_) | Statement::Return(_)))
}
