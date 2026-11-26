//! Rector 2.8.4's `ChangeSwitchToMatchRector`, with `SwitchExprsResolver`, `MatchSwitchAnalyzer`, `SwitchAnalyzer`,
//! `MatchFactory` and `MatchArmsFactory`.

use mago_syntax::cst::{
    BinaryOperator, Call, Construct, Expression, Node, NodeKind, Statement, Switch, SwitchCase,
    UnaryPrefixOperator,
};
use pun_phpstan::types::Type;

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::conditions::{comments_between, indentation, print_comments, reprint};
use crate::rules::support::expressions::{equal, is_constant, prec, wrap};
use crate::rules::support::phpdoc_writer::type_mapper::uniquate_types;
use crate::rules::support::statement_edits::{HOLDERS, assign, expression, holder_list};
use crate::rules::support::statements::{Token, tokens};
use crate::rules::types::compare::mapper::expr_type;
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferMatch;

impl Rule for PreferMatch {
    fn id(&self) -> &'static str {
        "lint/php-8.0/prefer-match"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(HOLDERS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let raw = crate::syntax::statements(node);
        if !raw.iter().any(|s| matches!(s, Statement::Switch(_))) {
            return;
        }
        let Some(list) = holder_list(ctx.source, node, ancestors) else {
            return;
        };
        // possible reference override, where match
        if is_inside_by_ref_function_like(node, ancestors) {
            return;
        }
        let facts = facts(ctx);
        for (key, stmt) in list.stmts.iter().enumerate() {
            let Statement::Switch(switch) = stmt else {
                continue;
            };
            let next = list.stmts.get(key + 1).copied();
            if let Some(report) = refactor(ctx, &facts, ancestors, switch, next) {
                out.push(report);
            }
        }
    }
}

fn is_inside_by_ref_function_like(node: Node<'_, '_>, ancestors: &[Node<'_, '_>]) -> bool {
    std::iter::once(&node)
        .chain(ancestors.iter().rev())
        .find_map(|n| match n {
            Node::Function(f) => Some(f.ampersand.is_some()),
            Node::Method(m) => Some(m.ampersand.is_some()),
            Node::Closure(c) => Some(c.ampersand.is_some()),
            Node::ArrowFunction(a) => Some(a.ampersand.is_some()),
            Node::PropertyHook(h) => Some(h.ampersand.is_some()),
            Node::Class(_)
            | Node::Interface(_)
            | Node::Trait(_)
            | Node::Enum(_)
            | Node::AnonymousClass(_) => Some(false),
            _ => None,
        })
        .unwrap_or(false)
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Normal,
    Assign,
    Return,
    Throw,
}

struct CondAndExpr<'a> {
    /// `null` for the default arm (`getCondExprs()` also maps an empty list to `null`).
    conds: Option<Vec<&'a Expression<'a>>>,
    expr: &'a Expression<'a>,
    kind: Kind,
    comments: Vec<Span>,
}

