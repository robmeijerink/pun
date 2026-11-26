use crate::problem::{Edit, Fix, Safety, Span};
use mago_codex::metadata::CodebaseMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::reference::TReference;
use mago_codex::ttype::union::TUnion;
use mago_span::HasSpan;
use mago_syntax::cst::{Argument, Expression, Instantiation, Node, NodeKind, Variable};

use crate::rules::support::conditions::{indentation, print_comments, reprint};
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::statements::previous_token_end;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct RequirePreviousException;

impl Rule for RequirePreviousException {
    fn id(&self) -> &'static str {
        "lint/complexity/require-previous-exception"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Throw])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Throw(t) = node else { return };
        let Expression::Instantiation(new) = t.exception else {
            return;
        };
        let caught = ancestors.iter().filter_map(|a| match a {
            Node::TryCatchClause(c) => c.variable.as_ref().map(|v| v.name),
            _ => None,
        });
        for variable in caught {
            let Some(text) = refactor(
                ctx,
                ancestors,
                t.span().start.offset as usize,
                new,
                variable,
            ) else {
                continue;
            };
            out.push(Report {
                span: node_span(t.exception),
                message: "Pass the caught exception on to the exception thrown in its `catch`."
                    .into(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits: vec![Edit::new(node_span(t.exception), text)],
                }),
            });
            return;
        }
    }
}

struct Arg {
    name: Option<String>,
    value: String,
    written: Option<Span>,
    start: usize,
}

fn refactor(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    throw_start: usize,
    new: &Instantiation<'_>,
    variable: &[u8],
) -> Option<String> {
    let source = ctx.source;
    let class = match new.class {
        Expression::Identifier(name) => Some(resolve_class_name(ancestors, name)),
        Expression::Self_(_) | Expression::Static(_) | Expression::Parent(_) => None,
        _ => return None,
    };
    let facts = facts(ctx);
    let codebase = facts.documented();
    let constructor = class
        .as_deref()
        .filter(|c| codebase.get_class_like(c.as_bytes()).is_some())
        .and_then(|c| codebase.get_declaring_method(c.as_bytes(), b"__construct"));
    let has_parameter = |name: &str| {
        constructor.is_some_and(|m| {
            m.parameters
                .iter()
                .any(|p| &p.get_name().0.as_bytes()[1..] == name.as_bytes())
        })
    };
    let position = match constructor {
        None => 2,
        Some(m) => m.parameters.iter().position(|p| {
            let t = p.type_metadata.as_ref().map(|t| &t.type_union);
            takes_throwable(codebase, t)
        })?,
    };
    let written: Vec<&Argument<'_>> = new
        .argument_list
        .as_ref()
        .map_or_default(|l| l.arguments.iter().collect());
    if written.len() > position {
        return None;
    }
    let mut args: Vec<Arg> = written
        .iter()
        .map(|a| Arg {
            name: match a {
                Argument::Named(n) => Some(String::from_utf8_lossy(n.name.value).into_owned()),
                Argument::Positional(_) => None,
            },
            value: String::new(),
            written: Some(node_span(*a)),
            start: node_span(*a).start,
        })
        .collect();
    let has_argument =
        |args: &[Arg], name: &str| args.iter().any(|a| a.name.as_deref() == Some(name));
    let named = args.first().is_some_and(|a| a.name.is_some());
    let var = String::from_utf8_lossy(variable).into_owned();
    let has_code_parameter = has_parameter("code");
    let has_code_argument = has_argument(&args, "code");
    if args.len() < 2 && (!has_code_parameter || has_code_argument) {
        return None;
    }
    let mut changed = false;
    if args.is_empty() {
        args.push(new_arg(None, format!("{var}->getMessage()")));
        changed = true;
    } else if let Some(Argument::Named(first)) = written.first()
        && first.name.value == b"previous"
        && is_variable(first.value, variable)
    {
        args[0] = Arg {
            name: Some("message".into()),
            value: format!("{var}->getMessage()"),
            written: Some(Span::new(
                node_span(&first.name).end,
                node_span(first.value).start,
            )),
            start: node_span(&first.name).start,
        };
        changed = true;
    }
    let code_name = named.then(|| "code".to_string());
    if args.len() < 2 {
        args.push(new_arg(code_name, format!("{var}->getCode()")));
        changed = true;
    }
    if args[1].name.as_deref() == Some("previous") {
        // Rector 2.9.2: previous is already passed, code is omitted on purpose.
        if !changed {
            return None;
        }
    } else if has_parameter("previous") && !has_argument(&args, "previous") {
        let count = if named {
            1
        } else {
            (position + 1).saturating_sub(args.len()).max(1)
        };
        for _ in 0..count {
            args.push(new_arg(named.then(|| "previous".to_string()), var.clone()));
        }
    } else if !changed {
        return None;
    }
    Some(print_new(source, throw_start, new, &args))
}

