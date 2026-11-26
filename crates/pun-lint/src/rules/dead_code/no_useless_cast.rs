use crate::problem::{Edit, Fix, Safety};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::atomic::scalar::string::TStringCasing;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{
    Access, BinaryOperator, ClassLikeMemberSelector, Expression, Hint, Literal, Node, NodeKind,
    UnaryPrefixOperator, Variable,
};

use crate::rules::support::class_members::{object_class_names, qualified, walk};
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::statements::{keeps_parentheses, parent_of};
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoUselessCast;

#[derive(PartialEq, Clone, Copy)]
enum Kind {
    String,
    Bool,
    Int,
    Float,
    Object,
    Array,
    ConstantArray,
    Mixed,
    Other,
}

impl Rule for NoUselessCast {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-useless-cast"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::UnaryPrefix])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::UnaryPrefix(c) = node else { return };
        let wanted = match c.operator {
            UnaryPrefixOperator::StringCast(..) | UnaryPrefixOperator::BinaryCast(..) => {
                Kind::String
            }
            UnaryPrefixOperator::BoolCast(..) | UnaryPrefixOperator::BooleanCast(..) => Kind::Bool,
            UnaryPrefixOperator::ArrayCast(..) => Kind::Array,
            UnaryPrefixOperator::IntCast(..) | UnaryPrefixOperator::IntegerCast(..) => Kind::Int,
            UnaryPrefixOperator::ObjectCast(..) => Kind::Object,
            UnaryPrefixOperator::DoubleCast(..)
            | UnaryPrefixOperator::RealCast(..)
            | UnaryPrefixOperator::FloatCast(..) => Kind::Float,
            _ => return,
        };
        let expr = unparenthesize(c.operand);
        let facts = facts(ctx);
        if matches!(expr, Expression::ConstantAccess(_)) {
            return;
        }
        let Some(native) = facts.native_type(node_span(expr)) else {
            return;
        };
        let mut kind = kind(native);
        if let Expression::Binary(b) = expr
            && matches!(b.operator, BinaryOperator::Exponentiation(_))
            && kind == Kind::Int
            && !(is_int_literal(b.lhs) && is_int_literal(b.rhs))
        {
            kind = Kind::Other;
        }
        let same = kind == wanted || (kind == Kind::ConstantArray && wanted == Kind::Array);
        if !same || should_skip(&facts, ancestors, expr, native, wanted) {
            return;
        }
        let span = node_span(expr);
        let text = &ctx.source[span.start..span.end];
        let text = if keeps_parentheses(expr, parent_of(ancestors)) {
            format!("({text})")
        } else {
            text.to_string()
        };
        let span = node_span(c);
        out.push(Report {
            span,
            message: "This cast does not change the value's type; remove it.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, text)],
            }),
        });
    }
}

fn is_int_literal(expr: &Expression<'_>) -> bool {
    matches!(
        unparenthesize(expr),
        Expression::Literal(Literal::Integer(_))
    )
}

fn kind(t: &TUnion) -> Kind {
    match t.types.as_ref() {
        [atomic] => atomic_kind(atomic),
        [a, b] if is_bool(a) && is_bool(b) => Kind::Bool,
        types if types.iter().all(|a| matches!(a, TAtomic::Array(_))) => Kind::Array,
        _ => Kind::Other,
    }
}

fn is_bool(atomic: &TAtomic) -> bool {
    matches!(atomic, TAtomic::Scalar(TScalar::Bool(_)))
}

fn atomic_kind(atomic: &TAtomic) -> Kind {
    match atomic {
        TAtomic::Mixed(_) => Kind::Mixed,
        TAtomic::Scalar(TScalar::String(s))
            if s.literal.is_none() && s.casing != TStringCasing::Unspecified =>
        {
            Kind::Other
        }
        TAtomic::Scalar(TScalar::String(_)) => Kind::String,
        TAtomic::Scalar(TScalar::ClassLikeString(_)) => Kind::String,
        TAtomic::Scalar(TScalar::Integer(_)) => Kind::Int,
        TAtomic::Scalar(TScalar::Float(_)) => Kind::Float,
        TAtomic::Scalar(TScalar::Bool(_)) => Kind::Bool,
        TAtomic::Array(TArray::Keyed(k)) if k.parameters.is_none() => Kind::ConstantArray,
        TAtomic::Array(_) => Kind::Array,
        TAtomic::Object(TObject::Named(n)) if !n.is_this => Kind::Object,
        TAtomic::Object(TObject::Enum(_)) => Kind::Object,
        _ => Kind::Other,
    }
}