fn refactor(
    ctx: &Context<'_>,
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    switch: &Switch<'_>,
    next: Option<&Statement<'_>>,
) -> Option<Report> {
    let source = ctx.source;
    let tokens = tokens(ctx);
    let mut cond_and_exprs = resolve(tokens, switch)?;
    if should_skip_switch(ctx, facts, ancestors, switch, &cond_and_exprs, next) {
        return None;
    }
    if !have_cond_and_exprs_match_potential(ctx, &cond_and_exprs) {
        return None;
    }
    let is_return = cond_and_exprs.iter().any(|c| c.kind == Kind::Return);
    if matches!(
        expr_type(facts, ancestors, switch.expression, false),
        Type::Object(..) | Type::EnumCase(..)
    ) {
        return None;
    }
    let remove_next = create_from_cond_and_exprs(ctx, tokens, switch, &mut cond_and_exprs, next)?;
    let assign_var = resolve_assign_var(&cond_and_exprs);
    // `hasDefaultValue()`
    if !cond_and_exprs.iter().any(|c| c.conds.is_none()) {
        return None;
    }
    let cond = unparenthesize(switch.expression);
    // Rector 2.9.2's `isStringCondWithIntArm()`: an `(int)` cast would turn the loose `"777abc" == 777` (false)
    // into `777 === 777` (true).
    if expr_type(facts, ancestors, cond, true).is_string().yes()
        && (cond_and_exprs.iter().filter_map(|c| c.conds.as_ref()))
            .flatten()
            .any(|arm| expr_type(facts, ancestors, arm, true).is_integer().yes())
    {
        return None;
    }
    let span = node_span(switch);
    let base = indentation(source, span.start);
    let cond_code = match cast_match_cond(facts, ancestors, cond, &cond_and_exprs) {
        _ if mirror_dynamic_bool_expr(facts, ancestors, cond, &cond_and_exprs) => {
            "true".to_string()
        }
        Some(cast) => format!(
            "{cast} {}",
            wrap(&print(source, tokens, cond, base), prec(cond), 10, None)
        ),
        None => print(source, tokens, cond, base),
    };
    let r#match = print_match(source, tokens, base, &cond_code, &cond_and_exprs);
    let code = match assign_var {
        Some(var) => format!("{} = {match};", print(source, tokens, var, base)),
        None if is_return => format!("return {match};"),
        None => format!("{match};"),
    };
    let end = match next {
        Some(next) if remove_next && is_return => node_span(next).end,
        _ => span.end,
    };
    Some(Report::fixed(
        node_span(&switch.switch),
        "Use `match` instead of `switch`.",
        Safety::Unsafe,
        vec![Edit::new(Span::new(span.start, end), code)],
    ))
}

fn statements<'a>(case: &'a SwitchCase<'a>) -> Vec<&'a Statement<'a>> {
    let statements = match case {
        SwitchCase::Expression(case) => &case.statements,
        SwitchCase::Default(case) => &case.statements,
    };
    // PHP-Parser keeps no node for an empty `;` statement
    (statements.iter())
        .filter(|s| !matches!(s, Statement::Noop(_)))
        .collect()
}

fn case_cond<'a>(case: &'a SwitchCase<'a>) -> Option<&'a Expression<'a>> {
    match case {
        SwitchCase::Expression(case) => Some(unparenthesize(case.expression)),
        SwitchCase::Default(_) => None,
    }
}

fn separator_end(case: &SwitchCase<'_>) -> usize {
    use mago_span::HasSpan;
    let separator = match case {
        SwitchCase::Expression(case) => &case.separator,
        SwitchCase::Default(case) => &case.separator,
    };
    separator.span().end.offset as usize
}

