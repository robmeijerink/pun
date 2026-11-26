use crate::problem::{Edit, Fix, Safety, Span};
use mago_syntax::cst::*;

use crate::rules::support::class_members::{member_from, param_used};
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::options::part_on;
use crate::rules::support::rewrites::{list_removal, merge, remove_params};
use crate::syntax::is_this;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoUnusedPrivateMethodParam {
    /// `unused` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoUnusedPrivateMethodParam {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-unused-private-method-param"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !checks(options, "unused")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
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
        let (members, left_brace, name) = match node {
            Node::Class(class) => (&class.members, class.left_brace, Some(&class.name)),
            Node::AnonymousClass(class) => (&class.members, class.left_brace, None),
            _ => return,
        };
        let mut path = ancestors.to_vec();
        path.push(node);
        let this = name.map(|n| {
            let id = Identifier::Local(*n);
            resolve_class_name(&path, &id)
        });
        let class = ClassScope {
            path: &path,
            this: this.as_deref(),
        };
        for (index, member) in members.iter().enumerate() {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            if !method
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Private(_)))
            {
                continue;
            }
            let params: Vec<&FunctionLikeParameter> =
                method.parameter_list.parameters.iter().collect();
            let removed: Vec<bool> = params.iter().map(|p| !param_used(method, p)).collect();
            if !removed.contains(&true) {
                continue;
            }
            let Some(mut edits) = caller_edits(&class, members, method, &params, &removed) else {
                continue;
            };
            let from = member_from(members, left_brace, index);
            edits.extend(remove_params(ctx, method, from, &removed));
            let edits = merge(edits);
            if edits.windows(2).any(|w| w[0].span.end > w[1].span.start) {
                continue;
            }
            out.push(Report {
                span: node_span(method),
                message: "Remove the unused parameters of this private method.".to_string(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits,
                }),
            });
        }
    }
}

struct ClassScope<'p, 'a> {
    path: &'p [Node<'a, 'a>],
    this: Option<&'p str>,
}

enum Reference<'a> {
    Caller(&'a ArgumentList<'a>),
    Callable,
    Unknown(Option<&'a ArgumentList<'a>>),
}

