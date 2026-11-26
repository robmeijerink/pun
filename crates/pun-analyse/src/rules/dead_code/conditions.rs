use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use mago_allocator::LocalArena;
use mago_database::file::File;

use mago_codex::assertion::Assertion;
use mago_codex::metadata::function_like::FunctionLikeMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::comparator::ComparisonResult;
use mago_codex::ttype::comparator::union_comparator;
use mago_codex::ttype::union::TUnion;
use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;
use mago_syntax::walker::MutWalker;

use super::super::Checker;
use super::super::members;
use super::super::types::flow::unparen;
use super::finally;
use super::noop::{self, asserts, function_of};
use crate::codebase::parse;
use mago_codex::symbol::SymbolKind;
use pun_phpstan::ignore::Error;
use pun_phpstan::types::{
    Accessory, Level, NeverKind, Tri, Type, float_to_string, intersect, is_super_type_of,
    recommended_level, union,
};

const REMOVE_TIP: &str = "Remove remaining cases below this one and this error will disappear too.";
const PHPDOC_TIP: &str = "Because the type is coming from a PHPDoc, you can turn off this check by setting <fg=cyan>treatPhpDocTypesAsCertain: false</> in your <fg=cyan>%configurationFile%</>.";

#[derive(Clone, Copy)]
enum CheckTypes {
    Full,
    Native,
}

impl CheckTypes {
    fn map<'a>(self, c: &'a Checker<'_, '_>) -> &'a HashMap<Span, TUnion> {
        match self {
            Self::Full => &c.types,
            Self::Native => &c.native_types,
        }
    }

    fn argument(self, c: &Checker, expression: &Expression) -> Option<Type> {
        match self {
            Self::Full => c.phpstan_type(expression),
            Self::Native => c.phpstan_type_in(expression, &c.native_types),
        }
    }
}

const TYPE_CHECKS: &[&str] = &[
    "array_is_list",
    "array_key_exists",
    "assert",
    "class_exists",
    "ctype_digit",
    "enum_exists",
    "fnmatch",
    "in_array",
    "interface_exists",
    "is_a",
    "is_array",
    "is_bool",
    "is_callable",
    "is_countable",
    "is_double",
    "is_float",
    "is_int",
    "is_integer",
    "is_iterable",
    "is_long",
    "is_null",
    "is_numeric",
    "is_object",
    "is_real",
    "is_scalar",
    "is_string",
    "is_subclass_of",
    "key_exists",
    "mb_stripos",
    "mb_strlen",
    "mb_strpos",
    "mb_strripos",
    "mb_strrpos",
    "mb_strstr",
    "method_exists",
    "preg_match",
    "preg_match_all",
    "property_exists",
    "settype",
    "str_contains",
    "str_ends_with",
    "str_starts_with",
    "stripos",
    "strlen",
    "strpos",
    "strripos",
    "strrpos",
    "strstr",
    "trait_exists",
];

const NEVER: &[&str] = &[
    "never",
    "never-return",
    "never-returns",
    "no-return",
    "noreturn",
];

#[derive(Default)]
pub(in super::super) struct Facts {
    last: HashMap<Span, bool>,
    never_loops: HashSet<Span>,
    match_arms: HashSet<Span>,
    caught: HashMap<Span, Vec<Span>>,
    throws: HashSet<Span>,
    pub(in super::super) exact: HashSet<Span>,
}

pub(in super::super) fn facts(c: &Checker, program: &Program) -> Facts {
    let mut pass = Pass {
        contents: &c.file.contents,
        comments: &c.facts.comments,
        functions: Vec::new(),
        tries: Vec::new(),
        facts: Facts::default(),
    };
    if c.level >= 4 {
        pass.walk_program(program, &mut ());
        pass.facts.exact = super::impure_points::exact_variables(program);
    }
    pass.facts
}

struct Pass<'c> {
    contents: &'c [u8],
    comments: &'c super::super::Comments,
    functions: Vec<(bool, bool, bool)>,
    tries: Vec<(usize, Vec<Span>)>,
    facts: Facts,
}

fn body<'s, 'arena>(s: &'s [Statement<'arena>]) -> &'s [Statement<'arena>] {
    match s {
        [Statement::Block(b)] => b.statements.as_slice(),
        s => s,
    }
}

fn is_throw(s: &Statement) -> bool {
    matches!(s, Statement::Expression(e) if matches!(e.expression, Expression::Throw(_)))
}

impl Pass<'_> {
    fn list(&mut self, statements: &[Statement]) {
        if let [.., Statement::If(i), last] = statements
            && is_throw(last)
            && i.body.else_statements().is_none()
        {
            let cond = i
                .body
                .else_if_clauses()
                .last()
                .map_or(i.condition, |(e, _)| *e);
            self.facts.last.insert(unparen(cond).span(), true);
        }
    }

    fn enter(&mut self, named: bool, start: Span, hint: Option<&FunctionLikeReturnTypeHint>) {
        let native = hint.is_some_and(|h| matches!(h.hint, Hint::Never(_)));
        let doc = self
            .comments
            .php_doc_comment(self.contents, start.start.offset as usize)
            .is_some_and(|doc| {
                String::from_utf8_lossy(doc).lines().any(|line| {
                    let mut words = line
                        .trim_start_matches([' ', '\t', '*', '/'])
                        .split_whitespace();
                    matches!(
                        words.next(),
                        Some("@return" | "@phpstan-return" | "@psalm-return")
                    ) && words
                        .next()
                        .is_some_and(|t| NEVER.contains(&t.to_ascii_lowercase().as_str()))
                })
            });
        let throws = self
            .comments
            .php_doc_comment(self.contents, start.start.offset as usize)
            .is_some_and(|doc| {
                String::from_utf8_lossy(doc).lines().any(|line| {
                    let mut words = line
                        .trim_start_matches([' ', '\t', '*', '/'])
                        .split_whitespace();
                    matches!(words.next(), Some("@throws" | "@phpstan-throws"))
                        && words.next().is_some_and(|t| {
                            t.split('|').any(|class| {
                                let class = class
                                    .rsplit('\\')
                                    .next()
                                    .unwrap_or_default()
                                    .to_ascii_lowercase();
                                ["unhandledmatcherror", "error", "throwable"]
                                    .contains(&class.as_str())
                            })
                        })
                })
            });
        self.functions.push((named, native || doc, throws));
    }
}

impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for Pass<'_> {
    fn walk_in_function(&mut self, f: &'ast Function<'arena>, (): &mut ()) {
        self.enter(true, f.span(), f.return_type_hint.as_ref());
        self.list(f.body.statements.as_slice());
    }
    fn walk_out_function(&mut self, _: &'ast Function<'arena>, (): &mut ()) {
        self.functions.pop();
    }
    fn walk_in_method(&mut self, m: &'ast Method<'arena>, (): &mut ()) {
        self.enter(true, m.span(), m.return_type_hint.as_ref());
        if let MethodBody::Concrete(b) = &m.body {
            self.list(b.statements.as_slice());
        }
    }
    fn walk_out_method(&mut self, _: &'ast Method<'arena>, (): &mut ()) {
        self.functions.pop();
    }
    fn walk_in_closure(&mut self, f: &'ast Closure<'arena>, (): &mut ()) {
        self.enter(false, f.span(), f.return_type_hint.as_ref());
    }
    fn walk_out_closure(&mut self, _: &'ast Closure<'arena>, (): &mut ()) {
        self.functions.pop();
    }
    fn walk_in_arrow_function(&mut self, f: &'ast ArrowFunction<'arena>, (): &mut ()) {
        self.enter(false, f.span(), f.return_type_hint.as_ref());
    }
    fn walk_out_arrow_function(&mut self, _: &'ast ArrowFunction<'arena>, (): &mut ()) {
        self.functions.pop();
    }
    fn walk_in_namespace(&mut self, n: &'ast Namespace<'arena>, (): &mut ()) {
        self.list(n.statements().as_slice());
    }
    fn walk_in_try(&mut self, t: &'ast Try<'arena>, (): &mut ()) {
        fn names(h: &Hint, out: &mut Vec<Span>) {
            match h {
                Hint::Union(u) => {
                    names(u.left, out);
                    names(u.right, out);
                }
                h => out.push(h.span()),
            }
        }
        let mut caught = Vec::new();
        for clause in &t.catch_clauses {
            names(&clause.hint, &mut caught);
        }
        self.tries.push((self.functions.len(), caught));
        self.list(t.block.statements.as_slice());
        if let Some(f) = &t.finally_clause {
            self.list(f.block.statements.as_slice());
        }
    }
    fn walk_out_try(&mut self, _: &'ast Try<'arena>, (): &mut ()) {
        self.tries.pop();
    }
    fn walk_in_property_hook(&mut self, h: &'ast PropertyHook<'arena>, (): &mut ()) {
        self.enter(true, h.span(), None);
    }
    fn walk_out_property_hook(&mut self, _: &'ast PropertyHook<'arena>, (): &mut ()) {
        self.functions.pop();
    }
    fn walk_in_match(&mut self, m: &'ast Match<'arena>, (): &mut ()) {
        if self
            .functions
            .iter()
            .rev()
            .find(|(named, ..)| *named)
            .is_some_and(|(.., throws)| *throws)
        {
            self.facts.throws.insert(m.span());
        }
        let depth = self.functions.len();
        let caught: Vec<Span> = self
            .tries
            .iter()
            .filter(|(d, _)| *d == depth)
            .flat_map(|(_, c)| c.clone())
            .collect();
        if !caught.is_empty() {
            self.facts.caught.insert(m.span(), caught);
        }
        let last = m.arms.len().saturating_sub(1);
        for (n, arm) in m.arms.iter().enumerate() {
            if let MatchArm::Expression(a) = arm
                && let Some(cond) = a.conditions.iter().last()
            {
                let span = unparen(cond).span();
                self.facts.last.insert(span, n == last);
                self.facts.match_arms.insert(span);
            }
        }
    }
    fn walk_in_if(&mut self, i: &'ast If<'arena>, (): &mut ()) {
        let elseifs = i.body.else_if_clauses();
        if let Some(last) = elseifs.len().checked_sub(1) {
            let missing_or_throwing = i
                .body
                .else_statements()
                .is_none_or(|s| matches!(body(s), [s] if is_throw(s)));
            for (n, (cond, _)) in elseifs.iter().enumerate() {
                self.facts
                    .last
                    .insert(unparen(cond).span(), n == last && missing_or_throwing);
            }
        }
        self.list(body(i.body.statements()));
        for (_, s) in &elseifs {
            self.list(body(s));
        }
        if let Some(s) = i.body.else_statements() {
            self.list(body(s));
        }
    }
    fn walk_in_switch_case(&mut self, case: &'ast SwitchCase<'arena>, (): &mut ()) {
        self.list(case.statements());
    }
    fn walk_in_try_catch_clause(&mut self, clause: &'ast TryCatchClause<'arena>, (): &mut ()) {
        self.list(clause.block.statements.as_slice());
    }
    fn walk_in_do_while(&mut self, d: &'ast DoWhile<'arena>, (): &mut ()) {
        self.list(body(std::slice::from_ref(d.statement)));
    }
    fn walk_in_for(&mut self, f: &'ast For<'arena>, (): &mut ()) {
        self.list(body(f.body.statements()));
    }
    fn walk_in_foreach(&mut self, f: &'ast Foreach<'arena>, (): &mut ()) {
        self.list(body(f.body.statements()));
    }
    fn walk_in_while(&mut self, w: &'ast While<'arena>, (): &mut ()) {
        self.list(body(w.body.statements()));
        let function = self
            .functions
            .iter()
            .rev()
            .find(|(named, ..)| *named)
            .or(self.functions.last());
        if function.is_some_and(|(_, never, _)| *never) {
            self.facts.never_loops.insert(w.span());
        }
    }
}

