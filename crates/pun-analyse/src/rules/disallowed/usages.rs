//! `analyse/disallowed/namespace` (`NamespaceUsages`, `NamespaceFirstClassCallableUsages`),
//! `analyse/disallowed/constant` (`ConstantUsages`, `ClassConstantUsages`), `analyse/disallowed/superglobal`
//! (`VariableUsages`), `analyse/disallowed/attribute` (`AttributeUsages`) and `analyse/disallowed/property`
//! (`InstancePropertyUsages`, `StaticPropertyUsages`), with their `RuleErrors`.

use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::symbol::SymbolKind;
use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;
use pun_tokens::php_compat;

use super::{
    Entry, Position, Site, Walker, format_identifier, format_message, matches, partial_args,
};

/// `UsageFactory\NamespaceUsage`.
struct Usage {
    namespace: String,
    description: &'static str,
    identifier: &'static str,
    use_item: bool,
}

impl<'a> Walker<'_, 'a, '_> {
    /// `NamespaceUsageFactory::createClassUsage`.
    fn class_usage(&self, name: String) -> Usage {
        let kind = self
            .c
            .codebase
            .get_class_like(name.as_bytes())
            .map(|k| k.kind);
        let (description, identifier) = match kind {
            Some(SymbolKind::Enum) => ("Enum", "disallowed.enum"),
            Some(SymbolKind::Interface) => ("Interface", "disallowed.interface"),
            Some(SymbolKind::Trait) => ("Trait", "disallowed.trait"),
            _ => ("Class", "disallowed.class"),
        };
        Usage {
            namespace: super::normalize(&name),
            description,
            identifier,
            use_item: false,
        }
    }

    /// `DisallowedNamespaceRuleErrors::getDisallowedMessage`, for `disallowedNamespaces` and then
    /// `disallowedClasses` (two registrations of each rule).
    fn namespace(&mut self, at: Span, usage: &Usage, site: &Site, position: Option<Position>) {
        let d = self.d;
        for entries in [&d.namespaces, &d.classes] {
            for e in entries {
                if !matches(&e.name, &usage.namespace, &e.excludes)
                    || self.excluded_by_attribute(e, &usage.namespace)
                    || self.is_allowed(site, None, e, false, position)
                    || (e.allow_in_use && usage.use_item)
                {
                    continue;
                }
                let suffix = if e.name == usage.namespace {
                    String::new()
                } else {
                    format!(" [{} matches {}]", usage.namespace, e.name)
                };
                let message = format!(
                    "{} {} is forbidden{}{suffix}",
                    usage.description,
                    usage.namespace,
                    format_message(e.message.as_deref())
                );
                self.report(at, e, usage.identifier, message);
                break;
            }
        }
    }

    /// `Identifier::matches`' `excludeWithAttribute`: the class has one of the attributes.
    fn excluded_by_attribute(&self, e: &Entry, class: &str) -> bool {
        let Some(k) = self.c.codebase.get_class_like(class.as_bytes()) else {
            return false;
        };
        (k.attributes.iter()).any(|a| {
            let name = self.original_name(a.name.as_bytes());
            (e.exclude_with_attributes.iter())
                .any(|x| php_compat::fnmatch_flags(x.as_bytes(), name.as_bytes(), true, true))
        })
    }

    fn hint_usages(&self, hint: &Hint, out: &mut Vec<Usage>) {
        match hint {
            Hint::Identifier(id) => out.push(self.class_usage(self.resolved(id))),
            Hint::Nullable(n) => self.hint_usages(n.hint, out),
            Hint::Parenthesized(p) => self.hint_usages(p.hint, out),
            Hint::Union(u) => {
                self.hint_usages(u.left, out);
                self.hint_usages(u.right, out);
            }
            Hint::Intersection(i) => {
                self.hint_usages(i.left, out);
                self.hint_usages(i.right, out);
            }
            _ => {}
        }
    }

    fn class_of(&self, class: &Expression) -> Vec<&'a ClassLikeMetadata> {
        self.named_classes_of(class)
    }
}