fn new_arg(name: Option<String>, value: String) -> Arg {
    Arg {
        name,
        value,
        written: None,
        start: 0,
    }
}

fn print_new(source: &str, throw_start: usize, new: &Instantiation<'_>, args: &[Arg]) -> String {
    let level = indentation(source, throw_start);
    let tokens = crate::rules::support::statements::lex(source);
    let mut items: Vec<(Vec<Span>, &Arg)> = Vec::new();
    for arg in args {
        let comments = match arg.written {
            Some(_) => {
                let start = previous_token_end(source, &tokens, arg.start);
                tokens
                    .iter()
                    .filter(|t| t.start >= start && t.end <= arg.start && t.kind.is_comment())
                    .map(|t| Span::new(t.start, t.end))
                    .collect()
            }
            None => Vec::new(),
        };
        items.push((comments, arg));
    }
    let multiline = items.iter().any(|(c, _)| !c.is_empty());
    let item_level = if multiline { level + 4 } else { level };
    let print = |arg: &Arg| match (arg.written, &arg.name) {
        (Some(span), _) if arg.value.is_empty() => {
            let adjust = item_level as isize - indentation(source, span.start) as isize;
            reprint(source, &tokens, span, adjust)
        }
        (Some(span), Some(name)) => {
            format!("{name}{}{}", &source[span.start..span.end], arg.value)
        }
        (_, Some(name)) => format!("{name}: {}", arg.value),
        (_, None) => arg.value.clone(),
    };
    let class = &source[node_span(new.class).start..node_span(new.class).end];
    let list = if multiline {
        let nl = format!("\n{}", " ".repeat(item_level));
        let lines: Vec<String> = items
            .iter()
            .map(|(comments, arg)| {
                let mut line = String::new();
                if !comments.is_empty() {
                    line.push_str(&nl);
                    line.push_str(&print_comments(source, comments, &nl));
                }
                line.push_str(&nl);
                line.push_str(&print(arg));
                line
            })
            .collect();
        format!("{}\n{}", lines.join(","), " ".repeat(level))
    } else {
        items
            .iter()
            .map(|(_, a)| print(a))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("new {class}({list})")
}

fn is_variable(value: &Expression<'_>, variable: &[u8]) -> bool {
    matches!(unparenthesize(value), Expression::Variable(Variable::Direct(d)) if d.name == variable)
}

fn takes_throwable(codebase: &CodebaseMetadata, t: Option<&TUnion>) -> bool {
    let Some(t) = t else {
        return false;
    };
    let mut classes: Vec<String> = t.types.iter().filter_map(class_name).collect();
    classes.dedup();
    if let [class] = &classes[..] {
        if t.types.len() == 1 {
            return throwable_of(codebase, class) != Some(false);
        }
        return throwable_of(codebase, class) == Some(true);
    }
    let kept: Vec<&TAtomic> = t
        .types
        .iter()
        .filter(|a| !matches!(a, TAtomic::Null))
        .collect();
    !kept.is_empty()
        && kept
            .iter()
            .all(|a| class_name(a).is_some_and(|c| throwable_of(codebase, &c) == Some(true)))
}

fn class_name(atomic: &TAtomic) -> Option<String> {
    let name = match atomic {
        TAtomic::Object(TObject::Named(n)) => n.name,
        TAtomic::Reference(TReference::Symbol { name, .. }) => *name,
        _ => return None,
    };
    Some(String::from_utf8_lossy(name.as_bytes()).into_owned())
}

fn throwable_of(codebase: &CodebaseMetadata, class: &str) -> Option<bool> {
    let metadata = codebase.get_class_like(class.as_bytes())?;
    if codebase.is_instance_of(class.as_bytes(), b"Throwable") {
        return Some(true);
    }
    if metadata.kind.is_trait() || metadata.kind.is_enum() || metadata.flags.is_final() {
        return Some(false);
    }
    None
}