pub(super) fn typed(program: &Program) -> Vec<Span> {
    struct Wanted {
        spans: Vec<Span>,
    }
    impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for Wanted {
        fn walk_in_if(&mut self, i: &'ast If<'arena>, (): &mut ()) {
            self.spans.push(i.condition.span());
        }
        fn walk_in_conditional(&mut self, t: &'ast Conditional<'arena>, (): &mut ()) {
            self.spans.push(t.condition.span());
        }
        fn walk_in_binary(&mut self, b: &'ast Binary<'arena>, (): &mut ()) {
            if logical(&b.operator).is_some() || comparison(&b.operator).is_some() {
                self.spans.extend([b.lhs.span(), b.rhs.span(), b.span()]);
            }
        }
        fn walk_in_argument_list(&mut self, a: &'ast ArgumentList<'arena>, (): &mut ()) {
            self.spans
                .extend(a.arguments.iter().map(|a| a.value().span()));
        }
        fn walk_in_unary_prefix(&mut self, u: &'ast UnaryPrefix<'arena>, (): &mut ()) {
            if matches!(u.operator, UnaryPrefixOperator::Not(_)) {
                self.spans.push(u.operand.span());
            }
        }
        fn walk_in_match(&mut self, m: &'ast Match<'arena>, (): &mut ()) {
            for arm in &m.arms {
                if let MatchArm::Expression(a) = arm {
                    self.spans.extend(a.conditions.iter().map(|c| c.span()));
                }
            }
        }
        fn walk_in_switch(&mut self, s: &'ast Switch<'arena>, (): &mut ()) {
            self.spans.push(s.expression.span());
            self.spans.extend(
                s.body
                    .cases()
                    .iter()
                    .filter_map(|c| c.expression())
                    .map(|e| e.span()),
            );
        }
    }
    let mut wanted = Wanted { spans: Vec::new() };
    wanted.walk_program(program, &mut ());
    wanted.spans
}

fn logical(op: &BinaryOperator) -> Option<(&'static str, &'static str)> {
    match op {
        BinaryOperator::And(_) => Some(("booleanAnd", "&&")),
        BinaryOperator::Or(_) => Some(("booleanOr", "||")),
        BinaryOperator::LowAnd(_) => Some(("logicalAnd", "and")),
        BinaryOperator::LowOr(_) => Some(("logicalOr", "or")),
        BinaryOperator::LowXor(_) => Some(("logicalXor", "xor")),
        _ => None,
    }
}

fn skipped(e: &Expression) -> bool {
    match e {
        Expression::Binary(b) => matches!(
            b.operator,
            BinaryOperator::Equal(_)
                | BinaryOperator::NotEqual(_)
                | BinaryOperator::AngledNotEqual(_)
                | BinaryOperator::Identical(_)
                | BinaryOperator::NotIdentical(_)
                | BinaryOperator::Instanceof(_)
                | BinaryOperator::And(_)
                | BinaryOperator::Or(_)
                | BinaryOperator::LessThan(_)
                | BinaryOperator::LessThanOrEqual(_)
                | BinaryOperator::GreaterThan(_)
                | BinaryOperator::GreaterThanOrEqual(_)
        ),
        Expression::UnaryPrefix(u) => matches!(u.operator, UnaryPrefixOperator::Not(_)),
        Expression::Conditional(_) => true,
        Expression::Construct(Construct::Isset(_) | Construct::Empty(_)) => true,
        _ => false,
    }
}

impl Checker<'_, '_> {
    fn constant(&self, e: &Expression) -> Option<bool> {
        if skipped(unparen(e)) || self.is_resource_check(e) {
            return None;
        }
        self.truthiness(e)
    }

    /// PHPStan has no type-specifying extension for `is_resource()`, whose result stays `bool`: a resource can be
    /// closed (by a callee, for one) while its type stays `resource`.
    fn is_resource_check(&self, e: &Expression) -> bool {
        matches!(unparen(e), Expression::Call(Call::Function(f))
            if matches!(function_of(self, f.function), Some(noop::Function::Builtin(b)) if b.name.eq_ignore_ascii_case("is_resource")))
    }

    fn conditions_checked(&self) -> bool {
        self.level >= 4
    }

    fn deferred(&self, e: &Expression) -> bool {
        match unparen(e) {
            Expression::Call(Call::Function(f)) => self.owned_function(f),
            Expression::Call(Call::Method(m)) => self.asserting(m.object, &m.method),
            Expression::Call(Call::StaticMethod(m)) => self.asserting(m.class, &m.method),
            _ => false,
        }
    }

    fn owned_function(&self, f: &FunctionCall) -> bool {
        function_of(self, f.function).is_some_and(|function| match function {
            noop::Function::User(f) => asserts(f),
            noop::Function::Builtin(b) => {
                let name = b.name.to_ascii_lowercase();
                TYPE_CHECKS.contains(&name.as_str())
            }
        })
    }

    fn exact(&self, span: Span) -> Option<bool> {
        self.types.get(&span).and_then(|t| {
            if t.is_true() {
                Some(true)
            } else {
                t.is_false().then_some(false)
            }
        })
    }

    fn truthiness(&self, e: &Expression) -> Option<bool> {
        let t = self
            .type_of(e)
            .filter(|t| !t.types.is_empty() && !t.is_never())?;
        if t.is_always_truthy() {
            Some(true)
        } else {
            t.is_always_falsy().then_some(false)
        }
    }

    fn asserted(&self, f: &FunctionLikeMetadata, args: &ArgumentList) -> Option<bool> {
        self.asserted_with(f, args, CheckTypes::Full)
            .and_then(Verdict::always)
    }

    fn asserted_with(
        &self,
        f: &FunctionLikeMetadata,
        args: &ArgumentList,
        types: CheckTypes,
    ) -> Option<Verdict> {
        let void = f
            .return_type_metadata
            .as_ref()
            .is_some_and(|r| r.type_union.is_void());
        let assertions = if void {
            &f.assertions
        } else {
            &f.if_true_assertions
        };
        if !void && f.assertions_inferred
            || assertions.is_empty()
            || assertions.values().flatten().any(|a| {
                matches!(
                    a,
                    Assertion::IsType(TAtomic::GenericParameter(_))
                        | Assertion::IsNotType(TAtomic::GenericParameter(_))
                )
            })
        {
            return None;
        }
        let mut all = Some(true);
        for (name, assertions) in assertions {
            let i = f.parameters.iter().position(|p| p.get_name().0 == *name)?;
            let arg = args
                .arguments
                .iter()
                .enumerate()
                .find_map(|(j, a)| match a {
                    Argument::Positional(a) if j == i => Some(a.value),
                    Argument::Named(a) if name.as_bytes()[1..] == *a.name.value => Some(a.value),
                    _ => None,
                })?;
            let given = types.map(self).get(&arg.span())?;
            for assertion in assertions {
                let (Assertion::IsType(a) | Assertion::IsNotType(a)) = assertion else {
                    return None;
                };
                let wanted = TUnion::from_atomic(a.clone());
                let is = if union_comparator::is_contained_by(
                    self.codebase,
                    given,
                    &wanted,
                    false,
                    false,
                    false,
                    &mut ComparisonResult::new(),
                ) {
                    Some(true)
                } else if union_comparator::can_expression_types_be_identical(
                    self.codebase,
                    given,
                    &wanted,
                    false,
                    false,
                ) {
                    None
                } else {
                    Some(false)
                };
                let is = is.map(|b| b == matches!(assertion, Assertion::IsType(_)));
                all = match (all, is) {
                    (Some(false), _) | (_, Some(false)) => Some(false),
                    (Some(true), Some(true)) => Some(true),
                    _ => None,
                };
            }
        }
        Some(all.into())
    }

