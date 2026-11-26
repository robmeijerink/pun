use crate::problem::{Edit, Safety};
use mago_codex::metadata::CodebaseMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{
    Argument, ArrayElement, AssignmentOperator, Call, Expression, Method, MethodBody, Node,
    NodeKind, Statement, UnaryPrefixOperator, Variable,
};

use super::annotations::named_tags;
use super::no_bare_mock_assign::scoped;
use super::{AnyCall, is_public, object_class, own_class};
use crate::rules::support::docblocks::Doc;
use crate::rules::support::names::has_attribute;
use crate::rules::support::side_effects::any_node;
use crate::rules::support::statement_edits::{assign, expression};
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const MOCK_OBJECT: &str = "PHPUnit\\Framework\\MockObject\\MockObject";

pub struct PreferCreateStubExpression;

impl Rule for PreferCreateStubExpression {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-create-stub-expression"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Method(method) = node else { return };
        let MethodBody::Concrete(block) = &method.body else {
            return;
        };
        let is_test = is_test_class_method(ctx, ancestors, method);
        let statements: Vec<&Statement<'_>> = block
            .statements
            .iter()
            .filter(|s| !matches!(s, Statement::Noop(_)))
            .collect();
        if !is_test || statements.len() < 2 {
            return;
        }
        let roots: Vec<Node<'_, '_>> = statements.iter().map(|s| Node::Statement(s)).collect();
        for statement in &statements {
            let Some(assign) = expression(statement).and_then(assign) else {
                continue;
            };
            let Expression::Variable(Variable::Direct(variable)) = unparenthesize(assign.lhs)
            else {
                continue;
            };
            let Expression::Call(Call::Method(call)) = unparenthesize(assign.rhs) else {
                continue;
            };
            let call = AnyCall::of(Node::MethodCall(call)).unwrap();
            if !call.is_name(&["createMock"]) || call.args().is_empty() {
                continue;
            }
            if is_used_for_mocking(ctx, ancestors, &roots, &variable.name[1..], false) {
                continue;
            }
            // the mock can be configured through an alias, e.g. foreach ([$a, $b] as $mock)
            if is_escaping_to_alias(Node::Block(block), variable.name) {
                continue;
            }
            out.push(Report::fixed(
                node_span(statement),
                "Use createStub() for a mock that expects no calls.",
                Safety::Safe,
                vec![Edit::new(call.name_span(), "createStub")],
            ));
        }
    }
}

/// rector-phpunit's `MockObjectExprDetector::isEscapingToAlias()`: the variable is put in an array, assigned to
/// another variable or captured by a closure, anywhere in the method (closures included).
fn is_escaping_to_alias(body: Node<'_, '_>, name: &[u8]) -> bool {
    let named = |e: &Expression<'_>| matches!(unparenthesize(e), Expression::Variable(Variable::Direct(v)) if v.name == name);
    let in_array = |e: &Expression<'_>| {
        let elements = match unparenthesize(e) {
            Expression::Array(a) => &a.elements,
            Expression::LegacyArray(a) => &a.elements,
            _ => return false,
        };
        elements.iter().any(|el| match el {
            ArrayElement::Value(v) => named(v.value),
            ArrayElement::KeyValue(kv) => named(kv.value),
            _ => false,
        })
    };
    any_node(body, &|n| match n {
        Node::Foreach(f) => in_array(f.expression),
        Node::Assignment(a) if matches!(a.operator, AssignmentOperator::Assign(_)) => {
            let rhs = match unparenthesize(a.rhs) {
                Expression::UnaryPrefix(u)
                    if matches!(u.operator, UnaryPrefixOperator::Reference(_)) =>
                {
                    u.operand
                }
                rhs => rhs,
            };
            named(rhs) || in_array(rhs)
        }
        Node::Closure(c) => c
            .use_clause
            .as_ref()
            .is_some_and(|u| u.variables.iter().any(|v| v.variable.name == name)),
        _ => false,
    })
}