fn caller_edits(
    class: &ClassScope<'_, '_>,
    members: &Sequence<'_, ClassLikeMember<'_>>,
    method: &Method<'_>,
    params: &[&FunctionLikeParameter<'_>],
    removed: &[bool],
) -> Option<Vec<Edit>> {
    let mut references = Vec::new();
    for member in members {
        find_references(
            class,
            Node::ClassLikeMember(member),
            method.name.value,
            &mut references,
        );
    }
    let trailing = removed
        .iter()
        .position(|r| *r)
        .is_some_and(|first| removed[first..].iter().all(|r| *r));
    let removed_name = |key: usize, name: &[u8]| {
        removed.get(key).copied().unwrap_or(false) && params[key].variable.name[1..] == *name
    };
    let mut edits = Vec::new();
    for reference in &references {
        match reference {
            Reference::Callable => return None,
            Reference::Unknown(arguments) => {
                if !trailing {
                    return None;
                }
                let named = arguments.iter().flat_map(|a| a.arguments.iter()).any(|a| {
                    matches!(a, Argument::Named(n) if params.iter().zip(removed).any(|(p, r)| *r && p.variable.name[1..] == *n.name.value))
                });
                if named {
                    return None;
                }
            }
            Reference::Caller(arguments) => {
                let args: Vec<&Argument> = arguments.arguments.iter().collect();
                for (key, arg) in args.iter().enumerate() {
                    match arg {
                        Argument::Positional(p) if p.ellipsis.is_some() => return None,
                        Argument::Named(n) if !removed_name(key, n.name.value) => return None,
                        _ => {}
                    }
                }
                let drop: Vec<bool> = (0..args.len())
                    .map(|k| removed.get(k).copied().unwrap_or(false))
                    .collect();
                if !drop.contains(&true) {
                    continue;
                }
                let items: Vec<Span> = args.iter().map(|a| node_span(*a)).collect();
                match list_removal(&items, &drop) {
                    Some(list) => edits.extend(list),
                    None => edits.push(Edit::new(node_span(*arguments), "()".to_string())),
                }
            }
        }
    }
    Some(edits)
}

fn find_references<'a>(
    class: &ClassScope<'_, '_>,
    node: Node<'a, 'a>,
    name: &[u8],
    out: &mut Vec<Reference<'a>>,
) {
    let named = |selector: &ClassLikeMemberSelector<'_>| match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value.eq_ignore_ascii_case(name)),
        _ => None,
    };
    match node {
        Node::AnonymousClass(c) => {
            if let Some(args) = &c.argument_list {
                find_references(class, Node::PartialArgumentList(args), name, out);
            }
            return;
        }
        Node::MethodCall(call) => match named(&call.method) {
            Some(false) => {}
            Some(true) if is_this(call.object) => out.push(Reference::Caller(&call.argument_list)),
            _ => out.push(Reference::Unknown(Some(&call.argument_list))),
        },
        Node::NullSafeMethodCall(call) if named(&call.method) != Some(false) => {
            out.push(Reference::Unknown(Some(&call.argument_list)));
        }
        Node::StaticMethodCall(call) => match (named(&call.method), receiver(class, call.class)) {
            (Some(false), _) | (_, Some(false)) => {}
            (Some(true), Some(true)) => out.push(Reference::Caller(&call.argument_list)),
            _ => out.push(Reference::Unknown(Some(&call.argument_list))),
        },
        Node::MethodPartialApplication(call) if named(&call.method) != Some(false) => {
            out.push(if is_this(call.object) {
                Reference::Callable
            } else {
                Reference::Unknown(None)
            });
        }
        Node::StaticMethodPartialApplication(call) if named(&call.method) != Some(false) => {
            match receiver(class, call.class) {
                Some(false) => {}
                Some(true) => out.push(Reference::Callable),
                None => out.push(Reference::Unknown(None)),
            }
        }
        // Rector 2.9.2's `isUsedAsArrayCallable()`: `[$this, 'method']` or `[self::class, $dynamic]` hands the
        // method to code that decides how many arguments it passes; `[Other::class, 'method']` names another method.
        Node::Array(Array { elements, .. }) | Node::LegacyArray(LegacyArray { elements, .. }) => {
            if let [ArrayElement::Value(first), ArrayElement::Value(second)] = elements.as_slice()
                && let Some(own) = callable_class(class, first.value)
            {
                match unparenthesize(second.value) {
                    Expression::Literal(Literal::String(s)) => {
                        if !own {
                            return;
                        }
                        if s.value.is_some_and(|v| v.eq_ignore_ascii_case(name)) {
                            out.push(Reference::Callable);
                        }
                    }
                    _ => out.push(Reference::Callable),
                }
            }
        }
        Node::LiteralString(s) if s.value.is_some_and(|v| v.eq_ignore_ascii_case(name)) => {
            out.push(Reference::Unknown(None));
        }
        _ => {}
    }
    node.visit_children(|child| find_references(class, child, name, out));
}

/// The class an array callable's first item names: `Some(true)` for this class (`$this`, `self::class`,
/// `static::class`, `__CLASS__`), `Some(false)` for another one, `None` when it names no class.
fn callable_class(class: &ClassScope<'_, '_>, expr: &Expression<'_>) -> Option<bool> {
    match unparenthesize(expr) {
        e if is_this(e) => Some(true),
        Expression::MagicConstant(MagicConstant::Class(_)) => Some(true),
        Expression::Access(Access::ClassConstant(fetch)) if matches!(&fetch.constant, ClassLikeConstantSelector::Identifier(c) if c.value.eq_ignore_ascii_case(b"class")) => {
            receiver(class, fetch.class)
        }
        _ => None,
    }
}

fn receiver(class: &ClassScope<'_, '_>, expr: &Expression<'_>) -> Option<bool> {
    match expr {
        Expression::Self_(_) | Expression::Static(_) => Some(true),
        Expression::Parent(_) => Some(false),
        Expression::Identifier(id) => {
            let resolved = resolve_class_name(class.path, id);
            Some(
                class
                    .this
                    .is_some_and(|this| this.eq_ignore_ascii_case(&resolved)),
            )
        }
        e if is_this(e) => Some(true),
        _ => None,
    }
}

const CHECKS: [&str; 2] = ["overridden-in-tests", "unused"];

/// Pun's `checks`: the ones that run, one for each of Rector's 2 rules (`overridden-in-tests` is
/// `RemoveTestsOverriddenPrivateMethodParameterRector`, `unused` is `RemoveUnusedPrivateMethodParameterRector`).
pub(super) fn checks(options: &serde_json::Value, check: &str) -> Result<bool, OptionsError> {
    part_on(options, "checks", &CHECKS, check)
}