/// A parameter, return, property or class constant type.
pub(super) fn type_hint(w: &mut Walker, hint: &Hint, position: Option<Position>, site: &Site) {
    let mut usages = Vec::new();
    w.hint_usages(hint, &mut usages);
    for u in usages {
        w.namespace(hint.span(), &u, site, position);
    }
}

pub(super) fn use_statement(w: &mut Walker, u: &Use) {
    let items: Vec<&UseItem> = match &u.items {
        UseItems::Sequence(s) => s.items.iter().collect(),
        UseItems::TypedSequence(s) => s.items.iter().collect(),
        UseItems::TypedList(l) => l.items.iter().collect(),
        UseItems::MixedList(l) => l.items.iter().map(|i| &i.item).collect(),
    };
    for item in items {
        let usage = Usage {
            namespace: super::normalize(&w.c.written(item.name.span())),
            description: "Namespace",
            identifier: "disallowed.namespace",
            use_item: true,
        };
        w.namespace(item.span(), &usage, &Site::default(), None);
    }
}

pub(super) fn trait_use(w: &mut Walker, u: &TraitUse) {
    for id in &u.trait_names {
        let usage = Usage {
            namespace: super::normalize(&w.resolved(id)),
            description: "Trait",
            identifier: "disallowed.trait",
            use_item: false,
        };
        w.namespace(u.span(), &usage, &Site::default(), None);
    }
}

pub(super) fn class_hierarchy(
    w: &mut Walker,
    at: Span,
    extends: Option<&Extends>,
    implements: Option<&Implements>,
) {
    let names: Vec<String> = (extends.into_iter().flat_map(|e| e.types.iter()))
        .chain(implements.into_iter().flat_map(|i| i.types.iter()))
        .map(|id| w.resolved(id))
        .collect();
    for name in names {
        let usage = w.class_usage(name);
        w.namespace(at, &usage, &Site::default(), None);
    }
}

/// A static call's, class constant fetch's or `instanceof`'s class name.
pub(super) fn static_class(w: &mut Walker, at: Span, class: &Expression) {
    if let Expression::Identifier(id) = class {
        let usage = w.class_usage(w.resolved(id));
        w.namespace(at, &usage, &Site::default(), None);
    }
}

pub(super) fn instantiation(w: &mut Walker, new: &Instantiation) {
    if let Expression::Identifier(id) = new.class {
        let usage = Usage {
            namespace: super::normalize(&w.resolved(id)),
            description: "Class",
            identifier: "disallowed.class",
            use_item: false,
        };
        w.namespace(new.span(), &usage, &Site::default(), None);
    }
}

/// `DisallowedAttributeRuleErrors::get` for each attribute of a declaration.
pub(super) fn attributes<'arena>(
    w: &mut Walker<'_, '_, 'arena>,
    at: Span,
    lists: &Sequence<'arena, AttributeList<'arena>>,
    site: &Site,
) {
    let d = w.d;
    for attribute in lists.iter().flat_map(|l| l.attributes.iter()) {
        let name = w.resolved(&attribute.name);
        let args = attribute
            .argument_list
            .as_ref()
            .map(partial_args)
            .unwrap_or_default();
        for e in &d.attributes {
            if !matches(&e.name, &name, &e.excludes)
                || w.is_allowed(site, Some(&args), e, true, None)
            {
                continue;
            }
            let suffix = if e.name == name {
                String::new()
            } else {
                format!(" [{name} matches {}]", e.name)
            };
            let message = format!(
                "Attribute {name} is forbidden{}{suffix}",
                format_message(e.message.as_deref())
            );
            w.report(at, e, "disallowed.attribute", message);
            break;
        }
    }
}

/// `DisallowedConstantRuleErrors::get`.
fn constant_errors(
    w: &mut Walker,
    at: Span,
    constant: &str,
    display: Option<&str>,
    entries: &[Entry],
    id: &'static str,
) {
    for e in entries {
        if e.name != constant || w.is_allowed(&Site::default(), None, e, false, None) {
            continue;
        }
        let shown = display
            .filter(|d| *d != e.name)
            .map(|d| format!(" (as {d})"))
            .unwrap_or_default();
        let message = format!(
            "Using {}{shown} is forbidden{}",
            e.name,
            format_message(e.message.as_deref())
        );
        w.report(at, e, id, message);
        return;
    }
}

