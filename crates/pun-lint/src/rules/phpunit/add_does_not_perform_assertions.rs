//! rector-phpunit's `AddDoesNotPerformAssertionToNonAssertingTestRector` (PHPUnit 6.0), with its
//! `AssertCallAnalyzer` and `MockedVariableAnalyzer`: a test that asserts nothing gets
//! `#[DoesNotPerformAssertions]` (`@doesNotPerformAssertions` where the attribute class does not exist), and its
//! `$this->addToAssertionCount()` statements go.

use crate::problem::{Edit, Safety, Span};
use mago_codex::identifier::method::MethodIdentifier;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{
    Call, ClassLikeMember, ClassLikeMemberSelector, Expression, Method, MethodBody, Modifier, Node,
    NodeKind, Statement, Variable,
};
use mago_word::word;

use super::annotations::Annotated;
use super::prefer_create_stub_expression::is_test_class_method;
use super::{AnyCall, class_is, is_in_test_class, object_class, own_class, resolve};
use crate::rules::support::class_members::walk;
use crate::rules::support::docblocks::Doc;
use crate::rules::support::names::{has_attribute, resolve_function};
use crate::rules::support::rewrites::merge;
use crate::rules::support::side_effects::any_node;
use crate::rules::support::statement_edits::{block_list, line_indent, remove};
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const DOES_NOT_PERFORM_ASSERTIONS: &str =
    "PHPUnit\\Framework\\Attributes\\DoesNotPerformAssertions";
const TWIG_INTEGRATION_TEST_CASE: &str = "Twig\\Test\\IntegrationTestCase";
const MOCK_OBJECT: &str = "PHPUnit\\Framework\\MockObject\\MockObject";
const MAX_NESTED_METHOD_CALL_LEVEL: usize = 5;
const PHPUNIT_FUNCTION_NAMESPACE: &str = "PHPUnit\\Framework\\";
const ASSERT_METHOD_NAME_PREFIXES: &[&str] = &[
    "expectNotToPerformAssertions",
    "assert",
    "expectException",
    "setExpectedException",
    "expectOutput",
    "expectUserDeprecationMessage",
    "should",
];
/// `PHPUnitClassName::FRAMEWORK_TEST_CASES`: their methods are never parsed, their hooks do not count as test
/// assertions.
const FRAMEWORK_TEST_CASES: &[&str] = &[
    "PHPUnit\\Framework\\TestCase",
    "PHPUnit\\Framework\\Assert",
    "Symfony\\Bundle\\FrameworkBundle\\Test\\KernelTestCase",
    "Symfony\\Bundle\\FrameworkBundle\\Test\\WebTestCase",
    "Symfony\\Component\\Form\\Test\\TypeTestCase",
];
const HOOK_METHOD_NAMES: &[&str] = &[
    "setUp",
    "tearDown",
    "assertPreConditions",
    "assertPostConditions",
];
const HOOK_ATTRIBUTES: &[&str] = &[
    "PHPUnit\\Framework\\Attributes\\Before",
    "PHPUnit\\Framework\\Attributes\\After",
    "PHPUnit\\Framework\\Attributes\\PreCondition",
    "PHPUnit\\Framework\\Attributes\\PostCondition",
];
const HOOK_ANNOTATIONS: &[&str] = &["@before", "@after", "@preCondition", "@postCondition"];
/// Every parent class is parsed, so stop walking deep chains.
const MAX_PARENT_CLASS_LEVEL: usize = 5;

pub struct AddDoesNotPerformAssertions;