/// `SwitchExprsResolver::resolve()`.
fn resolve<'a>(tokens: &[Token], switch: &'a Switch<'a>) -> Option<Vec<CondAndExpr<'a>>> {
    let cases = switch.body.cases();
    if !cases.iter().all(is_valid_case) {
        return None;
    }
    let mut ordered: Vec<(usize, &SwitchCase<'_>)> = cases.iter().enumerate().collect();
    move_default_case_to_last(&mut ordered);
    let mut collection_empty_cases_cond = Vec::new();
    for &(key, case) in &ordered {
        if !statements(case).is_empty() {
            continue;
        }
        collection_empty_cases_cond.push((key, case_cond(case)?));
    }
    let mut cond_and_exprs = Vec::new();
    for &(key, case) in &ordered {
        let stmts = statements(case);
        let Some(&first) = stmts.first() else {
            continue;
        };
        let comments = comments_between(tokens, separator_end(case), node_span(first).start);
        let conds = case_cond(case).map(|cond| {
            let taken = collection_empty_cases_cond
                .iter()
                .take_while(|(i, _)| *i <= key)
                .count();
            let mut conds: Vec<_> = (collection_empty_cases_cond.drain(..taken))
                .map(|(_, c)| c)
                .collect();
            conds.push(cond);
            conds
        });
        let (expr, kind) = match (first, expression(first)) {
            (_, Some(expr @ Expression::Throw(_))) => (expr, Kind::Throw),
            (Statement::Return(r#return), _) => (unparenthesize(r#return.value?), Kind::Return),
            (_, Some(expr)) if assign(expr).is_some() => (expr, Kind::Assign),
            (_, Some(expr)) => (expr, Kind::Normal),
            _ => return None,
        };
        cond_and_exprs.push(CondAndExpr {
            conds,
            expr,
            kind,
            comments,
        });
    }
    Some(cond_and_exprs)
}

fn move_default_case_to_last(cases: &mut Vec<(usize, &SwitchCase<'_>)>) {
    let Some(key) = cases.iter().position(|(_, c)| c.is_default()) else {
        return;
    };
    // not has next? default is at the end, no need move
    if key + 1 == cases.len() {
        return;
    }
    // current default has no stmt? keep as is as rely to next case
    if statements(cases[key].1).is_empty() {
        return;
    }
    let mut from = key;
    while from > 0 && statements(cases[from - 1].1).is_empty() {
        from -= 1;
    }
    let total = cases.len();
    let default = cases[key].1;
    cases.drain(from..=key);
    cases.push((total, default));
}

fn is_valid_case(case: &SwitchCase<'_>) -> bool {
    let stmts = statements(case);
    match stmts.as_slice() {
        // prepend to previous one
        [] | [_, Statement::Break(_)] => true,
        // instant return
        [Statement::Return(_)] => true,
        // default value
        [_] => case.is_default(),
        _ => false,
    }
}

/// `MatchSwitchAnalyzer::shouldSkipSwitch()`.
fn should_skip_switch(
    ctx: &Context<'_>,
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    switch: &Switch<'_>,
    cond_and_exprs: &[CondAndExpr<'_>],
    next: Option<&Statement<'_>>,
) -> bool {
    if cond_and_exprs.is_empty() {
        return true;
    }
    let cases = switch.body.cases();
    if !has_each_case_break(cases) {
        return true;
    }
    if has_different_type_cases(facts, ancestors, cases, switch.expression) {
        return true;
    }
    if !has_each_case_single_stmt(cases) {
        return false;
    }
    if has_default_single_stmt(cases) {
        return false;
    }
    // is followed by return? is considered implicit default
    if is_next_stmt_return_with_expr(ctx, cases, next) {
        return false;
    }
    !next
        .and_then(expression)
        .is_some_and(|e| matches!(e, Expression::Throw(_)))
}

fn has_each_case_break(cases: &[SwitchCase<'_>]) -> bool {
    let is_return = |s: &&Statement<'_>| matches!(s, Statement::Return(_));
    if cases.len() == 1 {
        return statements(&cases[0]).iter().any(is_return);
    }
    cases[..cases.len() - 1].iter().all(|case| {
        let stmts = statements(case);
        stmts.is_empty()
            || stmts
                .iter()
                .any(|s| matches!(s, Statement::Break(_) | Statement::Return(_)))
    })
}

fn without_break<'a>(case: &'a SwitchCase<'a>) -> usize {
    (statements(case).iter())
        .filter(|s| !matches!(s, Statement::Break(_)))
        .count()
}

fn has_each_case_single_stmt(cases: &[SwitchCase<'_>]) -> bool {
    (cases.iter())
        .filter(|c| !c.is_default())
        .all(|c| without_break(c) == 1)
}

fn has_default_single_stmt(cases: &[SwitchCase<'_>]) -> bool {
    cases
        .iter()
        .find(|c| c.is_default())
        .is_some_and(|c| without_break(c) == 1)
}

/// `SwitchAnalyzer::hasDifferentTypeCases()`.
fn has_different_type_cases(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    cases: &[SwitchCase<'_>],
    cond: &Expression<'_>,
) -> bool {
    let types: Vec<Type> = (cases.iter())
        .filter_map(case_cond)
        .map(|c| expr_type(facts, ancestors, c, false))
        .collect();
    if types.is_empty() {
        return false;
    }
    let unique = uniquate_types(&types);
    if unique.len() == 1 && unique[0].is_integer().yes() {
        let cond = expr_type(facts, ancestors, cond, false);
        if !is_mixed(&cond) && cond.is_string().maybe() {
            return true;
        }
    }
    unique.len() > 1
}

fn is_mixed(t: &Type) -> bool {
    matches!(
        t,
        Type::Mixed { .. } | Type::SubtractedMixed { .. } | Type::Error
    )
}

fn is_next_stmt_return_with_expr(
    ctx: &Context<'_>,
    cases: &[SwitchCase<'_>],
    next: Option<&Statement<'_>>,
) -> bool {
    let Some(Statement::Return(r#return)) = next else {
        return false;
    };
    let Some(returned) = r#return.value else {
        return false;
    };
    cases.iter().all(|case| {
        (statements(case).iter())
            .filter_map(|s| expression(s).and_then(assign))
            .all(|a| equal(ctx, a.lhs, returned))
    })
}

/// `MatchSwitchAnalyzer::haveCondAndExprsMatchPotential()`.
fn have_cond_and_exprs_match_potential(
    ctx: &Context<'_>,
    cond_and_exprs: &[CondAndExpr<'_>],
) -> bool {
    let mut kinds = (cond_and_exprs.iter())
        .filter(|c| c.kind != Kind::Throw)
        .map(|c| c.kind);
    if let Some(first) = kinds.next()
        && kinds.any(|k| k != first)
    {
        return false;
    }
    let vars: Vec<&Expression<'_>> = (cond_and_exprs.iter())
        .filter_map(|c| assign(c.expr))
        .map(|a| unparenthesize(a.lhs))
        .collect();
    // Rector keys a target by its node class and name; Pun compares the code, so properties of different objects
    // stay apart
    vars.iter().all(|v| equal(ctx, vars[0], v))
}

/// `MatchFactory::createFromCondAndExprs()`: adds the default arm a following `throw` or `return` gives, and says
/// whether that statement goes; `None` when the switch can't become a match.
fn create_from_cond_and_exprs<'a>(
    ctx: &Context<'_>,
    tokens: &[Token],
    switch: &Switch<'_>,
    cond_and_exprs: &mut Vec<CondAndExpr<'a>>,
    next: Option<&'a Statement<'a>>,
) -> Option<bool> {
    let mut remove_next = false;
    if cond_and_exprs.iter().any(|c| c.conds.is_none()) {
        return Some(false);
    }
    let Some(next) = next else {
        return Some(false);
    };
    // 1. is followed by throws stmts?
    if let Some(throw @ Expression::Throw(_)) = expression(next) {
        cond_and_exprs.push(CondAndExpr {
            conds: None,
            expr: throw,
            kind: Kind::Return,
            comments: Vec::new(),
        });
        remove_next = true;
    }
    // 2. is followed by return expr
    if let Statement::Return(r#return) = next
        && let Some(returned) = r#return.value
    {
        let var = resolve_assign_var(cond_and_exprs);
        if let Some(var) = var
            && (matches!(var, Expression::ArrayAccess(_) | Expression::ArrayAppend(_))
                || !equal(ctx, returned, var))
        {
            return None;
        }
        remove_next = var.is_none();
        cond_and_exprs.push(CondAndExpr {
            conds: None,
            expr: unparenthesize(returned),
            kind: Kind::Return,
            comments: comments_between(tokens, node_span(switch).end, node_span(next).start),
        });
    }
    Some(remove_next)
}

fn resolve_assign_var<'a>(cond_and_exprs: &[CondAndExpr<'a>]) -> Option<&'a Expression<'a>> {
    (cond_and_exprs.iter())
        .find_map(|c| assign(c.expr))
        .map(|a| unparenthesize(a.lhs))
}

/// `castMatchCond()`: `(string)` when the match subject's native type is `int` and every arm condition is a
/// `string`.
fn cast_match_cond(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    cond: &Expression<'_>,
    cond_and_exprs: &[CondAndExpr<'_>],
) -> Option<&'static str> {
    if !expr_type(facts, ancestors, cond, true).is_integer().yes() {
        return None;
    }
    let mut new_match_cond = None;
    for conds in cond_and_exprs.iter().filter_map(|c| c.conds.as_ref()) {
        for arm_cond in conds {
            if expr_type(facts, ancestors, arm_cond, true)
                .is_string()
                .yes()
            {
                new_match_cond = Some("(string)");
            } else {
                new_match_cond = None;
                break;
            }
        }
    }
    new_match_cond
}

/// `mirrorDynamicBoolExpr()`: whether the subject becomes `true` because every arm condition is a boolean expression.
fn mirror_dynamic_bool_expr(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    cond: &Expression<'_>,
    cond_and_exprs: &[CondAndExpr<'_>],
) -> bool {
    // switch(true) already just use
    // switch(false) is dead code that can be on purpose
    if is_constant(cond, "true") || is_constant(cond, "false") {
        return false;
    }
    let mut is_changed = false;
    for conds in cond_and_exprs.iter().filter_map(|c| c.conds.as_ref()) {
        for cond in conds {
            if is_bool_expr(cond) || is_call_like_return_native_bool(facts, ancestors, cond) {
                is_changed = true;
                continue;
            }
            return false;
        }
    }
    is_changed
}

/// `ExprAnalyzer::isBoolExpr()`.
fn is_bool_expr(expr: &Expression<'_>) -> bool {
    match unparenthesize(expr) {
        Expression::UnaryPrefix(u) => matches!(
            u.operator,
            UnaryPrefixOperator::Not(_)
                | UnaryPrefixOperator::BoolCast(..)
                | UnaryPrefixOperator::BooleanCast(..)
        ),
        Expression::Construct(Construct::Empty(_) | Construct::Isset(_)) => true,
        Expression::Binary(b) => matches!(
            b.operator,
            BinaryOperator::Instanceof(_)
                | BinaryOperator::Equal(_)
                | BinaryOperator::NotEqual(_)
                | BinaryOperator::AngledNotEqual(_)
                | BinaryOperator::Identical(_)
                | BinaryOperator::NotIdentical(_)
                | BinaryOperator::GreaterThan(_)
                | BinaryOperator::GreaterThanOrEqual(_)
                | BinaryOperator::LessThan(_)
                | BinaryOperator::LessThanOrEqual(_)
                | BinaryOperator::And(_)
                | BinaryOperator::Or(_)
                | BinaryOperator::LowAnd(_)
                | BinaryOperator::LowOr(_)
                | BinaryOperator::LowXor(_)
        ),
        _ => false,
    }
}

fn is_call_like_return_native_bool(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> bool {
    let expr = unparenthesize(expr);
    matches!(
        expr,
        Expression::Call(
            Call::Function(_) | Call::Method(_) | Call::NullSafeMethod(_) | Call::StaticMethod(_)
        ) | Expression::Instantiation(_)
    ) && expr_type(facts, ancestors, expr, true).is_boolean().yes()
}

/// An original node as PHP-Parser's format-preserving printer writes it at indentation `level`.
fn print(source: &str, tokens: &[Token], expr: &Expression<'_>, level: usize) -> String {
    let span = node_span(unparenthesize(expr));
    let adjust = level as isize - indentation(source, span.start) as isize;
    reprint(source, tokens, span, adjust)
}

/// `pExpr_Match()` with `pCommaSeparatedMultiline()`: one arm per line, each after its comments.
fn print_match(
    source: &str,
    tokens: &[Token],
    base: usize,
    cond: &str,
    cond_and_exprs: &[CondAndExpr<'_>],
) -> String {
    let level = base + 4;
    let nl = format!("\n{}", " ".repeat(level));
    let mut out = format!("match ({cond}) {{");
    for arm in cond_and_exprs {
        out.push_str(&nl);
        if !arm.comments.is_empty() {
            out.push_str(&print_comments(source, &arm.comments, &nl));
            out.push_str(&nl);
        }
        let body = match assign(arm.expr) {
            Some(a) => a.rhs,
            None => arm.expr,
        };
        let conds = match &arm.conds {
            Some(conds) => (conds.iter())
                .map(|c| print(source, tokens, c, level))
                .collect::<Vec<_>>()
                .join(", "),
            None => "default".to_string(),
        };
        out.push_str(&conds);
        out.push_str(" => ");
        out.push_str(&print(source, tokens, body, level));
        out.push(',');
    }
    out.push('\n');
    out.push_str(&" ".repeat(base));
    out.push('}');
    out
}