    fn asserting(&self, on: &Expression, method: &ClassLikeMemberSelector) -> bool {
        matches!(self.method_target(on, method), Some(members::Method::User { f, .. }) if asserts(f))
    }

    fn constant_condition(
        &mut self,
        e: &Expression,
        line_of: Span,
        message: &str,
        identifier: &str,
        tip: Option<&str>,
    ) -> Option<bool> {
        let Some(value) = self.constant(e) else {
            self.no_condition(unparen(e).span(), &format!("{identifier}True"));
            return None;
        };
        if !self.deferred(e) {
            let line = self.line_of(line_of);
            let (text, suffix) = if value {
                ("true", "True")
            } else {
                ("false", "False")
            };
            self.condition_error(
                unparen(e).span(),
                line,
                &format!("{identifier}{suffix}"),
                value,
                format!("{message} is always {text}."),
                tip,
            );
        }
        Some(value)
    }

    fn last_condition(&self, span: Span) -> Option<bool> {
        self.conditions.last.get(&span).copied()
    }
}

pub(in super::super) fn if_statement(c: &mut Checker, i: &If) {
    if !c.conditions_checked() {
        return;
    }
    let tip = c.native_types.get(&i.condition.span())
        .filter(|ty| !ty.is_never() && !ty.is_always_truthy() && !ty.is_always_falsy())
        .map(|_| "Because the type is coming from a PHPDoc, you can turn off this check by setting <fg=cyan>treatPhpDocTypesAsCertain: false</> in your <fg=cyan>%configurationFile%</>.");
    c.constant_condition(
        i.condition,
        unparen(i.condition).span(),
        "If condition",
        "if.always",
        tip,
    );
    for (cond, _) in i.body.else_if_clauses() {
        let last = c.last_condition(unparen(cond).span());
        if c.constant(cond) == Some(true) && last == Some(true) {
            continue;
        }
        let tip = (last == Some(false)).then_some(REMOVE_TIP);
        let tip = tip.filter(|_| c.constant(cond) == Some(true));
        c.constant_condition(
            cond,
            unparen(cond).span(),
            "Elseif condition",
            "elseif.always",
            tip,
        );
    }
}

pub(in super::super) fn ternary(c: &mut Checker, t: &Conditional) {
    if !c.conditions_checked() {
        return;
    }
    c.constant_condition(
        t.condition,
        t.span(),
        "Ternary operator condition",
        "ternary.always",
        None,
    );
}

pub(in super::super) fn while_loop(c: &mut Checker, w: &While) {
    if !c.conditions_checked() {
        return;
    }
    match c.constant(w.condition) {
        Some(false) => {
            c.constant_condition(
                w.condition,
                w.span(),
                "While loop condition",
                "while.always",
                None,
            );
        }
        Some(true) => {
            let body = finally::statements(c, w.body.statements());
            let exits = !body.exits.is_empty()
                || body
                    .jumps
                    .iter()
                    .any(|(_, kind, n)| *kind == "break" || *n > 1);
            if !exits
                && !has_yield(w.body.statements())
                && !c.conditions.never_loops.contains(&w.span())
            {
                c.constant_condition(
                    w.condition,
                    w.span(),
                    "While loop condition",
                    "while.always",
                    None,
                );
            }
        }
        None => {}
    }
}

pub(in super::super) fn do_while(c: &mut Checker, d: &DoWhile) {
    if !c.conditions_checked() {
        return;
    }
    let body = std::slice::from_ref(d.statement);
    let exits = finally::statements(c, body);
    let reported = match c.constant(d.condition) {
        Some(true) => {
            exits.exits.is_empty()
                && !has_yield(body)
                && exits
                    .jumps
                    .iter()
                    .all(|(_, kind, n)| *kind == "continue" && *n <= 1)
        }
        Some(false) => !exits.jumps.iter().any(|(_, kind, _)| *kind == "break"),
        None => false,
    };
    if reported {
        c.constant_condition(
            d.condition,
            unparen(d.condition).span(),
            "Do-while loop condition",
            "doWhile.always",
            None,
        );
    }
}

pub(in super::super) fn boolean_not(c: &mut Checker, u: &UnaryPrefix) {
    if !c.conditions_checked() || !matches!(u.operator, UnaryPrefixOperator::Not(_)) {
        return;
    }
    let Some(value) = c.constant(u.operand) else {
        c.no_condition(u.span(), "booleanNot.alwaysTrue");
        return;
    };
    let last = c.last_condition(u.span());
    if !value && last == Some(true) {
        return;
    }
    let tip = (!value && last == Some(false)).then_some(REMOVE_TIP);
    if !c.deferred(u.operand) {
        let line = c.line_of(unparen(u.operand).span());
        let (text, suffix) = if value {
            ("false", "False")
        } else {
            ("true", "True")
        };
        c.condition_error(
            u.span(),
            line,
            &format!("booleanNot.always{suffix}"),
            !value,
            format!("Negated boolean expression is always {text}."),
            tip,
        );
    }
}

fn comparison(op: &BinaryOperator) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match op {
        BinaryOperator::Identical(_) => ("identical", "===", "Strict"),
        BinaryOperator::NotIdentical(_) => ("notIdentical", "!==", "Strict"),
        BinaryOperator::Equal(_) => ("equal", "==", "Loose"),
        BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_) => {
            ("notEqual", "!=", "Loose")
        }
        BinaryOperator::GreaterThan(_) => ("greater", ">", ""),
        BinaryOperator::GreaterThanOrEqual(_) => ("greaterOrEqual", ">=", ""),
        BinaryOperator::LessThan(_) => ("smaller", "<", ""),
        BinaryOperator::LessThanOrEqual(_) => ("smallerOrEqual", "<=", ""),
        BinaryOperator::Instanceof(_) => ("instanceof", "instanceof", "Instanceof"),
        _ => return None,
    })
}