impl Rule for AddDoesNotPerformAssertions {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-does-not-perform-assertions"
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
        let Some(at) = ancestors.iter().rposition(|a| {
            matches!(
                a,
                Node::Class(_)
                    | Node::AnonymousClass(_)
                    | Node::Interface(_)
                    | Node::Trait(_)
                    | Node::Enum(_)
            )
        }) else {
            return;
        };
        // the rule visits classes: a test method of a trait is left alone, and an abstract class's child can add
        // asserting lifecycle methods that count toward the inherited tests
        if matches!(ancestors[at], Node::Trait(_))
            || matches!(ancestors[at], Node::Class(c) if c.modifiers.iter().any(|m| matches!(m, Modifier::Abstract(_))))
            || !matches!(method.body, MethodBody::Concrete(_))
            || !is_in_test_class(ctx, ancestors, Some(method))
            || !is_test_class_method(ctx, ancestors, method)
            // the parent test case asserts in its own integration methods
            || class_is(ctx, &ancestors[..=at], &[TWIG_INTEGRATION_TEST_CASE])
        {
            return;
        }
        let start = node_span(method).start;
        let doc = Doc::of(ctx, ancestors, start);
        let tagged = doc.as_ref().is_some_and(|doc| {
            (0..doc.len()).any(|i| {
                matches!(
                    doc.words(i).first(),
                    Some(&("@doesNotPerformAssertions" | "@expectedException"))
                )
            })
        });
        if tagged
            || has_attribute(
                ancestors,
                &method.attribute_lists,
                DOES_NOT_PERFORM_ASSERTIONS,
            )
        {
            return;
        }
        let mut chain = ancestors.to_vec();
        chain.push(node);
        if contains_assert_call(ctx, &chain, method, true, 1)
            || contains_mock(ctx, &chain, node)
            // assertions in lifecycle methods count toward every test of the class
            || has_asserting_lifecycle_method(ctx, &ancestors[..=at])
        {
            return;
        }
        let source = ctx.source;
        let mut edits = remove_add_to_assertion_count_calls(ctx, node);
        // the attribute is available since PHPUnit 10, prefer it over the annotation
        if facts(ctx)
            .codebase
            .get_class_like(DOES_NOT_PERFORM_ASSERTIONS.as_bytes())
            .is_some()
        {
            let Some(added) = Annotated::of(ctx, node, ancestors).and_then(|a| {
                a.edits(
                    source,
                    None,
                    &[format!("#[\\{DOES_NOT_PERFORM_ASSERTIONS}]")],
                )
            }) else {
                return;
            };
            edits.extend(added);
        } else {
            let tag = "@doesNotPerformAssertions";
            let indent = line_indent(source, start);
            edits.push(match &doc {
                Some(doc) => {
                    let printed = doc.print_appending(&vec![false; doc.len()], &[tag]);
                    // a one-line doc opens up at column 0
                    let printed = match doc.text_all().contains('\n') {
                        true => printed,
                        false => printed.replace('\n', &format!("\n{indent}")),
                    };
                    Edit::new(doc.span, printed)
                }
                None => Edit::new(
                    Span::new(start, start),
                    format!("/**\n{indent} * {tag}\n{indent} */\n{indent}"),
                ),
            });
        }
        out.push(Report::fixed(
            node_span(&method.name),
            "Mark a test that performs no assertions with `#[DoesNotPerformAssertions]`.",
            Safety::Unsafe,
            merge(edits),
        ));
    }
}

fn call_name<'a>(selector: &'a ClassLikeMemberSelector<'a>) -> Option<&'a [u8]> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

fn is_assert_name(name: &[u8]) -> bool {
    ASSERT_METHOD_NAME_PREFIXES
        .iter()
        .any(|p| name.starts_with(p.as_bytes()))
}

/// `AssertCallAnalyzer::containsAssertCall()`; `chain` ends at `method`. Only the test method itself (`top`) has
/// Mago's types: in the methods it calls, Pun follows calls on `$this`, `self`, `static` and named classes. Rector
/// resolves no class for `parent::`, so neither does Pun.
fn contains_assert_call(
    ctx: &Context<'_>,
    chain: &[Node<'_, '_>],
    method: &Method<'_>,
    top: bool,
    level: usize,
) -> bool {
    // probably no assert method in the end
    if level > MAX_NESTED_METHOD_CALL_LEVEL {
        return false;
    }
    let MethodBody::Concrete(block) = &method.body else {
        return false;
    };
    let root = Node::Block(block);
    // A. a direct assert or mock call
    let mut found = false;
    walk(root, &mut |n, path| {
        if found {
            return false;
        }
        found = match n {
            Node::MethodCall(call) => {
                let name = call_name(&call.method);
                name.is_some_and(|n| n.eq_ignore_ascii_case(b"expects"))
                    || (top
                        && object_class(ctx, call.object).is_some_and(|c| {
                            c == "PHPUnit\\Framework\\MockObject\\MockBuilder"
                                || c == "Prophecy\\Prophet"
                        }))
                    || name.is_some_and(is_assert_name)
            }
            Node::StaticMethodCall(call) => call_name(&call.method).is_some_and(is_assert_name),
            // standalone function assert, e.g. "use function PHPUnit\Framework\assertNotNull;"
            Node::FunctionCall(call) => match unparenthesize(call.function) {
                Expression::Identifier(id) => {
                    let mut scope = chain.to_vec();
                    scope.extend_from_slice(path);
                    let (name, _) = resolve_function(&scope, id);
                    name.strip_prefix(PHPUNIT_FUNCTION_NAMESPACE.as_bytes())
                        .is_some_and(is_assert_name)
                }
                _ => false,
            },
            _ => false,
        };
        !found
    });
    if found {
        return true;
    }
    // B. nested calls
    let mut calls = Vec::new();
    walk(root, &mut |n, path| {
        if let Some(call) = AnyCall::of(n) {
            let mut scope = chain.to_vec();
            scope.extend_from_slice(path);
            calls.push((call, scope));
        }
        true
    });
    calls.into_iter().any(|(call, scope)| {
        // is a mock call
        if call.is_name(&["expects"]) {
            return true;
        }
        let (Some(class), Some(name)) = (called_class(ctx, &scope, &call, top), call.name()) else {
            return false;
        };
        // e.g. "parent::setUp()" declared in PHPUnit or Symfony, no need to parse their huge classes
        if declared_in_framework_test_case(ctx, &class, name) {
            return false;
        }
        with_method(ctx, chain, &class, name, &mut |called_chain, called| {
            // skip circular self calls
            !std::ptr::addr_eq(called, method)
                && contains_assert_call(ctx, called_chain, called, false, level + 1)
        })
    })
}

