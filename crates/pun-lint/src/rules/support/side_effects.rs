//! Rector 2.8.4's `SideEffectNodeDetector` and `PureFunctionDetector` (src/NodeAnalyzer, src/DeadCode) on Mago's tree.
//! PHPStan's reflection is replaced by two lists from its reflection of PHP 8.4 (crates/pun-lint/data, written by
//! scripts/gen-lint-data.py): the built-in functions without side effects and the built-in `Throwable` classes. A
//! static call on a project's exception class therefore counts as a side effect here, where Rector, which reflects
//! the project's classes, sees none. `isTestMock` asks whether the receiver's type is a PHPUnit `TestCase`; without
//! types Pun takes `$this`, `self`, `static` and `parent` in a class whose parent's name ends in `TestCase`.

use std::sync::LazyLock;

use mago_syntax::cst::*;

use crate::rules::support::names::{imported_function, resolve_class_name};
use crate::rules::support::statement_edits::is_plain_assign;

static PURE: LazyLock<Vec<&str>> = LazyLock::new(|| {
    include_str!("../../../data/pure-functions.txt")
        .lines()
        .collect()
});
static THROWABLES: LazyLock<Vec<&str>> = LazyLock::new(|| {
    include_str!("../../../data/throwables.txt")
        .lines()
        .collect()
});

pub(crate) fn listed(names: &[&str], name: &str) -> bool {
    names
        .binary_search(&name.to_ascii_lowercase().as_str())
        .is_ok()
}

pub(crate) fn is_pure_call(ancestors: &[Node<'_, '_>], call: &FunctionCall<'_>) -> bool {
    let Expression::Identifier(identifier) = call.function else {
        return false;
    };
    let name = match identifier {
        Identifier::Local(local) => imported_function(ancestors, local.value)
            .unwrap_or_else(|| String::from_utf8_lossy(local.value).into_owned()),
        Identifier::FullyQualified(_) => String::from_utf8_lossy(identifier.value())
            .trim_start_matches('\\')
            .to_string(),
        Identifier::Qualified(_) => return false,
    };
    listed(&PURE, &name)
}

pub(crate) fn has_side_effect(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    if let Expression::Assignment(assignment) = crate::syntax::unparenthesize(expr)
        && is_plain_assign(assignment)
    {
        return true;
    }
    any_node(Node::Expression(expr), &|node| match node {
        Node::Expression(e) => call_has_side_effect(ancestors, e),
        _ => false,
    })
}

pub(crate) fn any_node(node: Node<'_, '_>, f: &dyn Fn(Node<'_, '_>) -> bool) -> bool {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if f(node) {
            return true;
        }
        node.visit_children(|child| stack.push(child));
    }
    false
}

/// Rector's `ExprModifiedInConditionalAnalyzer::isModified()`: `expr` holds an assignment (`=`, `op=`, `=&`) or an
/// increment or decrement, at any depth.
pub(crate) fn writes_variable(expr: &Expression<'_>) -> bool {
    any_node(Node::Expression(expr), &|n| {
        matches!(n, Node::Assignment(_) | Node::UnaryPostfix(_))
            || matches!(n, Node::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::PreIncrement(_) | UnaryPrefixOperator::PreDecrement(_)))
    })
}

pub(crate) fn call_has_side_effect(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    match expr {
        Expression::Call(Call::StaticMethod(call)) => {
            if let Expression::Identifier(class) = call.class
                && is_throwable(&resolve_class_name(ancestors, class))
            {
                return false;
            }
            !is_test_mock(ancestors, call.class, &call.method)
        }
        Expression::Call(Call::Method(call)) => !is_test_mock(ancestors, call.object, &call.method),
        Expression::Call(Call::NullSafeMethod(_)) | Expression::AnonymousClass(_) => true,
        Expression::Instantiation(new) => match new.class {
            Expression::Identifier(class) => {
                let name = resolve_class_name(ancestors, class);
                name.split('\\').next() != Some("PhpParser") || !name.contains('\\')
            }
            _ => true,
        },
        Expression::Call(Call::Function(call)) => !is_pure_call(ancestors, call),
        Expression::ArrayAccess(_) | Expression::ArrayAppend(_) => {
            let mut base = expr;
            loop {
                base = match base {
                    Expression::ArrayAccess(a) => a.array,
                    Expression::ArrayAppend(a) => a.array,
                    Expression::Parenthesized(p) => p.expression,
                    _ => break,
                };
            }
            !matches!(base, Expression::Variable(_))
        }
        _ => false,
    }
}

fn is_throwable(name: &str) -> bool {
    listed(&THROWABLES, name)
}

fn is_test_mock(
    ancestors: &[Node<'_, '_>],
    receiver: &Expression<'_>,
    method: &ClassLikeMemberSelector<'_>,
) -> bool {
    let ClassLikeMemberSelector::Identifier(name) = method else {
        return false;
    };
    if !name.value.eq_ignore_ascii_case(b"createMock")
        && !name.value.eq_ignore_ascii_case(b"createStub")
    {
        return false;
    }
    let own = match receiver {
        Expression::Variable(Variable::Direct(v)) => v.name == b"$this",
        Expression::Self_(_) | Expression::Static(_) | Expression::Parent(_) => true,
        _ => false,
    };
    own && ancestors.iter().rev().any(|node| match node {
        Node::Class(class) => class.extends.as_ref().is_some_and(|extends| {
            extends.types.iter().any(|parent| {
                let parent = parent.value();
                parent.len() >= 8 && parent[parent.len() - 8..].eq_ignore_ascii_case(b"TestCase")
            })
        }),
        _ => false,
    })
}