fn should_skip(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
    native: &TUnion,
    wanted: Kind,
) -> bool {
    match expr {
        Expression::ArrayAccess(_) => true,
        Expression::Access(Access::Property(p)) => {
            let class = match unparenthesize(p.object) {
                Expression::Variable(Variable::Direct(v)) if v.name == b"$this" => {
                    this_class(facts, ancestors)
                }
                object => {
                    let span = node_span(object);
                    let t = facts.full_type(span).or_else(|| facts.native_type(span));
                    match t.map(object_class_names) {
                        Some(names) if names.len() == 1 => Some(names[0].clone()),
                        _ => None,
                    }
                }
            };
            if let ClassLikeMemberSelector::Identifier(id) = &p.property
                && let Some(class) = class
            {
                !has_native_type(
                    facts,
                    &class,
                    &format!("${}", String::from_utf8_lossy(id.value)),
                )
            } else {
                true
            }
        }
        Expression::Access(Access::StaticProperty(p)) => {
            let class = match p.class {
                Expression::Identifier(id) => Some(resolve_class_name(ancestors, id)),
                Expression::Self_(_) | Expression::Static(_) => this_class(facts, ancestors),
                Expression::Parent(_) => this_class(facts, ancestors).and_then(|c| {
                    facts
                        .codebase
                        .get_class_like(c.as_bytes())
                        .and_then(|m| m.direct_parent_class.map(|p| p.to_string()))
                }),
                class => match facts.full_type(node_span(class)).map(object_class_names) {
                    Some(names) if names.len() == 1 => Some(names[0].clone()),
                    _ => None,
                },
            };
            if let Variable::Direct(v) = &p.property
                && let Some(class) = class
            {
                !has_native_type(facts, &class, &String::from_utf8_lossy(v.name))
            } else {
                true
            }
        }
        Expression::Variable(Variable::Direct(v)) => {
            native.possibly_undefined()
                || declared_otherwise(ancestors, v.name, wanted)
                || (wanted == Kind::String && is_foreach_key(ancestors, v.name))
                || facts
                    .full_type(node_span(expr))
                    .is_some_and(|full| !within(full, native))
                || names_variable(ancestors, node_span(expr).start)
        }
        _ => false,
    }
}

/// Whether `name` is a parameter of the closure or arrow function around it, declared without a type or with one other
/// than `wanted`. The analyzer narrows such a parameter to the type the callable it is passed to expects (`uksort()`'s
/// keys), where Rector reads the declared type (`int|string`, or `mixed`).
fn declared_otherwise(ancestors: &[Node<'_, '_>], name: &[u8], wanted: Kind) -> bool {
    let parameters = match ancestors.iter().rev().find(|a| {
        matches!(
            a,
            Node::Function(_) | Node::Method(_) | Node::Closure(_) | Node::ArrowFunction(_)
        )
    }) {
        Some(Node::Closure(c)) => &c.parameter_list.parameters,
        Some(Node::ArrowFunction(f)) => &f.parameter_list.parameters,
        _ => return false,
    };
    parameters.iter().any(|p| {
        p.variable.name == name
            && p.hint.as_ref().is_none_or(|hint| {
                let declared = match hint {
                    Hint::String(_) => Kind::String,
                    Hint::Integer(_) => Kind::Int,
                    Hint::Float(_) => Kind::Float,
                    Hint::Bool(_) => Kind::Bool,
                    Hint::Array(_) => Kind::Array,
                    _ => Kind::Other,
                };
                declared != wanted
            })
    })
}