fn is_framework_test_case(class: &str) -> bool {
    let class = class.trim_start_matches('\\');
    FRAMEWORK_TEST_CASES
        .iter()
        .any(|c| c.eq_ignore_ascii_case(class))
}

/// `AssertCallAnalyzer::isDeclaredInFrameworkTestCase()`.
fn declared_in_framework_test_case(ctx: &Context<'_>, class: &str, name: &[u8]) -> bool {
    let codebase = facts(ctx);
    let codebase = codebase.documented();
    let id = MethodIdentifier::new(word(class.as_bytes()), word(name));
    codebase
        .get_method_by_id(&codebase.get_declaring_method_identifier(&id))
        .is_some()
        && is_framework_test_case(
            &codebase
                .get_declaring_method_identifier(&id)
                .get_class_name()
                .to_string(),
        )
}

/// rector-phpunit's `LifecycleMethodAssertAnalyzer::hasAssertingLifecycleMethod()`: a hook of the class or of a
/// parent class (up to 5 levels, below PHPUnit's and Symfony's test cases) asserts. `chain` ends at the class.
fn has_asserting_lifecycle_method(ctx: &Context<'_>, chain: &[Node<'_, '_>]) -> bool {
    let members = match chain.last() {
        Some(Node::Class(c)) => &c.members,
        Some(Node::AnonymousClass(c)) => &c.members,
        _ => return false,
    };
    let mut overridden = Vec::new();
    if hooks_assert(ctx, chain, members, ctx.source, true, &mut overridden) {
        return true;
    }
    let facts = facts(ctx);
    let codebase = facts.documented();
    let Some(mut class) = super::own_class_reflection(ctx, chain) else {
        return false;
    };
    for _ in 0..MAX_PARENT_CLASS_LEVEL {
        let Some(parent) = codebase
            .get_class_like(class.as_bytes())
            .and_then(|c| c.direct_parent_class)
            .map(|p| p.to_string())
        else {
            return false;
        };
        if is_framework_test_case(&parent) {
            return false;
        }
        if with_class(ctx, chain, &parent, &mut |parent_chain, members, source| {
            hooks_assert(ctx, parent_chain, members, source, false, &mut overridden)
        }) {
            return true;
        }
        class = parent;
    }
    false
}

/// `isClassLikeAsserting()`'s loop over the methods of one class.
fn hooks_assert(
    ctx: &Context<'_>,
    chain: &[Node<'_, '_>],
    members: &mago_syntax::cst::Sequence<'_, ClassLikeMember<'_>>,
    source: &str,
    top: bool,
    overridden: &mut Vec<&'static str>,
) -> bool {
    for member in members {
        let ClassLikeMember::Method(method) = member else {
            continue;
        };
        let hook = HOOK_METHOD_NAMES
            .iter()
            .copied()
            .find(|h| method.name.value.eq_ignore_ascii_case(h.as_bytes()));
        // methods marked as hooks run too, also when overridden
        let relevant = hook.is_some_and(|h| !overridden.contains(&h))
            || HOOK_ATTRIBUTES
                .iter()
                .any(|a| has_attribute(chain, &method.attribute_lists, a))
            || hook_annotation(source, node_span(method).start);
        if relevant {
            let mut method_chain = chain.to_vec();
            method_chain.push(Node::Method(method));
            if contains_assert_call(ctx, &method_chain, method, top, 1) {
                return true;
            }
        }
        // a hook overridden here hides the parent one, unless it calls it through "parent::"
        if let Some(h) = hook
            && !any_node(Node::Method(method), &|n| {
                matches!(n, Node::StaticMethodCall(c)
                    if matches!(c.class, Expression::Parent(_))
                        && call_name(&c.method).is_some_and(|m| m.eq_ignore_ascii_case(h.as_bytes())))
            })
        {
            overridden.push(h);
        }
    }
    false
}

