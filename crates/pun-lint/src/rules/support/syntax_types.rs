use std::sync::LazyLock;

use mago_syntax::cst::*;

use crate::rules::support::names::called_function_name;
use crate::rules::support::side_effects::listed;
use crate::syntax::is_this;
use crate::syntax::{node_span, unparenthesize};

static BOOL_FUNCTIONS: LazyLock<Vec<&str>> = LazyLock::new(|| {
    include_str!("../../../data/bool-functions.txt")
        .lines()
        .collect()
});

pub(crate) fn is_bool(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    if let Expression::Variable(Variable::Direct(v)) = unparenthesize(expr) {
        return match parameter_hint(ancestors, v.name) {
            Some(hint) => matches!(hint, Hint::Bool(_)),
            None => local_of(ancestors, v.name, &|e| bool_value(ancestors, e)),
        };
    }
    bool_value(ancestors, expr)
}

fn bool_value(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    match unparenthesize(expr) {
        Expression::Literal(Literal::True(_) | Literal::False(_)) => true,
        Expression::Binary(b) => matches!(
            b.operator,
            BinaryOperator::Equal(_)
                | BinaryOperator::NotEqual(_)
                | BinaryOperator::Identical(_)
                | BinaryOperator::NotIdentical(_)
                | BinaryOperator::AngledNotEqual(_)
                | BinaryOperator::LessThan(_)
                | BinaryOperator::LessThanOrEqual(_)
                | BinaryOperator::GreaterThan(_)
                | BinaryOperator::GreaterThanOrEqual(_)
                | BinaryOperator::Instanceof(_)
                | BinaryOperator::And(_)
                | BinaryOperator::Or(_)
                | BinaryOperator::LowAnd(_)
                | BinaryOperator::LowOr(_)
                | BinaryOperator::LowXor(_)
        ),
        Expression::UnaryPrefix(u)
            if matches!(u.operator, UnaryPrefixOperator::ErrorControl(_)) =>
        {
            bool_value(ancestors, u.operand)
        }
        Expression::UnaryPrefix(u) => matches!(
            u.operator,
            UnaryPrefixOperator::Not(_)
                | UnaryPrefixOperator::BoolCast(..)
                | UnaryPrefixOperator::BooleanCast(..)
        ),
        Expression::Construct(Construct::Isset(_) | Construct::Empty(_)) => true,
        Expression::Call(Call::Function(call)) => called_function_name(ancestors, call.function)
            .is_some_and(|n| listed(&BOOL_FUNCTIONS, &n)),
        Expression::Call(Call::Method(call)) if is_this(call.object) => {
            own_method_returns(ancestors, &call.method, &|h| matches!(h, Hint::Bool(_)))
        }
        Expression::Call(Call::StaticMethod(call))
            if matches!(call.class, Expression::Self_(_) | Expression::Static(_)) =>
        {
            own_method_returns(ancestors, &call.method, &|h| matches!(h, Hint::Bool(_)))
        }
        _ => false,
    }
}

pub(crate) fn parameter_hint<'ast, 'arena>(
    ancestors: &[Node<'ast, 'arena>],
    name: &[u8],
) -> Option<&'ast Hint<'arena>> {
    let param = parameter(ancestors, name)?;
    if param.ellipsis.is_some() {
        return None;
    }
    param.hint.as_ref()
}

fn parameter<'ast, 'arena>(
    ancestors: &[Node<'ast, 'arena>],
    name: &[u8],
) -> Option<&'ast FunctionLikeParameter<'arena>> {
    let at = ancestors.iter().rposition(|n| {
        matches!(
            n,
            Node::Function(_) | Node::Method(_) | Node::Closure(_) | Node::ArrowFunction(_)
        )
    })?;
    let body = &ancestors[at];
    let params = match body {
        Node::Function(f) => &f.parameter_list,
        Node::Method(m) => &m.parameter_list,
        Node::Closure(c) => &c.parameter_list,
        Node::ArrowFunction(a) => &a.parameter_list,
        _ => unreachable!(),
    };
    let param = params.parameters.iter().find(|p| p.variable.name == name)?;
    let in_loop = ancestors[at..].iter().any(|n| {
        matches!(
            n,
            Node::For(_) | Node::Foreach(_) | Node::While(_) | Node::DoWhile(_)
        )
    });
    let until = match in_loop {
        true => usize::MAX,
        false => ancestors.last().map_or(usize::MAX, |n| node_span(n).end),
    };
    if written(*body, name, until) {
        return None;
    }
    Some(param)
}

