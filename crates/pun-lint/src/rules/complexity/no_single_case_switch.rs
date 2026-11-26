use crate::problem::{Edit, Fix, Safety, Span};
use mago_syntax::cst::{Expression, Literal, Node, NodeKind, Statement, SwitchBody, SwitchCase};

use crate::rules::support::conditions::{comments_between, indentation, print_comments, reprint};
use crate::rules::support::expressions::infix;
use crate::rules::support::statements::{
    Frame, Token, enclosing, holder_list, previous_token_end, tokens,
};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoSingleCaseSwitch;

impl Rule for NoSingleCaseSwitch {
    fn id(&self) -> &'static str {
        "lint/complexity/no-single-case-switch"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Switch])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Switch(switch) = node else { return };
        let cases = match &switch.body {
            SwitchBody::BraceDelimited(b) => &b.cases,
            SwitchBody::ColonDelimited(b) => &b.cases,
        };
        let [case] = cases.as_slice() else { return };
        if has_nested_break_or_continue(case) {
            return;
        }
        let source = ctx.source;
        let tokens = tokens(ctx);
        let (case_node, condition) = match case {
            SwitchCase::Expression(c) => (Node::SwitchExpressionCase(c), Some(c.expression)),
            SwitchCase::Default(c) => (Node::SwitchDefaultCase(c), None),
        };
        let items = items(source, tokens, case_node, condition.is_some());
        let span = node_span(switch);
        let before = previous_token_end(source, tokens, span.start);
        let start = comments_between(tokens, before, span.start)
            .first()
            .map_or(span.start, |c| c.start);
        let outer = indentation(source, span.start);
        let edit = match condition {
            Some(condition) => {
                let inner = outer + 4;
                let nl = format!("\n{}", " ".repeat(inner));
                let body: String = items
                    .iter()
                    .map(|item| format!("{nl}{}", item.print(source, tokens, inner)))
                    .collect();
                // Rector 2.9.2: switch compares loosely, so `===` only when both sides are ints.
                let facts = crate::rules::types::facts::facts(ctx);
                let int = |e: &Expression<'_>| {
                    (facts.full_type(node_span(unparenthesize(e))))
                        .is_some_and(|t| !t.types.is_empty() && t.is_int())
                };
                let op = if int(switch.expression) && int(condition) {
                    "==="
                } else {
                    "=="
                };
                let cond = infix(ctx, switch.expression, op, condition, (80, 80));
                let code = format!("if ({cond}) {{{body}\n{}}}", " ".repeat(outer));
                Edit::new(Span::new(start, span.end), code)
            }
            None => {
                if items.is_empty() {
                    return;
                }
                default_edit(source, tokens, ancestors, &items, span, start, outer)
            }
        };
        out.push(Report {
            span,
            message: "A `switch` with a single case is an `if`.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![edit],
            }),
        });
    }
}

fn has_nested_break_or_continue(case: &SwitchCase<'_>) -> bool {
    fn find(node: Node<'_, '_>) -> bool {
        let mut found = false;
        node.visit_children(|child| {
            found = found
                || match child {
                    Node::Function(_)
                    | Node::Method(_)
                    | Node::Closure(_)
                    | Node::ArrowFunction(_)
                    | Node::Class(_)
                    | Node::AnonymousClass(_) => false,
                    Node::Break(_) | Node::Continue(_) => true,
                    _ => find(child),
                }
        });
        found
    }
    let statements = match case {
        SwitchCase::Expression(c) => &c.statements,
        SwitchCase::Default(c) => &c.statements,
    };
    statements.iter().any(|s| match s {
        Statement::Break(_) => false,
        Statement::Continue(_) => true,
        s => find(Node::Statement(s)),
    })
}

struct Item {
    comments: Vec<Span>,
    code: Code,
}

enum Code {
    Statement(Span),
    Break(String),
}

impl Item {
    fn print(&self, source: &str, tokens: &[Token], level: usize) -> String {
        let nl = format!("\n{}", " ".repeat(level));
        let comments = print_comments(source, &self.comments, &nl);
        let code = match &self.code {
            Code::Statement(s) => {
                let adjust = level as isize - indentation(source, s.start) as isize;
                reprint(source, tokens, *s, adjust)
            }
            Code::Break(b) => b.clone(),
        };
        if self.comments.is_empty() {
            code
        } else {
            format!("{comments}{nl}{code}")
        }
    }
}

fn items(source: &str, tokens: &[Token], case: Node<'_, '_>, with_condition: bool) -> Vec<Item> {
    let list = holder_list(source, tokens, case);
    let mut items = Vec::new();
    let mut previous = list.inner.start;
    for statement in &list.stmts {
        let span = node_span(*statement);
        let comments = comments_between(tokens, previous, span.start);
        previous = span.end;
        let code = match statement {
            Statement::Break(_) if !with_condition => continue,
            Statement::Break(b) => match b.level.map(unparenthesize) {
                Some(Expression::Literal(Literal::Integer(i)))
                    if i.value.is_some_and(|v| v > 1) =>
                {
                    let level = i.value.expect("checked") - 1;
                    Code::Break(if level == 1 {
                        "break;".to_string()
                    } else {
                        format!("break {level};")
                    })
                }
                _ => continue,
            },
            _ => Code::Statement(span),
        };
        items.push(Item { comments, code });
    }
    items
}

fn default_edit(
    source: &str,
    tokens: &[Token],
    ancestors: &[Node<'_, '_>],
    items: &[Item],
    span: Span,
    start: usize,
    outer: usize,
) -> Edit {
    let nl = format!("\n{}", " ".repeat(outer));
    if let [item] = items {
        return Edit::new(
            Span::new(start, span.end),
            item.print(source, tokens, outer),
        );
    }
    let (list, index) = enclosing(source, tokens, ancestors);
    if list.frame == Frame::Bare {
        let inner = format!("\n{}", " ".repeat(outer + 4));
        let body: String = items
            .iter()
            .map(|item| format!("{inner}{}", item.print(source, tokens, outer + 4)))
            .collect();
        return Edit::new(span, format!("{{{body}{nl}}}"));
    }
    if index > 0 {
        let before = previous_token_end(source, tokens, start).max(list.inner.start);
        let code: String = items
            .iter()
            .map(|item| format!("{nl}{}", item.print(source, tokens, outer)))
            .collect();
        return Edit::new(Span::new(before, span.end), code);
    }
    let next = list.stmts.get(index + 1).map(|s| {
        let s = node_span(*s);
        comments_between(tokens, span.end, s.start)
            .first()
            .map_or(s.start, |c| c.start)
    });
    let code: Vec<String> = items
        .iter()
        .map(|item| item.print(source, tokens, outer))
        .collect();
    match next {
        Some(next) => Edit::new(Span::new(start, next), format!("{}{nl}", code.join(&nl))),
        None => Edit::new(Span::new(start, span.end), code.join(&nl)),
    }
}