/// Whether `name` is a `foreach` key in the function around it. A key PHP stores from a numeric string (`'20'`) is an
/// `int`, though the analyzer, like PHPStan and Rector, types the keys of `$a[$string] = ...` as `string`: removing the
/// cast would pass an `int` where a string is needed.
fn is_foreach_key(ancestors: &[Node<'_, '_>], name: &[u8]) -> bool {
    let Some(root) = ancestors.iter().rev().find(|a| {
        matches!(
            a,
            Node::Function(_) | Node::Method(_) | Node::Closure(_) | Node::ArrowFunction(_)
        )
    }) else {
        return false;
    };
    let mut found = false;
    walk(*root, &mut |node, path| {
        if found
            || (!path.is_empty()
                && matches!(
                    node,
                    Node::Function(_) | Node::Closure(_) | Node::ArrowFunction(_) | Node::Class(_)
                ))
        {
            return false;
        }
        if let Node::ForeachKeyValueTarget(t) = node
            && let Expression::Variable(Variable::Direct(k)) = unparenthesize(t.key)
            && k.name == name
        {
            found = true;
        }
        !found
    });
    found
}

pub(crate) fn names_variable(ancestors: &[Node<'_, '_>], at: usize) -> bool {
    let Some(root) = ancestors.iter().rev().find(|a| {
        matches!(
            a,
            Node::Function(_) | Node::Method(_) | Node::Closure(_) | Node::ArrowFunction(_)
        )
    }) else {
        return false;
    };
    let mut names: Vec<&[u8]> = Vec::new();
    let mut values: Vec<Vec<u8>> = Vec::new();
    walk(*root, &mut |node, path| {
        if !path.is_empty()
            && matches!(
                node,
                Node::Function(_) | Node::Closure(_) | Node::ArrowFunction(_) | Node::Class(_)
            )
        {
            return false;
        }
        match node {
            Node::DirectVariable(v) if v.span.start.offset as usize <= at => {
                names.push(&v.name[1..]);
            }
            Node::Assignment(a) if node_span(a).start < at => {
                if let (
                    Expression::Variable(Variable::Direct(_)),
                    Expression::Literal(Literal::String(s)),
                ) = (a.lhs, unparenthesize(a.rhs))
                    && let Some(value) = s.value
                {
                    values.push(value.to_vec());
                }
            }
            _ => {}
        }
        true
    });
    values.iter().any(|v| names.contains(&v.as_slice()))
}

fn this_class(facts: &Facts, ancestors: &[Node<'_, '_>]) -> Option<String> {
    ancestors
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, a)| match a {
            Node::Class(c) => Some(qualified(&ancestors[..i], c.name.value)),
            Node::Enum(e) => Some(qualified(&ancestors[..i], e.name.value)),
            Node::Trait(t) => Some(qualified(&ancestors[..i], t.name.value)),
            Node::AnonymousClass(c) => {
                Some(facts.anonymous_class_name(mago_span::HasSpan::span(c)))
            }
            _ => None,
        })
}

fn has_native_type(facts: &Facts, class: &str, name: &str) -> bool {
    if facts
        .codebase
        .get_class_like(class.as_bytes())
        .is_some_and(|c| c.kind.is_enum())
    {
        return false;
    }
    facts
        .codebase
        .get_declaring_property(class.as_bytes(), name.as_bytes())
        .and_then(|p| p.type_declaration_metadata.as_ref())
        .is_some_and(|t| !t.type_union.is_mixed())
}

pub(crate) fn within(full: &TUnion, native: &TUnion) -> bool {
    let constant = |a: &TAtomic| match a {
        TAtomic::Array(TArray::Keyed(k)) => k.parameters.is_none() && k.known_items.is_some(),
        TAtomic::Array(TArray::List(l)) => l.known_elements.is_some() && l.element_type.is_never(),
        _ => false,
    };
    if native.types.iter().any(constant) {
        return full.types.iter().all(|a| native.types.contains(a));
    }
    let wanted = kind(native);
    full.types.iter().all(|a| {
        let k = atomic_kind(a);
        k == wanted
            || (wanted == Kind::String && matches!(a, TAtomic::Scalar(TScalar::String(_))))
            || (wanted == Kind::Array && matches!(a, TAtomic::Array(_)))
    })
}
