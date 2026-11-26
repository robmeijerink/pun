use crate::problem::{Edit, Fix, Safety, Span};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::*;

use crate::rules::phpunit::is_in_test_class;
use crate::rules::support::class_members::member_from;
use crate::rules::support::class_members::{ClassInfo, class_info, is_private, type_of, walk};
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::rewrites::{list_removal, merge, remove_params};
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{is_this, node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoUnusedTestMethodParam {
    /// `overridden-in-tests` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoUnusedTestMethodParam {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-unused-private-method-param"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !super::no_unused_private_method_param::checks(options, "overridden-in-tests")?;
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
        let facts = facts(ctx);
        let Some(class) = class_info(node, ancestors, &facts) else {
            return;
        };
        let (members, left_brace) = (class.members, class.left_brace);
        let candidate = |m: &Method<'_>| {
            is_private(m)
                && !m.name.value.eq_ignore_ascii_case(b"__construct")
                && !m.parameter_list.parameters.is_empty()
        };
        if !members
            .iter()
            .any(|m| matches!(m, ClassLikeMember::Method(m) if candidate(m)))
        {
            return;
        }
        let mut path = ancestors.to_vec();
        path.push(node);
        if !is_in_test_class(ctx, &path, None) {
            return;
        }
        for (index, member) in members.iter().enumerate() {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            if !candidate(method) {
                continue;
            }
            let removed = overridden_parameters(method);
            if !removed.contains(&true) {
                continue;
            }
            let Some(callers) = caller_edits(&facts, ancestors, &class, method, &removed) else {
                continue;
            };
            let from = member_from(members, left_brace, index);
            let mut edits = callers.clone();
            edits.extend(remove_params(ctx, method, from, &removed));
            let mut edits = merge(edits);
            if edits.windows(2).any(|w| w[0].span.end > w[1].span.start) {
                edits = merge(callers);
            }
            out.push(Report {
                span: node_span(method),
                message: "This parameter is overwritten before it is read; remove it.".to_string(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits,
                }),
            });
        }
    }
}

fn overridden_parameters(method: &Method<'_>) -> Vec<bool> {
    let params: Vec<&FunctionLikeParameter<'_>> = method.parameter_list.parameters.iter().collect();
    let none = vec![false; params.len()];
    let MethodBody::Concrete(body) = &method.body else {
        return none;
    };
    let stmts: Vec<&Statement<'_>> = (body.statements.iter())
        .filter(|s| !matches!(s, Statement::Noop(_)))
        .collect();
    if stmts.is_empty() || has_indirect_variable_usage(Node::Method(method)) {
        return none;
    }
    params
        .iter()
        .map(|p| {
            p.ampersand.is_none()
                && p.ellipsis.is_none()
                && p.modifiers.is_empty()
                && p.hooks.is_none()
                && is_overridden_before_first_use(&stmts, p.variable.name)
        })
        .collect()
}

fn is_overridden_before_first_use(stmts: &[&Statement<'_>], name: &[u8]) -> bool {
    for stmt in stmts {
        let found = variables_named(Node::Statement(stmt), name);
        if found.is_empty() {
            continue;
        }
        if found.len() != 1 {
            return false;
        }
        let Statement::Expression(e) = stmt else {
            return false;
        };
        let Expression::Assignment(a) = unparenthesize(e.expression) else {
            return false;
        };
        if !matches!(a.operator, AssignmentOperator::Assign(_))
            || matches!(a.rhs, Expression::UnaryPrefix(u) if u.operator.is_reference())
        {
            return false;
        }
        return node_span(unparenthesize(a.lhs)) == found[0];
    }
    false
}

fn variables_named(node: Node<'_, '_>, name: &[u8]) -> Vec<Span> {
    let mut out = Vec::new();
    walk(node, &mut |n, path| {
        if let Node::DirectVariable(v) = n
            && v.name == name
            && !matches!(
                path.iter()
                    .rev()
                    .find(|p| !matches!(p, Node::Variable(_) | Node::Expression(_))),
                Some(
                    Node::PropertyAbstractItem(_)
                        | Node::PropertyConcreteItem(_)
                        | Node::StaticPropertyAccess(_)
                )
            )
        {
            out.push(node_span(&n));
        }
        true
    });
    out
}

fn has_indirect_variable_usage(node: Node<'_, '_>) -> bool {
    const NAMES: [&[u8]; 6] = [
        b"compact",
        b"extract",
        b"func_get_arg",
        b"func_get_args",
        b"func_num_args",
        b"get_defined_vars",
    ];
    let mut found = false;
    walk(node, &mut |n, _| {
        found = found
            || match n {
                Node::IndirectVariable(_) | Node::NestedVariable(_) => true,
                Node::FunctionCall(call) => match call.function {
                    Expression::Identifier(id) => {
                        let name = id
                            .value()
                            .rsplit(|b| *b == b'\\')
                            .next()
                            .unwrap_or_default();
                        NAMES.iter().any(|n| n.eq_ignore_ascii_case(name))
                    }
                    _ => false,
                },
                _ => false,
            };
        !found
    });
    found
}

enum Caller<'a> {
    Call(&'a ArgumentList<'a>),
    Callable,
}

