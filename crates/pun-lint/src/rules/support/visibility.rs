use crate::problem::{Edit, Span};
use mago_syntax::cst::{Class, ClassLikeMember, Modifier, Node, Sequence};

use crate::rules::support::class_members::modifiers_text;
use crate::syntax::node_span;

pub(crate) fn final_class<'a>(node: Node<'a, '_>) -> Option<&'a Class<'a>> {
    let Node::Class(c) = node else { return None };
    let class = c;
    let is_final = class
        .modifiers
        .iter()
        .any(|m| matches!(m, Modifier::Final(_)));
    let uses_trait = class
        .members
        .iter()
        .any(|m| matches!(m, ClassLikeMember::TraitUse(_)));
    (is_final && class.extends.is_none() && !uses_trait).then_some(class)
}

pub(crate) fn is_protected(modifiers: &Sequence<'_, Modifier<'_>>) -> bool {
    // Rector 2.9.2 privatizes `protected protected(set)` too; `make_private` drops the set visibility.
    modifiers
        .iter()
        .any(|m| matches!(m, Modifier::Protected(_)))
}

pub(crate) fn make_private(source: &str, modifiers: &Sequence<'_, Modifier<'_>>) -> Edit {
    let start = node_span(modifiers.first().expect("a protected member has modifiers")).start;
    let end = node_span(modifiers.last().expect("a protected member has modifiers")).end;
    let rest = &source[end..];
    let next = end + (rest.len() - rest.trim_start().len());
    Edit::new(
        Span::new(start, next),
        modifiers_text(modifiers)
            .replacen("final ", "", 1)
            .replacen("protected ", "private ", 1),
    )
}
