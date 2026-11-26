//! rector-phpunit's `WithConsecutiveRector` (PHPUnit 10.0), with its `WillReturnCallbackFactory`,
//! `ConsecutiveIfsFactory`, `NestedClosureAssertFactory`, `ExpectsMethodCallDecorator`, `WillReturnIfNodeDecorator`
//! and `UsedVariablesResolver`: `withConsecutive()` becomes a `willReturnCallback()` closure that asserts each call's
//! parameters by `$matcher`'s invocation count.
//!
//! Rector reprints the nodes it builds with php-parser's printer and keeps the rest as written; Pun writes the same
//! text. A `withConsecutive()` with one argument keeps its place and the closure takes the argument's indentation;
//! with any other count php-parser reprints the call after its receiver, the closure at the statement's indentation.

use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{
    Argument, ArrayElement, BinaryOperator, Call, ClassLikeMemberSelector, Construct, Expression,
    MethodCall, Node, NodeKind, UnaryPrefixOperator,
};

use super::is_in_test_class;
use crate::rules::support::class_members::walk;
use crate::rules::support::statement_edits::{line_indent, previous_token_end, reindent};
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const INVOCATION_ORDER: &str = "PHPUnit\\Framework\\MockObject\\Rule\\InvocationOrder";

/// Text that is either written here or copied from the source, reindented from the line it starts on.
enum Piece {
    Lit(String),
    Orig(Span, usize),
}

type Stmt = Vec<Piece>;

enum Block {
    If(usize, Vec<Stmt>),
    Stmt(Stmt),
}

fn lit(s: impl Into<String>) -> Piece {
    Piece::Lit(s.into())
}

fn orig(expr: &impl mago_span::HasSpan, from: usize) -> Piece {
    Piece::Orig(node_span(expr), from)
}

fn render(source: &str, stmt: &[Piece], indent: &str) -> String {
    stmt.iter()
        .map(|p| match p {
            Piece::Lit(s) => s.clone(),
            Piece::Orig(span, from) => reindent(source, *span, line_indent(source, *from), indent),
        })
        .collect()
}

fn render_blocks(source: &str, blocks: &[Block], invocation: &str, indent: &str) -> Vec<String> {
    let inner = format!("{indent}    ");
    blocks
        .iter()
        .map(|b| match b {
            Block::Stmt(s) => render(source, s, indent),
            Block::If(count, body) => {
                let mut text = format!("if ($matcher->{invocation}() === {count}) {{");
                for s in body {
                    text.push('\n');
                    text.push_str(&inner);
                    text.push_str(&render(source, s, &inner));
                }
                text.push('\n');
                text.push_str(indent);
                text.push('}');
                text
            }
        })
        .collect()
}

fn name_of<'a>(call: &'a MethodCall<'a>) -> Option<&'a [u8]> {
    match &call.method {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

fn is_named(call: &MethodCall<'_>, name: &str) -> bool {
    name_of(call).is_some_and(|n| n.eq_ignore_ascii_case(name.as_bytes()))
}

fn method_call<'a>(expr: &'a Expression<'a>) -> Option<&'a MethodCall<'a>> {
    match expr {
        Expression::Call(Call::Method(m)) => Some(m),
        _ => None,
    }
}

fn first_arg<'a>(call: &'a MethodCall<'a>) -> Option<&'a Expression<'a>> {
    call.argument_list.arguments.first().map(Argument::value)
}

fn assert_same(compared: Piece, key: usize) -> Stmt {
    vec![
        lit("$this->assertSame("),
        compared,
        lit(format!(", $parameters[{key}]);")),
    ]
}