fn own_method_returns(
    ancestors: &[Node<'_, '_>],
    selector: &ClassLikeMemberSelector<'_>,
    check: &dyn Fn(&Hint<'_>) -> bool,
) -> bool {
    let ClassLikeMemberSelector::Identifier(name) = selector else {
        return false;
    };
    let Some(members) = ancestors.iter().rev().find_map(|n| match n {
        Node::Class(c) => Some(&c.members),
        Node::Enum(e) => Some(&e.members),
        Node::AnonymousClass(c) => Some(&c.members),
        _ => None,
    }) else {
        return false;
    };
    members.iter().any(|m| match m {
        ClassLikeMember::Method(method) => {
            method.name.value.eq_ignore_ascii_case(name.value)
                && method
                    .return_type_hint
                    .as_ref()
                    .is_some_and(|r| check(&r.hint))
        }
        _ => false,
    })
}

pub(crate) fn local_of(
    ancestors: &[Node<'_, '_>],
    name: &[u8],
    check: &dyn Fn(&Expression<'_>) -> bool,
) -> bool {
    let Some(body) = ancestors
        .iter()
        .rev()
        .find(|n| matches!(n, Node::Function(_) | Node::Method(_) | Node::Closure(_)))
    else {
        return false;
    };
    if let Node::Closure(c) = body
        && c.use_clause
            .as_ref()
            .is_some_and(|u| u.variables.iter().any(|x| x.variable.name == name))
    {
        return false;
    }
    let assigned = std::cell::Cell::new(false);
    let other = crate::rules::support::side_effects::any_node(*body, &|n| match n {
        Node::Assignment(a) if matches!(unparenthesize(a.lhs), Expression::Variable(Variable::Direct(l)) if l.name == name) =>
        {
            assigned.set(true);
            !(matches!(a.operator, AssignmentOperator::Assign(_)) && check(a.rhs))
        }
        Node::UnaryPrefix(u) => {
            matches!(u.operator, UnaryPrefixOperator::Reference(_))
                && matches!(u.operand, Expression::Variable(Variable::Direct(d)) if d.name == name)
        }
        Node::Foreach(_) | Node::Global(_) | Node::Static(_) | Node::List(_) => {
            crate::rules::support::side_effects::any_node(
                n,
                &|m| matches!(m, Node::DirectVariable(d) if d.name == name),
            )
        }
        _ => false,
    });
    !other && assigned.get()
}

static INT_FUNCTIONS: LazyLock<Vec<&str>> = LazyLock::new(|| {
    include_str!("../../../data/int-functions.txt")
        .lines()
        .collect()
});

pub(crate) fn is_int(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    if let Expression::Variable(Variable::Direct(v)) = unparenthesize(expr) {
        return match parameter_hint(ancestors, v.name) {
            Some(hint) => matches!(hint, Hint::Integer(_)),
            None => local_of(ancestors, v.name, &|e| int_value(ancestors, e)),
        };
    }
    int_value(ancestors, expr)
}

fn int_value(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    match unparenthesize(expr) {
        Expression::Literal(Literal::Integer(_)) => true,
        Expression::UnaryPrefix(u)
            if matches!(
                u.operator,
                UnaryPrefixOperator::Negation(_) | UnaryPrefixOperator::Plus(_)
            ) =>
        {
            matches!(
                unparenthesize(u.operand),
                Expression::Literal(Literal::Integer(_))
            )
        }
        Expression::UnaryPrefix(u)
            if matches!(
                u.operator,
                UnaryPrefixOperator::IntCast(..) | UnaryPrefixOperator::IntegerCast(..)
            ) =>
        {
            true
        }
        Expression::Call(Call::Function(call)) => called_function_name(ancestors, call.function)
            .is_some_and(|n| listed(&INT_FUNCTIONS, &n)),
        Expression::Call(Call::Method(call)) if is_this(call.object) => {
            own_method_returns(ancestors, &call.method, &|h| matches!(h, Hint::Integer(_)))
        }
        Expression::Call(Call::StaticMethod(call))
            if matches!(call.class, Expression::Self_(_) | Expression::Static(_)) =>
        {
            own_method_returns(ancestors, &call.method, &|h| matches!(h, Hint::Integer(_)))
        }
        _ => false,
    }
}

pub(crate) type NonTyped<'f> = &'f dyn Fn(&Expression<'_>, Option<&Method<'_>>) -> bool;

/// `initialized(class, class's ancestors, name)`: a typed property without a default is set in the constructor.
pub(crate) type Initialized<'f> =
    &'f dyn for<'x, 'y> Fn(Node<'x, 'y>, &[Node<'x, 'y>], &[u8]) -> bool;

pub(crate) fn is_array_property(
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
    non_typed: NonTyped<'_>,
    initialized: Initialized<'_>,
) -> bool {
    let name = match unparenthesize(expr) {
        Expression::Access(Access::Property(PropertyAccess {
            object,
            property: ClassLikeMemberSelector::Identifier(id),
            ..
        })) if is_this(object) => id.value,
        Expression::Access(Access::StaticProperty(StaticPropertyAccess {
            class: Expression::Self_(_) | Expression::Static(_),
            property: Variable::Direct(v),
            ..
        })) => &v.name[1..],
        _ => return false,
    };
    own_property_is_array(ancestors, name, non_typed, initialized)
}

fn own_property_is_array(
    ancestors: &[Node<'_, '_>],
    name: &[u8],
    non_typed: NonTyped<'_>,
    initialized: Initialized<'_>,
) -> bool {
    let Some((at, class, members)) =
        ancestors
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, n)| match n {
                Node::Class(c) => Some((i, *n, &c.members)),
                Node::AnonymousClass(c) => Some((i, *n, &c.members)),
                _ => None,
            })
    else {
        return false;
    };
    // Rector 2.9.2: reading an uninitialized typed property throws, while `empty()` returns true.
    let set = |item: &PropertyItem<'_>| {
        matches!(item, PropertyItem::Concrete(_)) || initialized(class, &ancestors[..at], name)
    };
    members.iter().any(|m| match m {
        ClassLikeMember::Property(Property::Plain(p)) => {
            let Some(item) = p.items.iter().find(|i| &i.variable().name[1..] == name) else {
                return false;
            };
            match &p.hint {
                Some(hint) => matches!(hint, Hint::Array(_)) && set(item),
                None => {
                    matches!(item, PropertyItem::Concrete(c)
                            if matches!(unparenthesize(c.value), Expression::Array(_) | Expression::LegacyArray(_)))
                        && only_array_assigned(class, name, non_typed)
                }
            }
        }
        ClassLikeMember::Property(Property::Hooked(p)) => {
            matches!(p.hint, Some(Hint::Array(_))) && &p.item.variable().name[1..] == name && set(&p.item)
        }
        ClassLikeMember::Method(method) if method.name.value.eq_ignore_ascii_case(b"__construct") => {
            method.parameter_list.parameters.iter().any(|p| {
                p.is_promoted_property() && matches!(p.hint, Some(Hint::Array(_))) && &p.variable.name[1..] == name
            })
        }
        _ => false,
    })
}