/// The docblock right before `start` has `@before`, `@after`, `@preCondition` or `@postCondition`.
fn hook_annotation(source: &str, start: usize) -> bool {
    let before = source[..start].trim_end();
    let Some(open) = before.strip_suffix("*/").and_then(|b| b.rfind("/**")) else {
        return false;
    };
    let doc = &before[open..];
    HOOK_ANNOTATIONS.iter().any(|tag| {
        doc.match_indices(tag).any(|(i, _)| {
            !doc[i + tag.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
    })
}

/// Runs `f` on the class `class` names, with the nodes from its file's root down to it and that file's source.
fn with_class(
    ctx: &Context<'_>,
    chain: &[Node<'_, '_>],
    class: &str,
    f: &mut dyn FnMut(
        &[Node<'_, '_>],
        &mago_syntax::cst::Sequence<'_, ClassLikeMember<'_>>,
        &str,
    ) -> bool,
) -> bool {
    let facts = facts(ctx);
    let Some(name_span) = facts
        .documented()
        .get_class_like(class.as_bytes())
        .and_then(|c| c.name_span)
    else {
        return false;
    };
    let at = name_span.start.offset as usize;
    let find = |program: Node<'_, '_>,
                source: &str,
                f: &mut dyn FnMut(
        &[Node<'_, '_>],
        &mago_syntax::cst::Sequence<'_, ClassLikeMember<'_>>,
        &str,
    ) -> bool| {
        let mut found = None;
        walk(program, &mut |n, path| {
            if found.is_some() {
                return false;
            }
            if let Node::Class(c) = n {
                if node_span(&c.name).start == at {
                    let mut chain = path.to_vec();
                    chain.push(n);
                    found = Some((chain, &c.members));
                }
                return false;
            }
            true
        });
        found.is_some_and(|(chain, members)| f(&chain, members, source))
    };
    if facts.text(name_span).is_some() {
        return chain
            .first()
            .is_some_and(|program| find(*program, ctx.source, f));
    }
    let Some(source) = facts.source_of(name_span.file_id) else {
        return false;
    };
    let arena = mago_allocator::LocalArena::new();
    let file = mago_database::file::File::ephemeral(
        std::borrow::Cow::Borrowed(b"parent.php"),
        std::borrow::Cow::Owned(source.as_bytes().to_vec()),
    );
    let program = mago_syntax::parser::parse_file(&arena, &file);
    find(Node::Program(program), &source, f)
}

fn called_class(
    ctx: &Context<'_>,
    scope: &[Node<'_, '_>],
    call: &AnyCall<'_>,
    top: bool,
) -> Option<String> {
    let receiver = unparenthesize(call.receiver);
    match receiver {
        Expression::Variable(Variable::Direct(v)) if v.name == b"$this" => own_class(scope),
        Expression::Self_(_) | Expression::Static(_) if call.is_static => own_class(scope),
        Expression::Identifier(id) if call.is_static => Some(resolve(scope, id)),
        // "parent" is the direct parent class of the class the call is in
        Expression::Parent(_) if call.is_static => facts(ctx)
            .documented()
            .get_class_like(own_class(scope)?.as_bytes())?
            .direct_parent_class
            .map(|p| p.to_string()),
        _ if top => object_class(ctx, receiver),
        _ => None,
    }
}

/// Runs `f` on the method `class::name` resolves to, with the nodes from its file's root down to it.
fn with_method(
    ctx: &Context<'_>,
    chain: &[Node<'_, '_>],
    class: &str,
    name: &[u8],
    f: &mut dyn FnMut(&[Node<'_, '_>], &Method<'_>) -> bool,
) -> bool {
    let facts = facts(ctx);
    let Some(name_span) = facts
        .documented()
        .get_declaring_method(class.as_bytes(), name)
        .and_then(|m| m.name_span)
    else {
        return false;
    };
    let at = name_span.start.offset as usize;
    if facts.text(name_span).is_some() {
        return chain
            .first()
            .is_some_and(|program| find_method(*program, at, name, f));
    }
    let Some(source) = facts.source_of(name_span.file_id) else {
        return false;
    };
    let arena = mago_allocator::LocalArena::new();
    let file = mago_database::file::File::ephemeral(
        std::borrow::Cow::Borrowed(b"called.php"),
        std::borrow::Cow::Owned(source.as_bytes().to_vec()),
    );
    let program = mago_syntax::parser::parse_file(&arena, &file);
    find_method(Node::Program(program), at, name, f)
}

fn find_method(
    program: Node<'_, '_>,
    at: usize,
    name: &[u8],
    f: &mut dyn FnMut(&[Node<'_, '_>], &Method<'_>) -> bool,
) -> bool {
    let mut found = None;
    walk(program, &mut |n, path| {
        if found.is_some() {
            return false;
        }
        if let Node::Method(m) = n {
            if node_span(&m.name).start == at && m.name.value.eq_ignore_ascii_case(name) {
                let mut chain = path.to_vec();
                chain.push(n);
                found = Some((chain, m));
            }
            return false;
        }
        true
    });
    found.is_some_and(|(chain, m)| f(&chain, m))
}

/// `MockedVariableAnalyzer::containsMockAsUsedVariable()`.
fn contains_mock(ctx: &Context<'_>, chain: &[Node<'_, '_>], node: Node<'_, '_>) -> bool {
    let facts = facts(ctx);
    let codebase = facts.documented();
    let is_super_of_mock = |name: &[u8]| {
        name.eq_ignore_ascii_case(MOCK_OBJECT.as_bytes())
            || codebase.is_instance_of(MOCK_OBJECT.as_bytes(), name)
    };
    let base = &chain[..chain.len() - 1];
    let mut found = false;
    walk(node, &mut |n, path| {
        if found {
            return false;
        }
        let span = match n {
            // is mockery mock
            Node::StaticMethodCall(call) => {
                if let Expression::Identifier(id) = unparenthesize(call.class) {
                    let mut scope = base.to_vec();
                    scope.extend_from_slice(path);
                    found = resolve(&scope, id).eq_ignore_ascii_case("Mockery")
                        && call_name(&call.method).is_some_and(|n| n.eq_ignore_ascii_case(b"mock"));
                }
                return !found;
            }
            Node::DirectVariable(v) => node_span(v),
            Node::PropertyAccess(a) => node_span(a),
            _ => return true,
        };
        let Some(t) = facts.full_type(span).filter(|t| !t.is_mixed()) else {
            return true;
        };
        found = t.types.iter().any(|a| match a {
            TAtomic::Object(TObject::Any) => true,
            TAtomic::Object(TObject::Named(named)) => {
                is_super_of_mock(named.name.as_bytes())
                    || named.intersection_types.iter().flat_map(|i| i.iter()).any(
                        |i| matches!(i, TAtomic::Object(TObject::Named(n)) if is_super_of_mock(n.name.as_bytes())),
                    )
            }
            _ => false,
        });
        !found
    });
    found
}

/// The `$this->addToAssertionCount()` statements, with a comment behind the last statement of their block (php-parser's
/// `Nop` right behind the removed call).
fn remove_add_to_assertion_count_calls(ctx: &Context<'_>, node: Node<'_, '_>) -> Vec<Edit> {
    let source = ctx.source;
    let mut edits = Vec::new();
    walk(node, &mut |n, _| {
        let Node::Block(block) = n else { return true };
        let list = block_list(source, block);
        let indices: Vec<usize> = (list.stmts.iter().enumerate())
            .filter(|(_, s)| is_add_to_assertion_count(s))
            .map(|(i, _)| i)
            .collect();
        let Some(&last) = indices.last() else {
            return true;
        };
        match list.nop_span(source) {
            Some(_) if indices.len() == list.stmts.len() => {
                let indent = line_indent(source, block.left_brace.start.offset as usize);
                edits.push(Edit::new(list.inner, format!("\n{indent}")));
            }
            Some(nop) if last == list.stmts.len() - 1 => {
                edits.extend(remove(ctx, &list, &indices[..indices.len() - 1]));
                edits.push(Edit::new(Span::new(list.before(ctx, last), nop.end), ""));
            }
            _ => edits.extend(remove(ctx, &list, &indices)),
        }
        true
    });
    edits
}

fn is_add_to_assertion_count(statement: &Statement<'_>) -> bool {
    let Statement::Expression(e) = statement else {
        return false;
    };
    let Expression::Call(Call::Method(call)) = e.expression else {
        return false;
    };
    matches!(unparenthesize(call.object), Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
        && call_name(&call.method).is_some_and(|n| n.eq_ignore_ascii_case(b"addToAssertionCount"))
}