/// `NestedClosureAssertFactory::create()`.
fn nested_closure_assert(call: &MethodCall<'_>, key: usize) -> Option<Vec<Stmt>> {
    let first = first_arg(call)?;
    if let Expression::ArrowFunction(arrow) = unparenthesize(first) {
        let expr = unparenthesize(arrow.expression);
        if let Expression::Binary(b) = expr
            && matches!(b.operator, BinaryOperator::Identical(_))
        {
            // unwrap closure arrow function to direct assert as more readable
            if matches!(unparenthesize(b.lhs), Expression::Variable(_)) {
                return Some(vec![assert_same(orig(b.rhs, node_span(b.rhs).start), key)]);
            }
            if matches!(unparenthesize(b.rhs), Expression::Variable(_)) {
                return Some(vec![assert_same(orig(b.lhs, node_span(b.lhs).start), key)]);
            }
        }
        let empty = |e: &Expression<'_>| {
            matches!(
                unparenthesize(e),
                Expression::Construct(Construct::Empty(_))
            )
        };
        if let Expression::UnaryPrefix(u) = expr
            && matches!(u.operator, UnaryPrefixOperator::Not(_))
            && empty(u.operand)
        {
            return Some(vec![vec![lit(format!(
                "$this->assertNotEmpty($parameters[{key}]);"
            ))]]);
        }
        if empty(expr) {
            return Some(vec![vec![lit(format!(
                "$this->assertEmpty($parameters[{key}]);"
            ))]]);
        }
    }
    Some(vec![
        vec![
            lit("$callback = "),
            orig(first, node_span(first).start),
            lit(";"),
        ],
        vec![lit(format!(
            "$this->assertTrue($callback($parameters[{key}]));"
        ))],
    ])
}

/// `WillReturnCallbackFactory::createParametersMatch()` with `ConsecutiveIfsFactory::createIfs()`; `None` where Rector
/// fails on the file.
fn parameters_match(call: &MethodCall<'_>, invocation: &str) -> Option<Vec<Block>> {
    let args: Vec<&Argument<'_>> = call.argument_list.arguments.iter().collect();
    if let Some(Argument::Positional(first)) = args.first()
        && first.ellipsis.is_some()
    {
        let value = unparenthesize(first.value);
        if !matches!(
            value,
            Expression::Variable(_)
                | Expression::Access(_)
                | Expression::Call(_)
                | Expression::ArrayAccess(_)
        ) {
            return None;
        }
        return Some(vec![Block::Stmt(vec![
            lit("$this->assertSame("),
            orig(first.value, node_span(first.value).start),
            lit(format!("[$matcher->{invocation}() - 1], $parameters);")),
        ])]);
    }
    let mut ifs = Vec::new();
    for (key, arg) in args.iter().enumerate() {
        let mut body: Vec<Stmt> = Vec::new();
        let value = arg.value();
        let elements = match value {
            Expression::Array(a) => Some(&a.elements),
            Expression::LegacyArray(a) => Some(&a.elements),
            _ => None,
        };
        if let Some(elements) = elements {
            for (assert_key, element) in elements.iter().enumerate() {
                let item = match element {
                    ArrayElement::Value(v) => v.value,
                    ArrayElement::KeyValue(kv) => kv.value,
                    ArrayElement::Variadic(v) => v.value,
                    ArrayElement::Missing(_) => return None,
                };
                let from = node_span(item).start;
                let Some(assert) = method_call(item) else {
                    body.push(assert_same(orig(item, from), assert_key));
                    continue;
                };
                if is_named(assert, "equalTo") {
                    let name = node_span(&assert.method);
                    let list = &assert.argument_list;
                    let at = list
                        .arguments
                        .last()
                        .map_or(list.left_parenthesis.end.offset as usize, |a| {
                            node_span(a).end
                        });
                    let comma = if list.arguments.is_empty() { "" } else { ", " };
                    body.push(vec![
                        Piece::Orig(Span::new(from, name.start), from),
                        lit("assertEquals"),
                        Piece::Orig(Span::new(name.end, at), from),
                        lit(format!("{comma}$parameters[{assert_key}]")),
                        Piece::Orig(Span::new(at, node_span(assert).end), from),
                        lit(";"),
                    ]);
                } else if is_named(assert, "callback") {
                    body.extend(nested_closure_assert(assert, assert_key)?);
                } else {
                    body.push(assert_same(orig(item, from), assert_key));
                }
            }
        } else if let Some(m) = method_call(value) {
            // special callable case
            if is_named(m, "callback")
                && let Some(Expression::ArrowFunction(arrow)) = first_arg(m).map(unparenthesize)
                && let Expression::Binary(b) = unparenthesize(arrow.expression)
                && matches!(b.operator, BinaryOperator::Identical(_))
                && matches!(unparenthesize(b.lhs), Expression::Variable(_))
            {
                return Some(vec![Block::Stmt(assert_same(
                    orig(b.rhs, node_span(b.rhs).start),
                    0,
                ))]);
            }
            return None;
        }
        ifs.push(Block::If(key + 1, body));
    }
    Some(ifs)
}