/// `ConstantUsages`, and the class usage a qualified constant name is to `NamespaceUsages`.
pub(super) fn constant(w: &mut Walker, a: &ConstantAccess) {
    let name = match &a.name {
        Identifier::Local(l) => String::from_utf8_lossy(l.value).into_owned(),
        id => {
            let name = super::normalize(&w.resolved(id));
            let usage = w.class_usage(name.clone());
            w.namespace(a.span(), &usage, &Site::default(), None);
            name
        }
    };
    let d = w.d;
    constant_errors(
        w,
        a.span(),
        &name,
        None,
        &d.constants,
        "disallowed.constant",
    );
}

/// `ClassConstantUsages`, for `disallowedConstants` and then `disallowedEnums`.
pub(super) fn class_constant(w: &mut Walker, a: &ClassConstantAccess) {
    let names: Vec<String> = match &a.constant {
        ClassLikeConstantSelector::Identifier(id) => {
            vec![String::from_utf8_lossy(id.value).into_owned()]
        }
        ClassLikeConstantSelector::Expression(e) => (w.type_at(e.expression).into_iter())
            .flat_map(|t| t.constant_strings())
            .map(|s| String::from_utf8_lossy(&s).into_owned())
            .collect(),
        ClassLikeConstantSelector::Missing(_) => Vec::new(),
    };
    let classes = w.class_of(a.class);
    let d = w.d;
    for name in names {
        if name.eq_ignore_ascii_case("class") || classes.is_empty() {
            continue;
        }
        let displays: Vec<String> = classes.iter().map(|k| super::class_display(k)).collect();
        let display = format!("{}::{name}", format_identifier(&displays));
        let Some(declaring) = w.constant_declaring(classes[0], &name) else {
            continue;
        };
        if classes
            .iter()
            .any(|k| w.constant_declaring(k, &name).is_none())
        {
            continue;
        }
        let constant = format!(
            "{}::{name}",
            super::normalize(&super::class_display(declaring))
        );
        for entries in [&d.constants, &d.enums] {
            constant_errors(
                w,
                a.span(),
                &constant,
                Some(&display),
                entries,
                "disallowed.classConstant",
            );
        }
    }
}