fn compare(c: &mut Checker, b: &Binary) {
    let Some((identifier, sigil, kind)) = comparison(&b.operator) else {
        return;
    };
    if kind == "Instanceof"
        && !matches!(
            b.rhs,
            Expression::Identifier(_)
                | Expression::Self_(_)
                | Expression::Static(_)
                | Expression::Parent(_)
        )
    {
        let allowed = Type::Union(vec![Type::String, Type::ObjectWithoutClass]);
        let found = c.find_type_to_check(b.rhs, &|t| !is_super_type_of(&allowed, t).yes());
        if found
            .ty
            .is_some_and(|t| !is_super_type_of(&allowed, &t).yes())
            && let (Some(left), Some(class)) = (c.phpstan_type(b.lhs), c.phpstan_type(b.rhs))
        {
            let (l, r) = super::super::declared::with_classes(c, || {
                (
                    left.describe(Level::TypeOnly),
                    class.describe(Level::TypeOnly),
                )
            });
            let line = c.line_of(b.span());
            c.report_at_line(
                line,
                "instanceof.invalidExprType",
                format!("Instanceof between {l} and {r} results in an error."),
                None,
            );
            return;
        }
    }
    let value = c.exact(b.span());
    let right = match (kind, b.rhs) {
        ("Instanceof", Expression::Identifier(id)) => {
            Some(Type::Object(c.resolved(id), vec![], false))
        }
        ("Instanceof", Expression::Self_(_) | Expression::Static(_)) => c
            .class_scope()
            .map(|k| Type::Object(c.display(k), vec![], false)),
        _ => c.phpstan_type(b.rhs),
    };
    let (Some(value), Some(left), Some(right)) = (value, c.phpstan_type(b.lhs), right) else {
        c.no_condition(b.span(), &format!("{identifier}.alwaysTrue"));
        return;
    };
    let last = c.last_condition(b.span());
    let (text, suffix) = if value {
        ("true", "True")
    } else {
        ("false", "False")
    };
    let (l, r) = super::super::declared::with_classes(c, || {
        if kind == "Instanceof" {
            (
                left.describe(Level::TypeOnly),
                right.describe(recommended_level(&right, None)),
            )
        } else {
            (left.describe(Level::Value), right.describe(Level::Value))
        }
    });
    let message = if kind == "Instanceof" {
        if value && last == Some(true) {
            return;
        }
        format!("Instanceof between {l} and {r} will always evaluate to {text}.")
    } else if kind.is_empty() {
        format!("Comparison operation \"{sigil}\" between {l} and {r} is always {text}.")
    } else {
        if value && last == Some(true) {
            return;
        }
        format!(
            "{kind} comparison using {sigil} between {l} and {r} will always evaluate to {text}."
        )
    };
    let mut tips = Vec::new();
    if kind == "Strict"
        && [(b.lhs, &right), (b.rhs, &left)].iter().any(|(expression, other)| {
            let Expression::Call(Call::Function(call)) = unparen(expression) else { return false };
            !other.is_integer().no()
                && matches!(function_of(c, call.function), Some(noop::Function::Builtin(function))
            if function.name.eq_ignore_ascii_case("count") || function.name.eq_ignore_ascii_case("sizeof"))
        })
    {
        tips.push("Because the type is coming from a PHPDoc, you can turn off this check by setting treatPhpDocTypesAsCertain: false in your %configurationFile%.");
    }
    if value && last == Some(false) && !kind.is_empty() {
        tips.push(REMOVE_TIP);
    }
    if value
        && kind == "Strict"
        && !c.conditions.match_arms.contains(&b.span())
        && [&left, &right]
            .iter()
            .all(|t| matches!(t, Type::EnumCase(..)))
    {
        tips.push("Use match expression instead. PHPStan will report unhandled enum cases.");
    }
    let tip = match tips.as_slice() {
        [] => None,
        [tip] => Some(tip.to_string()),
        tips => Some(
            tips.iter()
                .map(|t| format!("• {t}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    };
    let line = c.line_of(b.span());
    c.condition_error(
        b.span(),
        line,
        &format!("{identifier}.always{suffix}"),
        value,
        message,
        tip.as_deref(),
    );
}

pub(in super::super) fn binary(c: &mut Checker, b: &Binary) {
    let Some((identifier, sigil)) = logical(&b.operator) else {
        if c.conditions_checked() {
            compare(c, b);
        }
        return;
    };
    if !c.conditions_checked() {
        return;
    }
    let last = c.last_condition(b.span());
    let xor = matches!(b.operator, BinaryOperator::LowXor(_));
    let top_level = c.is_top_level(b.span());
    let sigil = if xor { "xor" } else { sigil };
    let side = |c: &mut Checker, e: &Expression, which: &str| -> bool {
        let Some(value) = c.constant(e) else {
            return false;
        };
        if value && last == Some(true) {
            return false;
        }
        let tip = (value && last == Some(false)).then_some(REMOVE_TIP);
        let side = if which == "left" { "Left" } else { "Right" };
        c.constant_condition(
            e,
            unparen(e).span(),
            &format!("{side} side of {sigil}"),
            &format!("{identifier}.{which}Always"),
            tip,
        );
        true
    };
    let left = side(c, b.lhs, "left");
    let skipped = c
        .types
        .get(&b.lhs.span())
        .is_some_and(|t| match b.operator {
            BinaryOperator::And(_) | BinaryOperator::LowAnd(_) => t.is_always_falsy(),
            BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => t.is_always_truthy(),
            _ => false,
        });
    let right = (xor || !top_level) && !skipped && side(c, b.rhs, "right");
    if xor || left || right || top_level {
        return;
    }
    let Some(value) = c.types.get(&b.span()).and_then(|t| {
        if t.is_true() {
            Some(true)
        } else if t.is_false() {
            Some(false)
        } else {
            None
        }
    }) else {
        return;
    };
    if value && last == Some(true) {
        return;
    }
    let tip = (value && last == Some(false)).then_some(REMOVE_TIP);
    let (text, suffix) = if value {
        ("true", "True")
    } else {
        ("false", "False")
    };
    let line = c.line_of(b.span());
    c.condition_error(
        b.span(),
        line,
        &format!("{identifier}.always{suffix}"),
        value,
        format!("Result of {sigil} is always {text}."),
        tip,
    );
}

pub(in super::super) fn function_check(c: &mut Checker, call: &FunctionCall) {
    if !c.conditions_checked() || !c.owned_function(call) {
        return;
    }
    let name = c.written(call.function.span());
    let subject = format!("function {}", name.trim_start_matches('\\'));
    let value = c
        .function_decision(call, CheckTypes::Full)
        .and_then(Verdict::always);
    let phpdoc = value.is_some()
        && c.function_decision(call, CheckTypes::Native) == Some(Verdict::Undecided);
    impossible_check(
        c,
        call.span(),
        value,
        "function",
        subject,
        &call.argument_list,
        phpdoc,
    );
}

pub(in super::super) fn method_check(c: &mut Checker, call: &MethodCall) {
    method_report(
        c,
        call.span(),
        call.object,
        &call.method,
        "method",
        "method",
        &call.argument_list,
    );
}

pub(in super::super) fn static_method_check(c: &mut Checker, call: &StaticMethodCall) {
    method_report(
        c,
        call.span(),
        call.class,
        &call.method,
        "staticMethod",
        "static method",
        &call.argument_list,
    );
}

fn method_report(
    c: &mut Checker,
    span: Span,
    on: &Expression,
    method: &ClassLikeMemberSelector,
    identifier: &str,
    what: &str,
    args: &ArgumentList,
) {
    if !c.conditions_checked() {
        return;
    }
    let (subject, asserted) = match c.method_target(on, method) {
        Some(m @ members::Method::User { f, .. }) if asserts(f) => (
            format!("{what} {}::{}", m.declaring_display(c), m.name()),
            c.asserted(f, args),
        ),
        _ => return,
    };
    let value = c.exact(span).or(asserted);
    impossible_check(c, span, value, identifier, subject, args, false);
}

fn impossible_check(
    c: &mut Checker,
    span: Span,
    value: Option<bool>,
    identifier: &str,
    subject: String,
    args: &ArgumentList,
    phpdoc: bool,
) {
    let last = c.last_condition(span);
    let Some(value) = value.filter(|v| !(*v && last == Some(true))) else {
        c.no_condition(span, &format!("{identifier}.impossibleType"));
        return;
    };
    let types: Option<Vec<_>> = args
        .arguments
        .iter()
        .map(|a| c.phpstan_type(a.value()))
        .collect();
    let Some(types) = types else { return };
    let mut described: Vec<String> = super::super::declared::with_classes(c, || {
        types.iter().map(|t| t.describe(Level::Value)).collect()
    });
    let with = match described.len() {
        0..=2 => described
            .first()
            .map(|_| format!(" with {}", described.join(" and ")))
            .unwrap_or_default(),
        _ => {
            let last = described.pop().unwrap_or_default();
            format!(" with arguments {} and {last}", described.join(", "))
        }
    };
    let (text, suffix) = if value {
        ("true", "alreadyNarrowedType")
    } else {
        ("false", "impossibleType")
    };
    let tip = if value && last == Some(false) {
        Some(REMOVE_TIP)
    } else {
        phpdoc.then_some(PHPDOC_TIP)
    };
    let line = c.line_of(span);
    c.condition_error(
        span,
        line,
        &format!("{identifier}.{suffix}"),
        value,
        format!("Call to {subject}(){with} will always evaluate to {text}."),
        tip,
    );
}

fn has_yield(statements: &[Statement]) -> bool {
    struct Finder(bool);
    impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for Finder {
        fn walk_in_yield(&mut self, _: &'ast Yield<'arena>, (): &mut ()) {
            self.0 = true;
        }
        fn walk_closure(&mut self, _: &'ast Closure<'arena>, (): &mut ()) {}
        fn walk_arrow_function(&mut self, _: &'ast ArrowFunction<'arena>, (): &mut ()) {}
        fn walk_anonymous_class(&mut self, _: &'ast AnonymousClass<'arena>, (): &mut ()) {}
    }
    let mut finder = Finder(false);
    for s in statements {
        finder.walk_statement(s, &mut ());
    }
    finder.0
}

fn void_null(t: Type) -> Type {
    if t == Type::Void { Type::Null } else { t }
}

fn single(t: &Type) -> bool {
    matches!(
        t,
        Type::ConstInt(_)
            | Type::ConstString(_)
            | Type::ConstBool(_)
            | Type::Null
            | Type::EnumCase(..)
    )
}

fn identical(subject: &Type, arm: &Type) -> Option<bool> {
    if single(subject) && subject == arm {
        return Some(true);
    }
    (is_super_type_of(subject, arm) == Tri::No).then_some(false)
}

impl Checker<'_, '_> {
    fn enum_cases(&self, class: &str) -> Option<Vec<Type>> {
        let k = self.codebase.get_class_like(class.as_bytes())?;
        (k.kind == SymbolKind::Enum).then(|| {
            k.enum_cases
                .keys()
                .map(|case| {
                    Type::EnumCase(
                        class.to_string(),
                        String::from_utf8_lossy(case.as_bytes()).into_owned(),
                    )
                })
                .collect()
        })
    }

    fn subtractable(&self, t: &Type) -> bool {
        match t {
            Type::Union(ts) => ts.iter().all(|t| self.subtractable(t)),
            Type::Object(class, _, _) | Type::Static(class, _) => self.enum_cases(class).is_some(),
            t => matches!(
                t,
                Type::Int
                    | Type::IntRange(..)
                    | Type::ConstInt(_)
                    | Type::String
                    | Type::ConstString(_)
                    | Type::Bool
                    | Type::ConstBool(_)
                    | Type::Null
                    | Type::EnumCase(..)
            ),
        }
    }

    fn expand_enums(&self, t: Type) -> Type {
        match t {
            Type::Union(ts) => union(ts.into_iter().map(|t| self.expand_enums(t)).collect()),
            Type::Object(ref class, _, _) | Type::Static(ref class, _) => {
                let cases = self.enum_cases(class);
                cases.map_or(t, union)
            }
            t => t,
        }
    }

    fn remove(&self, t: Type, value: &Type) -> Type {
        match t {
            Type::Union(ts) => union(ts.into_iter().map(|t| self.remove(t, value)).collect()),
            t if t == *value => Type::Never(NeverKind::Implicit),
            Type::Bool if matches!(value, Type::ConstBool(_)) => {
                Type::ConstBool(*value == Type::ConstBool(false))
            }
            Type::Int | Type::IntRange(..) if let Type::ConstInt(n) = value => {
                let (min, max) = match t {
                    Type::IntRange(min, max) => (min, max),
                    _ => (None, None),
                };
                if min.is_some_and(|m| *n < m) || max.is_some_and(|m| *n > m) {
                    return t;
                }
                union(vec![
                    Type::int_range(min, n.checked_sub(1)),
                    Type::int_range(n.checked_add(1), max),
                ])
            }
            Type::Object(ref class, _, _) | Type::Static(ref class, _) if matches!(value, Type::EnumCase(c, _) if c.eq_ignore_ascii_case(class)) =>
            {
                let cases = self.enum_cases(class);
                cases.map_or(t, |cases| {
                    union(cases.into_iter().filter(|case| case != value).collect())
                })
            }
            t => t,
        }
    }
}

pub(super) fn finite(c: &Checker, t: &Type) -> Vec<Type> {
    match c.expand_enums(t.clone()) {
        Type::Union(ts) => {
            let all: Vec<Vec<Type>> = ts.iter().map(|t| finite(c, t)).collect();
            if all.iter().any(Vec::is_empty) {
                Vec::new()
            } else {
                all.concat()
            }
        }
        Type::Bool => vec![Type::ConstBool(true), Type::ConstBool(false)],
        t if single(&t) || matches!(t, Type::ConstFloat(_)) => vec![t],
        _ => Vec::new(),
    }
}

fn constant_strings(t: &Type) -> Vec<String> {
    match t {
        Type::ConstString(s) => vec![String::from_utf8_lossy(s).into_owned()],
        Type::Union(ts) => ts
            .iter()
            .map(constant_strings)
            .try_fold(Vec::new(), |mut all, s| {
                (!s.is_empty()).then(|| {
                    all.extend(s);
                    all
                })
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn has_constant_array(t: &Type) -> bool {
    match t {
        Type::Union(ts) | Type::Intersection(ts) => ts.iter().any(has_constant_array),
        t => matches!(t, Type::ConstArray(_)),
    }
}

fn extreme(results: impl IntoIterator<Item = Tri>) -> Tri {
    results
        .into_iter()
        .reduce(|a, b| if a == b { a } else { Tri::Maybe })
        .unwrap_or(Tri::Maybe)
}

/// What a check that applies to a condition decides about it. A check that does not apply returns `None` instead.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Undecided,
    Always(bool),
}

impl Verdict {
    fn always(self) -> Option<bool> {
        match self {
            Verdict::Undecided => None,
            Verdict::Always(value) => Some(value),
        }
    }
}

impl From<Option<bool>> for Verdict {
    fn from(value: Option<bool>) -> Verdict {
        value.map_or(Verdict::Undecided, Verdict::Always)
    }
}

fn decided(t: Tri) -> Option<bool> {
    (!t.maybe()).then(|| t.yes())
}

impl Checker<'_, '_> {
    fn function_decision(&self, call: &FunctionCall, types: CheckTypes) -> Option<Verdict> {
        let args = &call.argument_list;
        if self
            .written(call.function.span())
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("assert")
        {
            let ty = types
                .map(self)
                .get(&args.arguments.first()?.value().span())
                .filter(|ty| !ty.types.is_empty() && !ty.is_never())?;
            return Some(if ty.is_always_truthy() {
                Verdict::Always(true)
            } else if ty.is_always_falsy() {
                Verdict::Always(false)
            } else {
                Verdict::Undecided
            });
        }
        match function_of(self, call.function)? {
            noop::Function::User(function) => {
                let asserted = self.asserted_with(function, args, types);
                match types {
                    CheckTypes::Full => Some(
                        self.exact(call.span())
                            .or(asserted.and_then(Verdict::always))
                            .into(),
                    ),
                    CheckTypes::Native => asserted,
                }
            }
            noop::Function::Builtin(function) => self.special(
                &function.name.to_ascii_lowercase(),
                args,
                types,
                call.span(),
            ),
        }
    }

    fn special(
        &self,
        name: &str,
        args: &ArgumentList,
        types: CheckTypes,
        call: Span,
    ) -> Option<Verdict> {
        let arg = |i: usize| {
            args.arguments
                .get(i)
                .and_then(|a| types.argument(self, a.value()))
        };
        Some(Verdict::from(match name {
            "in_array" => {
                let needle = arg(0)?;
                let haystack = arg(1)?;
                return super::super::declared::with_classes(self, || {
                    self.in_array(
                        args,
                        types,
                        needle,
                        haystack,
                        args.arguments
                            .get(2)
                            .and_then(|a| self.phpstan_type(a.value())),
                    )
                });
            }
            "method_exists" => self.method_exists(arg(0)?, arg(1)?),
            "property_exists" => self.property_exists(arg(0)?, arg(1)?),
            "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
                let strings = constant_strings(&arg(0)?);
                let wrong_kind = |s: &String| {
                    self.codebase
                        .get_class_like(s.trim_start_matches('\\').as_bytes())
                        .is_some_and(|k| match name {
                            "class_exists" => {
                                matches!(k.kind, SymbolKind::Interface | SymbolKind::Trait)
                            }
                            "interface_exists" => k.kind != SymbolKind::Interface,
                            "trait_exists" => k.kind != SymbolKind::Trait,
                            _ => k.kind != SymbolKind::Enum,
                        })
                };
                (!strings.is_empty() && strings.iter().all(wrong_kind)).then_some(false)
            }
            _ => {
                let mixed = || Box::new(pun_phpstan::types::mixed());
                let sure = match name {
                    "is_int" | "is_integer" | "is_long" => Type::Int,
                    "is_string" => Type::String,
                    "is_bool" => Type::Bool,
                    "is_float" | "is_double" | "is_real" => Type::Float,
                    "is_null" => Type::Null,
                    "is_array" => Type::Array(mixed(), mixed()),
                    "is_object" => Type::ObjectWithoutClass,
                    "is_iterable" => Type::Iterable(mixed(), mixed()),
                    _ => {
                        return match types {
                            CheckTypes::Full => Some(self.exact(call).into()),
                            CheckTypes::Native => None,
                        };
                    }
                };
                let argument = arg(0)?;
                super::super::declared::with_classes(self, || {
                    decided(is_super_type_of(&sure, &argument))
                })
            }
        }))
    }

    fn in_array(
        &self,
        args: &ArgumentList,
        types: CheckTypes,
        needle: Type,
        haystack: Type,
        strict: Option<Type>,
    ) -> Option<Verdict> {
        if !haystack.is_array().yes() {
            return Some(Verdict::Undecided);
        }
        let value = haystack.iterable_value_type();
        let is_enum = |t: &Type| {
            let values = finite(self, t);
            !values.is_empty() && values.iter().all(|t| matches!(t, Type::EnumCase(..)))
        };
        if strict != Some(Type::ConstBool(true)) && !is_enum(&needle) && !is_enum(&value) {
            return Some(Verdict::Undecided);
        }
        let needles = finite(self, &needle);
        let values = finite(self, &value);
        let needle_super = is_super_type_of(&needle, &value);
        if !has_constant_array(&haystack) {
            if haystack.is_iterable_at_least_once().yes() {
                return Some(
                    (needles.len() == 1 && values.len() == 1)
                        .then(|| decided(needle_super))
                        .flatten()
                        .into(),
                );
            }
            if !needle_super.no() {
                return Some(Verdict::Undecided);
            }
        }
        if !matches!(&haystack, Type::ConstArray(a) if a.values.is_empty()) {
            let arrays = match &haystack {
                Type::Union(ts) => ts.clone(),
                t => vec![t.clone()],
            };
            if arrays.len() == 1 && matches!(value, Type::Never(_)) {
                return Some(Verdict::Undecided);
            }
            if !needle_super.no() {
                if needles.is_empty() {
                    return Some(Verdict::Undecided);
                }
                for array in &arrays {
                    let Type::ConstArray(a) = array else {
                        return Some(Verdict::Undecided);
                    };
                    let guaranteed: Vec<Type> = a
                        .values
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| !a.optional.contains(i))
                        .filter_map(|(_, v)| <[Type; 1]>::try_from(finite(self, v)).ok())
                        .map(|[v]| v)
                        .collect();
                    if !needles
                        .iter()
                        .all(|n| guaranteed.iter().any(|g| is_super_type_of(g, n).yes()))
                    {
                        return Some(Verdict::Undecided);
                    }
                }
            }
        }
        self.in_array_narrowing(args, types, &needle, &haystack)
    }

    fn in_array_narrowing(
        &self,
        args: &ArgumentList,
        types: CheckTypes,
        needle: &Type,
        haystack: &Type,
    ) -> Option<Verdict> {
        let left = unparen(args.arguments.first()?.value());
        let right = unparen(args.arguments.get(1)?.value());
        let elements = match right {
            Expression::Array(a) => Some(&a.elements),
            Expression::LegacyArray(a) => Some(&a.elements),
            _ => None,
        };
        let Some(elements) = elements.filter(|es| {
            !es.is_empty() && !es.iter().any(|e| matches!(e, ArrayElement::Variadic(_)))
        }) else {
            return Some(
                decided(
                    is_super_type_of(&haystack.iterable_value_type(), needle)
                        .and(haystack.is_iterable_at_least_once()),
                )
                .into(),
            );
        };
        let mut merged: Option<HashMap<String, (Type, Option<Type>, Tri)>> = None;
        for element in elements {
            let item = unparen(element.get_value()?);
            let value = types.argument(self, item)?;
            if elements.len() == 1 {
                return Some(Verdict::from(
                    if finite(self, needle).len() == 1 && *needle == value {
                        Some(true)
                    } else {
                        is_super_type_of(needle, &value).no().then_some(false)
                    },
                ));
            }
            let mut pins = HashMap::new();
            let constant = |e: &Expression| {
                matches!(
                    e,
                    Expression::Literal(Literal::True(_) | Literal::False(_) | Literal::Null(_))
                )
            };
            let fetch = |e: &Expression| constant(e) || matches!(e, Expression::ConstantAccess(_));
            if constant(left) || (matches!(needle, Type::ConstBool(_)) && !fetch(item)) {
                self.in_array_pin(
                    &mut pins,
                    item,
                    &value,
                    needle,
                    matches!(needle, Type::ConstBool(_)),
                )?;
            } else if constant(item) || (matches!(value, Type::ConstBool(_)) && !fetch(left)) {
                self.in_array_pin(
                    &mut pins,
                    left,
                    needle,
                    &value,
                    matches!(value, Type::ConstBool(_)),
                )?;
            } else {
                let left_single = finite(self, needle).len() == 1;
                let right_single = finite(self, &value).len() == 1;
                if left_single || !right_single {
                    self.in_array_pin(&mut pins, item, &value, needle, false)?;
                }
                if right_single || !left_single {
                    self.in_array_pin(&mut pins, left, needle, &value, false)?;
                }
            }
            if let Some(merged) = &mut merged {
                merged.retain(|key, (_, pin, _)| {
                    let Some((_, other, _)) = pins.get(key) else {
                        return false;
                    };
                    *pin = pin
                        .take()
                        .zip(other.clone())
                        .map(|(a, b)| union(vec![a, b]));
                    true
                });
            } else {
                merged = Some(pins);
            }
        }
        Some(
            merged?
                .into_values()
                .filter_map(|(given, pin, defined)| {
                    pin.map(|pin| {
                        if defined.yes() {
                            is_super_type_of(&pin, &given)
                        } else {
                            Tri::Maybe
                        }
                    })
                })
                .reduce(Tri::and)
                .and_then(decided)
                .into(),
        )
    }

    fn in_array_pin(
        &self,
        pins: &mut HashMap<String, (Type, Option<Type>, Tri)>,
        expression: &Expression,
        given: &Type,
        pin: &Type,
        boolean: bool,
    ) -> Option<()> {
        let expression = unparen(expression);
        if let Expression::Assignment(a) = expression {
            if !a.operator.is_assign() {
                return None;
            }
            self.in_array_pin(pins, a.lhs, given, pin, boolean)?;
            return self.in_array_pin(pins, a.rhs, given, pin, false);
        }
        let (own_boolean, defined) = match expression {
            Expression::Variable(Variable::Direct(_)) => (
                true,
                self.variables
                    .reads
                    .get(&expression.span())
                    .map_or(Tri::Maybe, |r| r.1),
            ),
            Expression::Literal(Literal::True(_) | Literal::False(_) | Literal::Null(_))
            | Expression::ConstantAccess(_) => (true, Tri::Yes),
            Expression::Literal(_) => (false, Tri::Yes),
            _ => return None,
        };
        let pin = (!(boolean && own_boolean)).then(|| pin.clone());
        pins.entry(noop::print(self, expression))
            .and_modify(|(_, old, _)| {
                *old = old
                    .take()
                    .zip(pin.clone())
                    .map(|(a, b)| intersect(vec![a, b]));
            })
            .or_insert_with(|| (given.clone(), pin, defined));
        Some(())
    }

    fn object_has_method(&self, class: &str, method: &str) -> (Tri, bool) {
        let Some(k) = self.codebase.get_class_like(class.as_bytes()) else {
            return (Tri::Maybe, false);
        };
        if members::method(self, k, method.as_bytes()).is_some() {
            (Tri::Yes, true)
        } else if members::annotation_method(self, k, method.as_bytes()).is_some() {
            (Tri::Yes, false)
        } else {
            (
                if k.flags.is_final() || k.kind == SymbolKind::Enum {
                    Tri::No
                } else {
                    Tri::Maybe
                },
                false,
            )
        }
    }

    fn method_exists(&self, object: Type, method: Type) -> Option<bool> {
        if let Type::ConstString(class) = &object
            && self.codebase.get_class_like(class).is_none()
        {
            return Some(false);
        }
        let Type::ConstString(method) = method else {
            return None;
        };
        let object = match object {
            Type::ConstString(class) => Type::Object(
                String::from_utf8_lossy(&class).into_owned(),
                Vec::new(),
                false,
            ),
            t => t,
        };
        let found: Vec<_> = object
            .object_class_names()
            .iter()
            .map(|class| self.object_has_method(class, &String::from_utf8_lossy(&method)))
            .collect();
        match extreme(found.iter().map(|f| f.0)) {
            Tri::Yes => found.iter().all(|f| f.1).then_some(true),
            Tri::No => Some(false),
            Tri::Maybe => None,
        }
    }

    fn property_exists(&self, object: Type, property: Type) -> Option<bool> {
        let Type::ConstString(property) = property else {
            return None;
        };
        let property = String::from_utf8_lossy(&property);
        if property.is_empty() {
            return None;
        }
        let objects = match &object {
            Type::Union(ts) => ts.clone(),
            t => vec![t.clone()],
        };
        let dollar = format!("${property}");
        let mut found = Vec::new();
        for object in &objects {
            found.push(match object {
                Type::ObjectShape(properties) => {
                    if properties.iter().any(|(name, _, _)| *name == property) {
                        return None;
                    }
                    Tri::No
                }
                Type::Object(class, _, _) | Type::Static(class, _) | Type::This(class) => {
                    let k = self.codebase.get_class_like(class.as_bytes())?;
                    if self.declared_property(k, dollar.as_bytes()).is_some() {
                        Tri::Yes
                    } else if k
                        .magic_property_ids
                        .contains_key(&mago_word::word(dollar.as_bytes()))
                    {
                        return None;
                    } else {
                        let open = self.allows_dynamic_properties(k)
                            || !(k.flags.is_final() || k.kind == SymbolKind::Enum);
                        if open { Tri::Maybe } else { Tri::No }
                    }
                }
                _ => return None,
            });
        }
        decided(extreme(found))
    }
}

pub(in super::super) fn switch_statement(c: &mut Checker, s: &Switch) {
    if !c.conditions_checked() || !c.on(crate::rules::Switches::SWITCH_CONDITION) {
        return;
    }
    let mut remaining = c.phpstan_type(s.expression).map(void_null);
    if let Some(expanded @ Type::EnumCase(..)) = remaining.clone().map(|t| c.expand_enums(t)) {
        remaining = Some(expanded);
    }
    let cases: Vec<&SwitchCase> = s.body.cases().iter().collect();
    let last = cases.iter().rposition(|case| case.expression().is_some());
    let mut seen: Vec<(Type, String, i64)> = Vec::new();
    let mut group: Vec<(Option<Type>, &Expression)> = Vec::new();
    let mut dead = false;
    for (i, case) in cases.iter().enumerate() {
        let Some(cond) = case.expression() else {
            group.clear();
            continue;
        };
        let value = c.phpstan_type(cond).map(void_null);
        let subject = remaining.clone();
        group.push((value.clone(), cond));
        if case
            .statements()
            .iter()
            .any(|s| finally::always_terminates(c, s))
        {
            let base = remaining.clone();
            for (value, cond) in group.drain(..) {
                remaining = match (
                    remaining,
                    base.as_ref()
                        .map_or(Unequal::Unknown, |b| unequal(c, b, value.as_ref(), cond)),
                ) {
                    (Some(t), Unequal::Removes(v)) => Some(c.remove(t, &v)),
                    (t, Unequal::KeepsAll) => t,
                    _ => None,
                };
            }
        }
        if dead {
            continue;
        }
        let line = c.line_of(cond.span());
        if let Some(value) = value
            .as_ref()
            .filter(|v| single(v) || matches!(v, Type::ConstFloat(_)))
        {
            let printed = noop::print(c, cond);
            if let Some((_, first, at)) = seen
                .iter()
                .find(|(t, ..)| t == value || loose(t, value).yes())
            {
                let message = format!(
                    "Case {printed} in switch is a duplicate of case {first} on line {at}."
                );
                c.report_at_line(line, "switch.duplicateCase", message, None);
                continue;
            }
            seen.push((value.clone(), printed, line));
        }
        let (Some(subject), Some(value)) = (subject, value) else {
            c.no_condition(cond.span(), "switch.alwaysTrue");
            continue;
        };
        let Some(result) = equal(c, &subject, &value) else {
            c.no_condition(cond.span(), "switch.alwaysTrue");
            continue;
        };
        dead |= result;
        if matches!(subject, Type::ConstBool(_)) && c.constant(cond).is_none()
            || result && Some(i) == last
        {
            c.no_condition(cond.span(), "switch.alwaysTrue");
            continue;
        }
        let (s, v) = super::super::declared::with_classes(c, || {
            (subject.describe(Level::Value), value.describe(Level::Value))
        });
        let (identifier, text, tip) = if result {
            ("switch.alwaysTrue", "true", Some(REMOVE_TIP))
        } else {
            ("switch.alwaysFalse", "false", None)
        };
        let message = format!("Switch condition comparison between {s} and {v} is always {text}.");
        c.condition_error(cond.span(), line, identifier, result, message, tip);
    }
}

impl Checker<'_, '_> {
    pub(super) fn is_enum(&self, t: &Type) -> bool {
        match t {
            Type::Union(ts) => ts.iter().all(|t| self.is_enum(t)),
            Type::EnumCase(..) => true,
            Type::Object(class, _, _) | Type::Static(class, _) => self.enum_cases(class).is_some(),
            _ => false,
        }
    }
}

fn equal(c: &Checker, subject: &Type, value: &Type) -> Option<bool> {
    if matches!(subject, Type::Never(_)) || matches!(value, Type::Never(_)) {
        return Some(false);
    }
    if c.is_enum(subject) && *value != Type::ConstBool(true)
        || c.is_enum(value) && *subject != Type::ConstBool(true)
    {
        return identical(&c.expand_enums(subject.clone()), value);
    }
    decided(loose(subject, value))
}

/// What a switch case that did not match tells about the subject in the cases after it.
enum Unequal {
    Unknown,
    KeepsAll,
    Removes(Type),
}

fn unequal(c: &Checker, subject: &Type, value: Option<&Type>, cond: &Expression) -> Unequal {
    if matches!(subject, Type::Mixed { .. }) {
        return Unequal::KeepsAll;
    }
    let Some(value) = value else {
        return Unequal::Unknown;
    };
    if subject.is_boolean().yes() {
        return truthy(value).map_or(Unequal::KeepsAll, |b| Unequal::Removes(Type::ConstBool(b)));
    }
    if scalar(subject)
        && !matches!(
            unparen(cond),
            Expression::Literal(Literal::True(_) | Literal::False(_) | Literal::Null(_))
                | Expression::ConstantAccess(_)
        )
    {
        return Unequal::KeepsAll;
    }
    match value {
        Type::Null | Type::ConstBool(_) => return Unequal::Unknown,
        Type::ConstInt(0) if !is_int(subject) => return Unequal::Unknown,
        Type::ConstString(s) if s.is_empty() => return Unequal::Unknown,
        _ => {}
    }
    let same = subject.is_string().yes() && value.is_string().yes()
        || is_int(subject) && is_int(value)
        || subject.is_float().yes() && value.is_float().yes()
        || c.is_enum(subject) && c.is_enum(value);
    if same && single(value) && c.subtractable(subject) {
        Unequal::Removes(value.clone())
    } else {
        Unequal::KeepsAll
    }
}

fn truthy(t: &Type) -> Option<bool> {
    match t {
        t if scalar(t) => Some(loose_scalars(t, &Type::ConstBool(true))),
        Type::Object(..) | Type::EnumCase(..) | Type::NonexistentParent => Some(true),
        _ => None,
    }
}

pub(super) fn scalar(t: &Type) -> bool {
    matches!(
        t,
        Type::ConstInt(_)
            | Type::ConstString(_)
            | Type::ConstFloat(_)
            | Type::ConstBool(_)
            | Type::Null
    )
}

fn numeric(s: &str) -> bool {
    let s = s.trim_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    let (mantissa, exponent) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = match mantissa.split_once('.') {
        Some((int, frac)) => digits(int) && digits(frac) && !(int.is_empty() && frac.is_empty()),
        None => !mantissa.is_empty() && digits(mantissa),
    };
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    mantissa_ok && exponent_ok
}

fn non_numeric_string(t: &Type) -> bool {
    match t {
        Type::ConstString(s) => !std::str::from_utf8(s).is_ok_and(numeric),
        Type::Union(ts) => ts.iter().all(non_numeric_string),
        _ => false,
    }
}

fn loose_scalars(a: &Type, b: &Type) -> bool {
    let truthy = |t: &Type| match t {
        Type::ConstInt(i) => *i != 0,
        Type::ConstFloat(f) => *f != 0.0,
        Type::ConstString(s) => !s.is_empty() && s.as_slice() != b"0",
        Type::ConstBool(b) => *b,
        _ => false,
    };
    let number = |t: &Type| match t {
        Type::ConstInt(i) => Some(*i as f64),
        Type::ConstFloat(f) => Some(*f),
        Type::ConstString(s) if std::str::from_utf8(s).is_ok_and(numeric) => std::str::from_utf8(s)
            .ok()?
            .trim_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c'])
            .parse()
            .ok(),
        _ => None,
    };
    let string = |t: &Type| match t {
        Type::ConstString(s) => s.clone(),
        Type::ConstInt(i) => i.to_string().into_bytes(),
        t => float_to_string(number(t).unwrap_or_default()).into_bytes(),
    };
    match (a, b) {
        (Type::Null, Type::Null) => true,
        (Type::ConstBool(_), _) | (_, Type::ConstBool(_)) => truthy(a) == truthy(b),
        (Type::Null, Type::ConstString(s)) | (Type::ConstString(s), Type::Null) => s.is_empty(),
        (Type::Null, t) | (t, Type::Null) => !truthy(t),
        (Type::ConstString(x), Type::ConstString(y))
            if !(std::str::from_utf8(x).is_ok_and(numeric)
                && std::str::from_utf8(y).is_ok_and(numeric)) =>
        {
            x == y
        }
        _ => match (number(a), number(b)) {
            (Some(x), Some(y)) => x == y,
            _ => string(a) == string(b),
        },
    }
}

pub(super) fn loose(a: &Type, b: &Type) -> Tri {
    let compound = |t: &Type| {
        matches!(
            t,
            Type::Union(_)
                | Type::Benevolent(_)
                | Type::Intersection(_)
                | Type::IntRange(..)
                | Type::Mixed { .. }
                | Type::Never(_)
        )
    };
    let empty_array = |t: &Type| matches!(t, Type::ConstArray(a) if a.keys.is_empty());
    let object = |t: &Type| {
        matches!(
            t,
            Type::Object(..)
                | Type::Static(..)
                | Type::This(_)
                | Type::EnumCase(..)
                | Type::Closure(_)
        )
    };
    match a {
        Type::Union(ts) | Type::Benevolent(ts) => extreme(ts.iter().map(|t| loose(t, b))),
        Type::Intersection(ts) => {
            let results: Vec<Tri> = ts.iter().map(|t| loose(t, b)).collect();
            if results.iter().any(|r| r.yes()) {
                Tri::Yes
            } else if results.iter().any(|r| r.no()) {
                Tri::No
            } else {
                Tri::Maybe
            }
        }
        a if scalar(a) && scalar(b) => Tri::from(loose_scalars(a, b)),
        a if scalar(a) && empty_array(b) => {
            Tri::from(matches!(a, Type::Null | Type::ConstBool(false)))
        }
        a if scalar(a) && compound(b) => loose(b, a),
        Type::ConstBool(x) if object(b) => Tri::from(*x),
        Type::IntRange(min, max) => {
            if min.is_some_and(|m| m > 0) || max.is_some_and(|m| m < 0) {
                match b {
                    Type::ConstBool(true) => return Tri::Yes,
                    Type::ConstBool(false) => return Tri::No,
                    _ => {}
                }
            }
            if let Type::ConstInt(n) = b
                && (max.is_some_and(|m| m < *n) || min.is_some_and(|m| m > *n))
            {
                return Tri::No;
            }
            int_loose(b)
        }
        Type::Int | Type::ConstInt(_) => int_loose(b),
        Type::String | Type::ConstString(_) | Type::ClassString(_) => {
            if b.is_array().yes() {
                Tri::No
            } else {
                Tri::Maybe
            }
        }
        Type::Array(..) => {
            if is_int(b) {
                Tri::No
            } else {
                Tri::Maybe
            }
        }
        Type::ConstArray(arr) => {
            if is_int(b) {
                return Tri::No;
            }
            if arr.keys.is_empty() {
                if b.is_iterable_at_least_once().yes() {
                    return Tri::No;
                }
                let values: Vec<&Type> = match b {
                    Type::Union(ts) if ts.iter().all(scalar) => ts.iter().collect(),
                    b if scalar(b) => vec![b],
                    _ => vec![],
                };
                if !values.is_empty() {
                    return extreme(
                        values
                            .into_iter()
                            .map(|v| Tri::from(matches!(v, Type::Null | Type::ConstBool(false)))),
                    );
                }
            }
            Tri::Maybe
        }
        Type::Object(..) | Type::Static(..) | Type::This(_) | Type::EnumCase(..) => match b {
            Type::ConstBool(x) => Tri::from(*x),
            _ => Tri::Maybe,
        },
        Type::Accessory(accessory) => {
            let no = match accessory {
                Accessory::NonEmptyString => {
                    *b == Type::Null || matches!(b, Type::ConstString(s) if s.is_empty())
                }
                Accessory::NonFalsyString => {
                    let falsy = |t: &Type| {
                        matches!(t, Type::Null | Type::ConstBool(false))
                            || matches!(t, Type::ConstString(s) if s.is_empty())
                            || empty_array(t)
                    };
                    match b {
                        Type::Union(ts) => ts.iter().all(falsy),
                        b => falsy(b),
                    }
                }
                Accessory::NumericString | Accessory::DecimalIntString { inverse: false } => {
                    *b == Type::Null || non_numeric_string(b)
                }
                Accessory::LowercaseString => {
                    matches!(b, Type::ConstString(s) if s.to_ascii_lowercase() != *s && !std::str::from_utf8(s).is_ok_and(numeric))
                }
                Accessory::UppercaseString => {
                    matches!(b, Type::ConstString(s) if s.to_ascii_uppercase() != *s && !std::str::from_utf8(s).is_ok_and(numeric))
                }
                Accessory::NonEmptyArray => empty_array(b),
                _ => false,
            };
            if no { Tri::No } else { Tri::Maybe }
        }
        _ => Tri::Maybe,
    }
}

fn int_loose(b: &Type) -> Tri {
    if b.is_array().yes() || non_numeric_string(b) {
        Tri::No
    } else {
        Tri::Maybe
    }
}

fn is_int(t: &Type) -> bool {
    match t {
        Type::Union(ts) => ts.iter().all(is_int),
        t => matches!(t, Type::Int | Type::ConstInt(_) | Type::IntRange(..)),
    }
}

pub(in super::super) fn match_expression(c: &mut Checker, m: &Match) {
    if !c.conditions_checked() {
        return;
    }
    let Some(mut remaining) = c
        .phpstan_type(m.expression)
        .map(void_null)
        .filter(|t| c.subtractable(t))
    else {
        return;
    };
    if let expanded @ Type::EnumCase(..) = c.expand_enums(remaining.clone()) {
        remaining = expanded;
    }
    let constant_subject = matches!(remaining, Type::ConstBool(_));
    let last_arm = m.arms.len().saturating_sub(1);
    let mut dead = false;
    let mut default = false;
    for (i, arm) in m.arms.iter().enumerate() {
        if dead {
            break;
        }
        let MatchArm::Expression(arm) = arm else {
            default = true;
            continue;
        };
        for cond in &arm.conditions {
            let Some(value) = c.phpstan_type(cond).map(void_null) else {
                continue;
            };
            let subject = remaining.clone();
            if single(&value) {
                remaining = c.remove(remaining, &value);
            }
            let Some(result) = identical(&subject, &value) else {
                c.no_condition(cond.span(), "match.alwaysTrue");
                continue;
            };
            dead |= result;
            if constant_subject && c.constant(cond).is_none()
                || c.deferred(cond)
                || result && i == last_arm
            {
                c.no_condition(cond.span(), "match.alwaysTrue");
                continue;
            }
            let (mut s, v) = super::super::declared::with_classes(c, || {
                (subject.describe(Level::Value), value.describe(Level::Value))
            });
            if let (Expression::Variable(Variable::Direct(this)), Type::EnumCase(class, _)) =
                (unparen(m.expression), &subject)
                && this.name == b"$this"
            {
                s = format!("$this({class})&{s}");
            }
            let (text, tip) = if result {
                ("true", Some(REMOVE_TIP))
            } else {
                ("false", None)
            };
            let line = c.line_of(cond.span());
            c.condition_error(
                cond.span(),
                line,
                if result {
                    "match.alwaysTrue"
                } else {
                    "match.alwaysFalse"
                },
                result,
                format!("Match arm comparison between {s} and {v} is always {text}."),
                tip,
            );
        }
    }
    if default || dead {
        return;
    }
    let remaining = c.expand_enums(remaining);
    let caught = c.conditions.caught.get(&m.span()).is_some_and(|names| {
        names.iter().any(|n| {
            let name = c.names.get(n);
            ["UnhandledMatchError", "Error", "Throwable"]
                .iter()
                .any(|s| name.eq_ignore_ascii_case(s.as_bytes()))
        })
    });
    if matches!(remaining, Type::Never(_)) || caught || c.conditions.throws.contains(&m.span()) {
        return;
    }
    let values = if matches!(remaining, Type::Union(_)) {
        "values"
    } else {
        "value"
    };
    let described = super::super::declared::with_classes(c, || remaining.describe(Level::Value));
    c.report(
        m.span(),
        "match.unhandled",
        format!("Match expression does not handle remaining {values}: {described}"),
        None,
    );
}

const IN_TRAIT: &str = "\u{1e}in-trait\u{1f}";

impl Checker<'_, '_> {
    pub(super) fn condition_error(
        &mut self,
        key: Span,
        line: i64,
        identifier: &str,
        value: bool,
        message: String,
        tip: Option<&str>,
    ) {
        let message = match self.in_trait() {
            Some(t) => format!(
                "{IN_TRAIT}{}\u{1f}{t}\u{1f}{}\u{1f}{value}\u{1f}{message}",
                rule(identifier),
                key.start.offset
            ),
            None => message,
        };
        self.report_at_line(line, identifier, message, tip);
    }

    pub(super) fn no_condition(&mut self, key: Span, identifier: &str) {
        if let Some(t) = self.in_trait() {
            let message = format!(
                "{IN_TRAIT}{}\u{1f}{t}\u{1f}{}\u{1f}-\u{1f}",
                rule(identifier),
                key.start.offset
            );
            let line = self.line_of(key);
            self.report_at_line(line, identifier, message, None);
        }
    }
}

fn rule(identifier: &str) -> &str {
    identifier.split('.').next().unwrap_or_default()
}

pub(crate) fn conditions_in_traits(errors: Vec<Error>) -> Vec<Error> {
    let (collected, mut errors): (Vec<Error>, Vec<Error>) = errors
        .into_iter()
        .partition(|e| e.message.starts_with(IN_TRAIT));
    let mut groups: BTreeMap<(String, String, String), BTreeMap<String, Vec<(String, Error)>>> =
        BTreeMap::new();
    for mut e in collected {
        let fields: Vec<String> = e.message[IN_TRAIT.len()..]
            .splitn(5, '\u{1f}')
            .map(str::to_string)
            .collect();
        let [rule, t, key, value, message] =
            <[String; 5]>::try_from(fields).expect("an entry has five fields");
        let context = groups
            .entry((rule, t, key))
            .or_default()
            .entry(e.file.clone())
            .or_default();
        if value != "-" {
            e.message = message;
            context.push((value, e));
        }
    }
    for contexts in groups.into_values() {
        let values: BTreeSet<&str> = contexts
            .values()
            .map(|es| es.first().map_or("-", |(v, _)| v.as_str()))
            .collect();
        if values.len() > 1 {
            continue;
        }
        let mut unique: Vec<Error> = Vec::new();
        for (_, e) in contexts.into_values().flatten() {
            if !unique.iter().any(|u| u.message == e.message) {
                unique.push(e);
            }
        }
        if let [one] = unique.as_mut_slice() {
            let file = one.trait_file_path.take().unwrap_or_default();
            one.file = file.clone();
            one.file_path = file;
        }
        errors.extend(unique);
    }
    errors
}

pub(in super::super) fn match_subject_types(
    run: &super::super::infer::Run<'_, '_>,
) -> (HashMap<Span, TUnion>, Vec<Span>) {
    let (file, program) = (run.file, run.program);
    // The subject's type is read from a copy whose arms are `default => 1`, so arm conditions don't narrow it. A
    // rewritten match yields `1` to the code after it, so one copy rewrites at most one match of each depth in
    // each function-like body; a later match's subject still sees an earlier match's real result (#38 item 3).
    struct Matches {
        found: Vec<(Span, usize, usize, usize, Option<Span>)>,
        depth: usize,
        bodies: Vec<Span>,
    }
    impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for Matches {
        fn walk_in_match(&mut self, m: &'ast Match<'arena>, (): &mut ()) {
            if !m.arms.is_empty() {
                let (start, end) = (
                    m.left_brace.end.offset as usize,
                    m.right_brace.start.offset as usize,
                );
                let body = self.bodies.last().copied();
                self.found
                    .push((m.expression.span(), start, end, self.depth, body));
            }
            self.depth += 1;
        }
        fn walk_out_match(&mut self, _: &'ast Match<'arena>, (): &mut ()) {
            self.depth -= 1;
        }
        fn walk_in_function(&mut self, f: &'ast Function<'arena>, (): &mut ()) {
            self.bodies.push(f.span());
        }
        fn walk_out_function(&mut self, _: &'ast Function<'arena>, (): &mut ()) {
            self.bodies.pop();
        }
        fn walk_in_method(&mut self, m: &'ast Method<'arena>, (): &mut ()) {
            self.bodies.push(m.span());
        }
        fn walk_out_method(&mut self, _: &'ast Method<'arena>, (): &mut ()) {
            self.bodies.pop();
        }
        fn walk_in_closure(&mut self, c: &'ast Closure<'arena>, (): &mut ()) {
            self.bodies.push(c.span());
        }
        fn walk_out_closure(&mut self, _: &'ast Closure<'arena>, (): &mut ()) {
            self.bodies.pop();
        }
        fn walk_in_arrow_function(&mut self, f: &'ast ArrowFunction<'arena>, (): &mut ()) {
            self.bodies.push(f.span());
        }
        fn walk_out_arrow_function(&mut self, _: &'ast ArrowFunction<'arena>, (): &mut ()) {
            self.bodies.pop();
        }
    }
    let mut matches = Matches {
        found: Vec::new(),
        depth: 0,
        bodies: Vec::new(),
    };
    matches.walk_program(program, &mut ());
    let mut types = HashMap::new();
    let mut unknown = Vec::new();
    if matches.found.is_empty() {
        return (types, unknown);
    }
    let mut seen: HashMap<(usize, Option<Span>), usize> = HashMap::new();
    let passes: Vec<(usize, usize)> = matches
        .found
        .iter()
        .map(|&(_, _, _, depth, body)| {
            let index = seen.entry((depth, body)).or_default();
            *index += 1;
            (depth, *index - 1)
        })
        .collect();
    let mut order: Vec<(usize, usize)> = passes.clone();
    order.sort_unstable();
    order.dedup();
    for pass in order {
        let mut contents = file.contents.to_vec();
        let mut subjects = Vec::new();
        for (&(subject, s, e, ..), _) in matches
            .found
            .iter()
            .zip(&passes)
            .filter(|(_, p)| **p == pass)
        {
            let mut arms = file.contents[s..e]
                .iter()
                .map(|b| if *b == b'\n' { b'\n' } else { b' ' })
                .collect::<Vec<_>>();
            let mut at = 0;
            let placed = [b"default".as_slice(), b"=>", b"1"].iter().all(|token| {
                while at + token.len() <= arms.len() && arms[at..at + token.len()].contains(&b'\n')
                {
                    at += 1;
                }
                let room = at + token.len() <= arms.len();
                if room {
                    arms[at..at + token.len()].copy_from_slice(token);
                    at += token.len();
                }
                room
            });
            if placed {
                contents[s..e].copy_from_slice(&arms);
                subjects.push(subject);
            } else {
                unknown.push(subject);
            }
        }
        let copy = File::new(
            file.name.clone(),
            file.file_type,
            file.path.clone(),
            Cow::Owned(contents),
        );
        let arena = LocalArena::new();
        let (program, names) = parse(&arena, &copy);
        let comments = super::super::Comments::of(&copy.contents, true);
        let copy_run = super::super::infer::Run {
            arena: &arena,
            file: &copy,
            program,
            names: &names,
            codebase: run.codebase,
            php_version: run.php_version,
            switches: run.switches,
            comments: &comments,
            functions: run.functions,
        };
        types.extend(super::super::infer::analyse(&copy_run, subjects, Vec::new()).types);
    }
    (types, unknown)
}

#[cfg(test)]
mod tests {
    use super::{Accessory, Tri, Type, loose};

    #[test]
    fn numeric_strings() {
        for accessory in [
            Accessory::NumericString,
            Accessory::DecimalIntString { inverse: false },
        ] {
            let t = Type::Intersection(vec![Type::String, Type::Accessory(accessory)]);
            assert_eq!(loose(&t, &Type::ConstString("abc".into())), Tri::No);
            assert_eq!(loose(&t, &Type::Null), Tri::No);
            assert_eq!(loose(&t, &Type::ConstString("1".into())), Tri::Maybe);
        }
    }

    #[test]
    fn int_and_non_numeric_strings() {
        let strings = Type::Union(vec![
            Type::ConstString("a".into()),
            Type::ConstString("b".into()),
        ]);
        assert_eq!(loose(&Type::Int, &strings), Tri::No);
    }
}