/// The variables a set of nodes reads, as `UsedVariablesResolver` finds them: not inside closures, functions or
/// classes, without `$this`, `$matcher` and `$parameters`, each name once.
fn used_variables(roots: &[Node<'_, '_>], out: &mut Vec<String>) -> Option<()> {
    let mut ok = true;
    for root in roots {
        walk(*root, &mut |n, _| match n {
            Node::Closure(_)
            | Node::Function(_)
            | Node::AnonymousClass(_)
            | Node::Class(_)
            | Node::StaticPropertyAccess(_) => false,
            Node::DirectVariable(v) => {
                let name = String::from_utf8_lossy(&v.name[1..]).into_owned();
                if !matches!(name.as_str(), "this" | "matcher" | "parameters")
                    && !out.contains(&name)
                {
                    out.push(name);
                }
                true
            }
            Node::IndirectVariable(_) | Node::NestedVariable(_) => {
                ok = false;
                false
            }
            _ => true,
        });
    }
    ok.then_some(())
}

enum Return<'a> {
    Value(&'a Expression<'a>),
    Argument(&'a Expression<'a>),
    Throw(&'a Expression<'a>),
}

pub struct NoWithConsecutive;

impl Rule for NoWithConsecutive {
    fn id(&self) -> &'static str {
        "lint/phpunit/no-with-consecutive"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ExpressionStatement])
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one port of WithConsecutiveRector::refactor()"
    )]
    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (Node::ExpressionStatement(statement), [.., Node::Block(block), Node::Statement(stmt)]) =
            (node, ancestors)
        else {
            return;
        };
        let Some(top) = method_call(statement.expression) else {
            return;
        };
        if !is_in_test_class(ctx, ancestors, None) {
            return;
        }
        let source = ctx.source;
        // every method call of the statement, in php-parser's order, and the chain the statement is
        let mut calls: Vec<&MethodCall<'_>> = Vec::new();
        walk(Node::MethodCall(top), &mut |n, _| {
            if let Node::MethodCall(m) = n {
                calls.push(m);
            }
            true
        });
        let mut chain: Vec<&MethodCall<'_>> = Vec::new();
        let mut at: &Expression<'_> = statement.expression;
        while let Some(m) = method_call(unparenthesize(at)) {
            chain.push(m);
            at = m.object;
        }
        let on_chain = |m: &MethodCall<'_>| chain.iter().any(|c| std::ptr::eq(*c, m));
        let find = |name: &str| calls.iter().copied().find(|m| is_named(m, name));
        let Some(with_consecutive) = find("withConsecutive") else {
            return;
        };
        if !on_chain(with_consecutive)
            || calls
                .iter()
                .any(|m| is_named(m, "willReturnMap") || is_named(m, "will"))
        {
            return;
        }
        let mut edits = Vec::new();
        let remove = |name: &str, edits: &mut Vec<Edit>| -> Option<Option<&MethodCall<'_>>> {
            let found = calls.iter().copied().filter(|m| is_named(m, name));
            for m in found.clone() {
                if !on_chain(m) {
                    return None;
                }
                edits.push(Edit::new(
                    Span::new(node_span(m.object).end, node_span(m).end),
                    "",
                ));
            }
            Some(found.into_iter().next())
        };
        let mut returned = None;
        let mut by_ref = false;
        let Some(will_return) = remove("willReturn", &mut edits) else {
            return;
        };
        if let Some(m) = will_return {
            let Some(arg) = first_arg(m) else { return };
            returned = Some(Return::Value(arg));
        }
        let Some(will_return_self) = remove("willReturnSelf", &mut edits) else {
            return;
        };
        if let Some(m) = will_return_self {
            let mut root: &Expression<'_> = m.object;
            while let Some(inner) = method_call(root) {
                root = inner.object;
            }
            returned = Some(Return::Value(root));
        }
        let Some(will_return_argument) = remove("willReturnArgument", &mut edits) else {
            return;
        };
        if let Some(m) = will_return_argument {
            let Some(arg) = first_arg(m) else { return };
            returned = Some(Return::Argument(arg));
        }
        let Some(on_consecutive_calls) = remove("willReturnOnConsecutiveCalls", &mut edits) else {
            return;
        };
        let Some(will_throw) = remove("willThrowException", &mut edits) else {
            return;
        };
        if let Some(m) = will_throw {
            let Some(arg) = first_arg(m) else { return };
            returned = Some(Return::Throw(arg));
        }
        let Some(will_return_reference) = remove("willReturnReference", &mut edits) else {
            return;
        };
        if let Some(m) = will_return_reference {
            let Some(arg) = first_arg(m) else { return };
            returned = Some(Return::Value(arg));
            // returns passed args
            by_ref = true;
        }
        // ExpectsMethodCallDecorator: `expects(...)` takes `$matcher`, which gets the matcher
        let mut expects_call = None;
        for m in calls.iter().filter(|m| is_named(m, "expects")) {
            let Some(arg) = first_arg(m) else { return };
            if matches!(
                arg,
                Expression::Call(Call::Method(_) | Call::StaticMethod(_))
            ) {
                expects_call = Some(arg);
                edits.push(Edit::new(node_span(arg), "$matcher"));
            }
        }
        if expects_call.is_none() {
            // add expects() method
            if let Some(m) = calls.iter().find(|m| method_call(m.object).is_none()) {
                let end = node_span(m.object).end;
                edits.push(Edit::new(Span::new(end, end), "->expects($matcher)"));
            }
        }
        let facts = facts(ctx);
        let codebase = facts.documented();
        let invocation = match codebase.get_class_like(INVOCATION_ORDER.as_bytes()) {
            // fallback to PHPUnit 9
            None => "getInvocationCount",
            Some(_)
                if codebase
                    .get_declaring_method(INVOCATION_ORDER.as_bytes(), b"getInvocationCount")
                    .is_some() =>
            {
                "getInvocationCount"
            }
            Some(_) => "numberOfInvocations",
        };
        let Some(mut blocks) = parameters_match(with_consecutive, invocation) else {
            return;
        };
        let consecutive_span = Span::new(
            node_span(with_consecutive.object).end,
            node_span(with_consecutive).end,
        );
        // 2. does willReturnCallback() exist? just merge them together
        if let Some(callback) = find("willReturnCallback") {
            let Some(Expression::Closure(closure)) = first_arg(callback) else {
                return;
            };
            if !on_chain(callback) {
                return;
            }
            let params = &closure.parameter_list;
            let at = params
                .parameters
                .last()
                .map_or(params.left_parenthesis.end.offset as usize, |p| {
                    node_span(p).end
                });
            let comma = if params.parameters.is_empty() {
                ""
            } else {
                ", "
            };
            edits.push(Edit::new(Span::new(at, at), format!("{comma}$parameters")));
            let body = &closure.body;
            let open = body.left_brace.end.offset as usize;
            let close = body.right_brace.start.offset as usize;
            let closure_indent = line_indent(source, node_span(closure).start);
            match body.statements.first() {
                Some(first) => {
                    let inner = line_indent(source, node_span(first).start);
                    let text: String = render_blocks(source, &blocks, invocation, inner)
                        .iter()
                        .map(|s| format!("\n{inner}{s}"))
                        .collect();
                    edits.push(Edit::new(Span::new(open, open), text));
                }
                None => {
                    let inner = format!("{closure_indent}    ");
                    let text: String = render_blocks(source, &blocks, invocation, &inner)
                        .iter()
                        .map(|s| format!("\n{inner}{s}"))
                        .collect();
                    edits.push(Edit::new(
                        Span::new(open, close),
                        format!("{text}\n{closure_indent}"),
                    ));
                }
            }
            edits.push(Edit::new(consecutive_span, ""));
        } else {
            // 3. rename and replace withConsecutive()
            let mut uses = Vec::new();
            // rector-phpunit (2.9.2's bundle) also reads the `willReturnOnConsecutiveCalls()` arguments
            let args: Vec<Node<'_, '_>> = with_consecutive
                .argument_list
                .arguments
                .iter()
                .chain(
                    on_consecutive_calls
                        .iter()
                        .flat_map(|m| m.argument_list.arguments.iter()),
                )
                .map(Node::Argument)
                .collect();
            let returned_node = match &returned {
                Some(Return::Value(e) | Return::Argument(e) | Return::Throw(e)) => {
                    vec![Node::Expression(e)]
                }
                None => Vec::new(),
            };
            if used_variables(&args, &mut uses).is_none()
                || used_variables(&returned_node, &mut uses).is_none()
            {
                return;
            }
            if let Some(m) = on_consecutive_calls {
                let values: Vec<&Expression<'_>> = m
                    .argument_list
                    .arguments
                    .iter()
                    .map(Argument::value)
                    .collect();
                for (key, b) in blocks.iter_mut().enumerate() {
                    let Block::If(_, body) = b else { continue };
                    let Some(value) = values.get(key) else { return };
                    body.push(vec![
                        lit("return "),
                        orig(*value, node_span(*value).start),
                        lit(";"),
                    ]);
                }
            }
            if let Some(r) = &returned {
                blocks.push(Block::Stmt(match r {
                    Return::Value(e) => {
                        vec![lit("return "), orig(*e, node_span(*e).start), lit(";")]
                    }
                    Return::Argument(e) => vec![
                        lit("return $parameters["),
                        orig(*e, node_span(*e).start),
                        lit("];"),
                    ],
                    Return::Throw(e) => {
                        vec![lit("throw "), orig(*e, node_span(*e).start), lit(";")]
                    }
                }));
            }
            let mut use_list = vec!["$matcher".to_string()];
            use_list.extend(uses.iter().map(|u| format!("${u}")));
            let closure = |base: &str| {
                let inner = format!("{base}    ");
                let mut text = format!(
                    "function {}(...$parameters) use ({}) {{",
                    if by_ref { "&" } else { "" },
                    use_list.join(", ")
                );
                for s in render_blocks(source, &blocks, invocation, &inner) {
                    text.push('\n');
                    text.push_str(&inner);
                    text.push_str(&s);
                }
                text.push('\n');
                text.push_str(base);
                text.push('}');
                text
            };
            let statement_indent = line_indent(source, node_span(stmt).start);
            let consecutive_args = &with_consecutive.argument_list.arguments;
            if consecutive_args.len() == 1 {
                let arg = consecutive_args.first().unwrap();
                edits.push(Edit::new(
                    node_span(&with_consecutive.method),
                    "willReturnCallback",
                ));
                edits.push(Edit::new(
                    node_span(arg),
                    closure(line_indent(source, node_span(arg).start)),
                ));
            } else {
                edits.push(Edit::new(
                    consecutive_span,
                    format!("->willReturnCallback({})", closure(statement_indent)),
                ));
            }
            // fallback to default by case count
            let matcher = match expects_call {
                Some(e) => render(source, &[orig(e, node_span(e).start)], statement_indent),
                None => format!("$this->exactly({})", consecutive_args.len()),
            };
            let at = previous_token_end(
                ctx,
                block.left_brace.end.offset as usize,
                node_span(stmt).start,
            );
            edits.push(Edit::new(
                Span::new(at, at),
                format!("\n{statement_indent}$matcher = {matcher};"),
            ));
        }
        edits.sort_by_key(|e| (e.span.start, e.span.end));
        if edits.windows(2).any(|w| w[0].span.end > w[1].span.start) {
            return;
        }
        out.push(Report::fixed(
            node_span(&with_consecutive.method),
            "Replace the removed `withConsecutive()` with a `willReturnCallback()` that asserts each call.",
            Safety::Unsafe,
            crate::rules::support::rewrites::merge(edits),
        ));
    }
}