impl<'a> Walker<'_, 'a, '_> {
    /// The class-like that declares constant (or enum case) `name` of `k`.
    fn constant_declaring(
        &self,
        k: &'a ClassLikeMetadata,
        name: &str,
    ) -> Option<&'a ClassLikeMetadata> {
        let word = mago_word::word(name);
        // Mago copies inherited constants into the child; the declaring class is the one around the declaration
        let inside = |k: &ClassLikeMetadata, s: Span| {
            s.file_id == k.span.file_id && k.span.contains(&s.start)
        };
        let has = |k: &ClassLikeMetadata| {
            k.constants.get(&word).is_some_and(|c| inside(k, c.span))
                || k.enum_cases.get(&word).is_some_and(|c| inside(k, c.span))
        };
        if has(k) {
            return Some(k);
        }
        (k.all_parent_classes.iter())
            .chain(k.used_traits.iter())
            .chain(k.all_parent_interfaces.iter())
            .filter_map(|n| self.c.codebase.get_class_like(n.as_bytes()))
            .find(|p| has(p))
    }

    /// The classes a class expression names or its type has.
    pub(super) fn named_classes_of(&self, class: &Expression) -> Vec<&'a ClassLikeMetadata> {
        match class {
            Expression::Identifier(id) => {
                self.class_named(&self.resolved(id)).into_iter().collect()
            }
            Expression::Self_(_) => self.class_named("self").into_iter().collect(),
            Expression::Static(_) => self.class_named("static").into_iter().collect(),
            Expression::Parent(_) => self.class_named("parent").into_iter().collect(),
            // `$this` is the class the code is in
            Expression::Variable(Variable::Direct(v))
                if v.name == b"$this" && self.class().is_some() =>
            {
                self.class().into_iter().collect()
            }
            e => (self.type_at(e).into_iter())
                .flat_map(|t| {
                    // a class string's class: `getObjectTypeOrClassStringObjectType`
                    let strings = t.constant_strings();
                    if !strings.is_empty() {
                        return (strings.iter())
                            .map(|s| {
                                String::from_utf8_lossy(s)
                                    .trim_start_matches('\\')
                                    .to_string()
                            })
                            .collect();
                    }
                    let t = match t {
                        pun_phpstan::types::Type::ClassString(Some(o)) => *o,
                        t => t,
                    };
                    t.object_class_names()
                })
                .filter_map(|n| self.type_class(&n))
                .collect(),
        }
    }

    /// `DisallowedPropertyRuleErrors::getErrorMessages`.
    fn property_errors(
        &mut self,
        at: Span,
        classes: &[&'a ClassLikeMetadata],
        name: &str,
        is_static: bool,
    ) {
        let d = self.d;
        for k in classes {
            let dollar = format!("${name}");
            let Some((declaring, origin, p)) = self.c.declared_property(k, dollar.as_bytes())
            else {
                continue;
            };
            if p.flags.is_static() != is_static {
                continue;
            }
            let shown = |k: &ClassLikeMetadata| {
                format!("{}::${name}", super::normalize(&super::class_display(k)))
            };
            let mut candidates = vec![shown(declaring)];
            if origin.kind == SymbolKind::Trait {
                candidates.push(shown(origin));
            }
            let display = shown(k);
            'entries: for e in &d.properties {
                for candidate in &candidates {
                    if !matches(&e.name, candidate, &[])
                        || self.is_allowed(&Site::default(), None, e, false, None)
                    {
                        continue;
                    }
                    let used = if display == *candidate {
                        candidate.clone()
                    } else {
                        format!("{candidate} (as {display})")
                    };
                    let suffix = if e.name == *candidate {
                        String::new()
                    } else {
                        format!(" [{candidate} matches {}]", e.name)
                    };
                    let message = format!(
                        "Using {used} is forbidden{}{suffix}",
                        format_message(e.message.as_deref())
                    );
                    self.report(at, e, "disallowed.property", message);
                    break 'entries;
                }
            }
        }
    }
}

/// `VariableUsages`.
pub(super) fn variable(w: &mut Walker, v: &Variable) {
    let Variable::Direct(v) = v else {
        return;
    };
    if w.static_properties.contains(&v.span) {
        return;
    }
    let name = String::from_utf8_lossy(v.name).into_owned();
    let d = w.d;
    for e in &d.superglobals {
        if e.name == name && !w.is_allowed(&Site::default(), None, e, false, None) {
            let message = format!(
                "Using {name} is forbidden{}",
                format_message(e.message.as_deref())
            );
            w.report(v.span, e, "disallowed.variable", message);
            return;
        }
    }
}

/// `InstancePropertyUsages`.
pub(super) fn property(w: &mut Walker, p: &PropertyAccess) {
    let classes = w.class_of(p.object);
    for name in w.member_names(&p.property) {
        w.property_errors(p.span(), &classes, &name, false);
    }
}

/// `StaticPropertyUsages`.
pub(super) fn static_property(w: &mut Walker, p: &StaticPropertyAccess) {
    // `TypeResolver::getNames`: the name, or the first constant string of a variable or a string literal
    let t = match &p.property {
        Variable::Direct(v) => {
            w.static_properties.insert(v.span);
            Some(pun_phpstan::types::Type::ConstString(v.name[1..].to_vec()))
        }
        Variable::Nested(n) => w.c.phpstan_type_at(n.variable.span()),
        Variable::Indirect(i) => match i.expression {
            Expression::Variable(_) | Expression::Literal(Literal::String(_)) => {
                w.type_at(i.expression)
            }
            _ => None,
        },
    };
    let classes = w.class_of(p.class);
    for name in t
        .into_iter()
        .flat_map(|t| t.constant_strings().into_iter().take(1))
    {
        let name = String::from_utf8_lossy(&name).into_owned();
        w.property_errors(p.span(), &classes, &name, true);
    }
}