fn caller_edits(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    method: &Method<'_>,
    removed: &[bool],
) -> Option<Vec<Edit>> {
    let params: Vec<&FunctionLikeParameter<'_>> = method.parameter_list.parameters.iter().collect();
    let name = method.name.value;
    let mut callers = Vec::new();
    for member in class.members {
        let ClassLikeMember::Method(m) = member else {
            continue;
        };
        walk(Node::Method(m), &mut |n, _| {
            let (receiver, selector, caller, is_static) = match n {
                Node::MethodCall(c) => (c.object, &c.method, Caller::Call(&c.argument_list), false),
                Node::StaticMethodCall(c) => {
                    (c.class, &c.method, Caller::Call(&c.argument_list), true)
                }
                Node::MethodPartialApplication(c) => (c.object, &c.method, Caller::Callable, false),
                Node::StaticMethodPartialApplication(c) => {
                    (c.class, &c.method, Caller::Callable, true)
                }
                // Rector 2.9.2's `isUsedAsArrayCallable()`: the code receiving `[$this, 'method']` decides how many
                // arguments it passes
                Node::Array(Array { elements, .. })
                | Node::LegacyArray(LegacyArray { elements, .. }) => {
                    if let [ArrayElement::Value(first), ArrayElement::Value(second)] =
                        elements.as_slice()
                    {
                        let own = match unparenthesize(first.value) {
                            Expression::MagicConstant(MagicConstant::Class(_)) => true,
                            Expression::Access(Access::ClassConstant(fetch)) if matches!(&fetch.constant, ClassLikeConstantSelector::Identifier(c) if c.value.eq_ignore_ascii_case(b"class")) => {
                                is_class(facts, ancestors, class, fetch.class, true)
                            }
                            e => is_this(e),
                        };
                        let named = match unparenthesize(second.value) {
                            Expression::Literal(Literal::String(s)) => {
                                s.value.is_some_and(|v| v.eq_ignore_ascii_case(name))
                            }
                            _ => true,
                        };
                        if own && named {
                            callers.push(Caller::Callable);
                        }
                    }
                    return true;
                }
                _ => return true,
            };
            if matches!(selector, ClassLikeMemberSelector::Identifier(id) if id.value.eq_ignore_ascii_case(name))
                && is_class(facts, ancestors, class, receiver, is_static)
            {
                callers.push(caller);
            }
            true
        });
    }
    let removed_name = |key: usize, arg: &[u8]| {
        removed.get(key).copied().unwrap_or(false) && params[key].variable.name[1..] == *arg
    };
    let mut edits = Vec::new();
    for caller in &callers {
        let Caller::Call(arguments) = caller else {
            return None;
        };
        let args: Vec<&Argument<'_>> = arguments.arguments.iter().collect();
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
    Some(edits)
}

fn is_class(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    receiver: &Expression<'_>,
    is_static: bool,
) -> bool {
    let name = class.name.as_bytes();
    if is_static {
        return match receiver {
            Expression::Self_(_) | Expression::Static(_) => true,
            Expression::Parent(_) => false,
            Expression::Identifier(id) => {
                let resolved = resolve_class_name(ancestors, id);
                resolved.eq_ignore_ascii_case(&class.name)
                    || facts.codebase.is_instance_of(resolved.as_bytes(), name)
            }
            other => is_of_class(facts, other, name),
        };
    }
    is_of_class(facts, receiver, name)
}

fn is_of_class(facts: &Facts, expr: &Expression<'_>, name: &[u8]) -> bool {
    let Some(t) = type_of(facts, expr) else {
        return false;
    };
    let mut types = (t.types.iter())
        .filter(|a| !matches!(a, TAtomic::Null) && !matches!(a, TAtomic::Scalar(s) if s.is_false()))
        .peekable();
    types.peek().is_some()
        && types.all(|a| match a {
            TAtomic::Object(TObject::Named(n)) => {
                n.name.as_bytes().eq_ignore_ascii_case(name)
                    || facts.codebase.is_instance_of(n.name.as_bytes(), name)
            }
            _ => false,
        })
}