pub(super) fn is_test_class_method(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    method: &Method<'_>,
) -> bool {
    is_public(method)
        && (method.name.value.starts_with(b"test")
            || has_attribute(
                ancestors,
                &method.attribute_lists,
                "PHPUnit\\Framework\\Attributes\\Test",
            )
            || Doc::of(ctx, ancestors, node_span(method).start).is_some_and(|doc| {
                named_tags(ctx, ancestors, &doc, "test").is_some_and(|t| !t.is_empty())
            }))
}

pub(super) fn is_used_for_mocking(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    roots: &[Node<'_, '_>],
    name: &[u8],
    documented: bool,
) -> bool {
    let is_name = |expr: &Expression<'_>| matches!(unparenthesize(expr), Expression::Variable(Variable::Direct(v)) if v.name[1..].eq_ignore_ascii_case(name));
    let mut related_variables = 0;
    let mut method_calls = Vec::new();
    scoped(roots, &mut |node| match node {
        Node::DirectVariable(v) if v.name[1..].eq_ignore_ascii_case(name) => related_variables += 1,
        Node::MethodCall(call) => method_calls.push(call),
        _ => {}
    });
    if related_variables == 1 {
        return false;
    }
    for call in method_calls {
        if !matches!(unparenthesize(call.object), Expression::Variable(_)) {
            continue;
        }
        if is_name(call.object) {
            return true;
        }
        for (index, arg) in call.argument_list.arguments.iter().enumerate() {
            let value = arg.value();
            if !is_name(value) {
                continue;
            }
            if passes_mock_object(ctx, ancestors, call, index, arg, documented) {
                return true;
            }
        }
    }
    false
}

fn passes_mock_object(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    call: &mago_syntax::cst::MethodCall<'_>,
    index: usize,
    arg: &Argument<'_>,
    documented: bool,
) -> bool {
    let class = match unparenthesize(call.object) {
        Expression::Variable(Variable::Direct(v)) if v.name == b"$this" => own_class(ancestors),
        other => object_class(ctx, other),
    };
    let (Some(class), mago_syntax::cst::ClassLikeMemberSelector::Identifier(method)) =
        (class, &call.method)
    else {
        return false;
    };
    let facts = facts(ctx);
    let codebase = if documented {
        facts.documented()
    } else {
        &facts.codebase
    };
    let Some(metadata) = codebase.get_declaring_method(class.as_bytes(), method.value) else {
        return false;
    };
    let mock_object =
        |parameter: &mago_codex::metadata::parameter::FunctionLikeParameterMetadata| {
            if documented {
                return parameter
                    .type_metadata
                    .as_ref()
                    .is_some_and(|t| is_super_type_of_mock_object(codebase, &t.type_union));
            }
            parameter
                .type_declaration_metadata
                .as_ref()
                .is_none_or(|t| is_super_type_of_mock_object(codebase, &t.type_union))
        };
    if let Argument::Named(named) = arg
        && metadata
            .parameters
            .iter()
            .any(|p| p.name.0.as_bytes()[1..] == *named.name.value && mock_object(p))
    {
        return true;
    }
    metadata.parameters.get(index).is_some_and(mock_object)
}

pub(super) fn is_super_type_of_mock_object(codebase: &CodebaseMetadata, t: &TUnion) -> bool {
    !t.types.is_empty() && t.types.iter().all(|a| is_mock_object(codebase, a))
}

pub(super) fn is_mock_object(codebase: &CodebaseMetadata, atomic: &TAtomic) -> bool {
    let TAtomic::Object(TObject::Named(named)) = atomic else {
        return false;
    };
    let is_mock = |c: &[u8]| {
        c.eq_ignore_ascii_case(MOCK_OBJECT.as_bytes())
            || codebase.is_instance_of(c, MOCK_OBJECT.as_bytes())
    };
    is_mock(named.name.as_bytes())
        || named
            .intersection_types
            .iter()
            .flat_map(|i| i.iter())
            .any(|a| matches!(a, TAtomic::Object(TObject::Named(n)) if is_mock(n.name.as_bytes())))
}
