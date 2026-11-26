//! `analyse/disallowed/control-structure` (the 15 `ControlStructures/*ControlStructure` classes) and
//! `analyse/disallowed/keyword` (`Keywords/GlobalKeyword`), with `DisallowedKeywordRuleErrors`.

use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;

use super::{Entry, Site, Walker, format_message};

/// `DisallowedKeywordRuleErrors::get`.
fn forbid(w: &mut Walker, at: Span, keyword: &str, entries: &[Entry], identifier: &'static str) {
    for e in entries {
        if e.name == keyword && !w.is_allowed(&Site::default(), None, e, false, None) {
            let message = format!(
                "Using the {keyword} {} is forbidden{}",
                e.description,
                format_message(e.message.as_deref())
            );
            w.report(at, e, identifier, message);
            return;
        }
    }
}

/// A control structure.
pub(super) fn keyword(w: &mut Walker, at: Span, keyword: &str, identifier: &'static str) {
    let d = w.d;
    forbid(w, at, keyword, &d.structures, identifier);
}

/// `GlobalKeyword`.
pub(super) fn global(w: &mut Walker, at: Span) {
    let d = w.d;
    forbid(w, at, "global", &d.keywords, "disallowed.global");
}

/// `IfControlStructure`, `ElseIfControlStructure` and `ElseControlStructure`.
pub(super) fn r#if(w: &mut Walker, i: &If) {
    keyword(w, i.span(), "if", "disallowed.if");
    let (elseifs, r#else): (Vec<Span>, Option<Span>) = match &i.body {
        IfBody::Statement(b) => (
            b.else_if_clauses.iter().map(HasSpan::span).collect(),
            b.else_clause.as_ref().map(HasSpan::span),
        ),
        IfBody::ColonDelimited(b) => (
            b.else_if_clauses.iter().map(HasSpan::span).collect(),
            b.else_clause.as_ref().map(HasSpan::span),
        ),
    };
    for at in elseifs {
        keyword(w, at, "elseif", "disallowed.elseIf");
    }
    if let Some(at) = r#else {
        keyword(w, at, "else", "disallowed.else");
    }
}

/// `RequireIncludeControlStructure`.
pub(super) fn construct(w: &mut Walker, construct: &Construct) {
    let (keyword_name, identifier) = match construct {
        Construct::Include(_) => ("include", "disallowed.include"),
        Construct::IncludeOnce(_) => ("include_once", "disallowed.includeOnce"),
        Construct::Require(_) => ("require", "disallowed.require"),
        Construct::RequireOnce(_) => ("require_once", "disallowed.requireOnce"),
        _ => return,
    };
    keyword(w, construct.span(), keyword_name, identifier);
}