fn only_array_assigned(class: Node<'_, '_>, name: &[u8], non_typed: NonTyped<'_>) -> bool {
    !breaks_array(class, None, name, non_typed)
}

fn breaks_array<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    method: Option<&'ast Method<'arena>>,
    name: &[u8],
    non_typed: NonTyped<'_>,
) -> bool {
    let method = match node {
        Node::Method(m) => Some(m),
        _ => method,
    };
    if let Node::Assignment(a) = node
        && breaks_with(a, method, name, non_typed)
    {
        return true;
    }
    let mut found = false;
    node.visit_children(|child| {
        if !found && breaks_array(child, method, name, non_typed) {
            found = true;
        }
    });
    found
}

fn breaks_with(
    a: &Assignment<'_>,
    method: Option<&Method<'_>>,
    name: &[u8],
    non_typed: NonTyped<'_>,
) -> bool {
    let mut target = unparenthesize(a.lhs);
    let mut element = false;
    loop {
        match target {
            Expression::ArrayAccess(x) => target = unparenthesize(x.array),
            Expression::ArrayAppend(x) => target = unparenthesize(x.array),
            _ => break,
        }
        element = true;
    }
    let named = match target {
        Expression::Access(Access::Property(p)) if is_this(p.object) => match &p.property {
            ClassLikeMemberSelector::Identifier(id) => id.value == name,
            _ => return true,
        },
        Expression::Access(Access::StaticProperty(p))
            if matches!(p.class, Expression::Self_(_) | Expression::Static(_)) =>
        {
            match &p.property {
                Variable::Direct(v) => &v.name[1..] == name,
                _ => return true,
            }
        }
        _ => false,
    };
    if !named || !matches!(a.operator, AssignmentOperator::Assign(_)) {
        return named && !element;
    }
    if element {
        match unparenthesize(a.rhs) {
            Expression::Variable(Variable::Direct(_)) => non_typed(a.rhs, method),
            Expression::Variable(_) => true,
            _ => false,
        }
    } else {
        !matches!(
            unparenthesize(a.rhs),
            Expression::Array(_) | Expression::LegacyArray(_)
        )
    }
}

fn written(body: Node<'_, '_>, name: &[u8], until: usize) -> bool {
    crate::rules::support::side_effects::any_node(body, &|n| {
        node_span(&n).start < until
            && match n {
                Node::Assignment(a) => {
                    matches!(unparenthesize(a.lhs), Expression::Variable(Variable::Direct(l)) if l.name == name)
                }
                Node::UnaryPrefix(u) => {
                    matches!(u.operator, UnaryPrefixOperator::Reference(_))
                        && matches!(u.operand, Expression::Variable(Variable::Direct(d)) if d.name == name)
                }
                Node::ForeachTarget(_) | Node::Global(_) | Node::Static(_) | Node::List(_) => {
                    crate::rules::support::side_effects::any_node(
                        n,
                        &|m| matches!(m, Node::DirectVariable(d) if d.name == name),
                    )
                }
                _ => false,
            }
    })
}
