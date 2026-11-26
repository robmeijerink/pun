//! The walker's entry into classes, interfaces, traits, enums and their members.

use super::super::*;

impl<'arena> Checker<'_, 'arena> {
    pub(super) fn visit_in_class(&mut self, class: &Class<'arena>) {
        let current = format!("Class {}", self.resolved_span(class.name.span));
        names::class_hierarchy(
            self,
            class.span(),
            &current,
            class.extends.as_ref(),
            class.implements.as_ref(),
            false,
            class.modifiers.contains_readonly(),
        );
        if self.trait_context.is_none() {
            let name = self.resolved_span(class.name.span);
            self.trait_uses_of(&name, class.members.as_slice());
        }
        self.enter_class(
            Some(self.resolved_span(class.name.span)),
            class.members.as_slice(),
        );
        self.push_frame(FrameKind::Class);
        var_names::misplaced(self, class.span(), "a class");
        let decl = self.decl_of(
            class.span(),
            Some(class.name.span),
            SymbolKind::Class,
            class.modifiers.as_slice(),
            class.extends.as_ref(),
        );
        self.decls.push(decl);
        declarations::duplicates(self, class.members.as_slice());
        let name = self.resolved_span(class.name.span);
        attributes::class_like(
            self,
            class.span(),
            class.attribute_lists.as_slice(),
            SymbolKind::Class,
            class.modifiers.contains_readonly(),
            &name,
        );
        if let Some(k) = self.codebase.get_class_like(name.as_bytes()) {
            hierarchy::in_class(self, class.span(), k);
            require_ancestors::check(self, class.span(), k);
            hierarchy::allowed_subtypes(self, class.span(), k);
            require_tags::class_like(self, class.span(), k);
            class_tags::class_like(self, class.span(), k);
            dead_code::unused_private(
                self,
                k,
                class.attribute_lists.as_slice(),
                class.members.as_slice(),
            );
            readonly_init::class(self, k, class.members.as_slice());
            types::too_wide_properties(self, k, class.members.as_slice());
        }
        type_aliases::class_like(self, class.span());
        invalid_throws::node(self, class.span());
        missing_types::class(self, class);
        type_coverage::properties(self, class.members.as_slice());
        generics::class(self, class);
        larastan::class(self, class);
        constants::read_only_class(self, class.span(), class.modifiers.as_slice(), false);
    }

    pub(super) fn visit_in_interface(&mut self, i: &Interface<'arena>) {
        let current = format!("Interface {}", self.resolved_span(i.name.span));
        names::class_hierarchy(
            self,
            i.span(),
            &current,
            i.extends.as_ref(),
            None,
            true,
            false,
        );
        self.enter_class(Some(self.resolved_span(i.name.span)), i.members.as_slice());
        self.push_frame(FrameKind::Class);
        var_names::misplaced(self, i.span(), "an interface");
        let decl = self.decl_of(
            i.span(),
            Some(i.name.span),
            SymbolKind::Interface,
            &[],
            None,
        );
        self.decls.push(decl);
        declarations::duplicates(self, i.members.as_slice());
        let name = self.resolved_span(i.name.span);
        attributes::class_like(
            self,
            i.span(),
            i.attribute_lists.as_slice(),
            SymbolKind::Interface,
            false,
            &name,
        );
        if let Some(k) = self.codebase.get_class_like(name.as_bytes()) {
            hierarchy::allowed_subtypes(self, i.span(), k);
            require_tags::class_like(self, i.span(), k);
            class_tags::class_like(self, i.span(), k);
        }
        type_aliases::class_like(self, i.span());
        invalid_throws::node(self, i.span());
        missing_types::interface(self, i);
        type_coverage::properties(self, i.members.as_slice());
        generics::interface(self, i);
    }

    pub(super) fn visit_in_enum(&mut self, e: &Enum<'arena>) {
        let current = format!("Enum {}", self.resolved_span(e.name.span));
        names::class_hierarchy(
            self,
            e.span(),
            &current,
            None,
            e.implements.as_ref(),
            false,
            false,
        );
        if self.trait_context.is_none() {
            let name = self.resolved_span(e.name.span);
            self.trait_uses_of(&name, e.members.as_slice());
        }
        self.enter_class(Some(self.resolved_span(e.name.span)), e.members.as_slice());
        self.push_frame(FrameKind::Class);
        var_names::misplaced(self, e.span(), "an enum");
        let decl = self.decl_of(e.span(), Some(e.name.span), SymbolKind::Enum, &[], None);
        self.decls.push(decl);
        enums::sanity(self, e);
        declarations::duplicates(self, e.members.as_slice());
        let name = self.resolved_span(e.name.span);
        attributes::class_like(
            self,
            e.span(),
            e.attribute_lists.as_slice(),
            SymbolKind::Enum,
            false,
            &name,
        );
        if let Some(k) = self.codebase.get_class_like(name.as_bytes()) {
            hierarchy::in_class(self, e.span(), k);
            require_ancestors::check(self, e.span(), k);
            hierarchy::allowed_subtypes(self, e.span(), k);
            require_tags::class_like(self, e.span(), k);
            class_tags::class_like(self, e.span(), k);
            dead_code::unused_private(self, k, e.attribute_lists.as_slice(), e.members.as_slice());
        }
        type_aliases::class_like(self, e.span());
        invalid_throws::node(self, e.span());
        missing_types::enum_(self, e);
        type_coverage::properties(self, e.members.as_slice());
        generics::enum_(self, e);
    }

    pub(super) fn visit_trait(&mut self, t: &Trait<'arena>, c: &mut ()) {
        if self.file.file_type != mago_database::file::FileType::Host {
            return;
        }
        let start = i64::from(self.file.line_number(t.span().start.offset)) + 1;
        let end = i64::from(self.file.line_number(t.span().end.offset)) + 1;
        self.trait_lines.push((start, end));
        let Some(class) = self.trait_context.as_ref().map(|t| t.class.clone()) else {
            attributes::trait_declaration(self, t);
            hierarchy::sealed_trait(self, t);
            let name = self.resolved_span(t.name.span);
            require_tags::trait_(self, t.span(), &name);
            class_tags::trait_definition(self, t.span(), &name);
            type_aliases::trait_definition(self, t.span(), &name);
            missing_types::trait_(self, t);
            generics::trait_(self, t);
            return;
        };
        let name = self.resolved_span(t.name.span);
        let target = self
            .trait_context
            .as_ref()
            .is_some_and(|t| t.trait_name.eq_ignore_ascii_case(&name));
        let Some(decl) = target.then(|| self.decl_of_class(&class)).flatten() else {
            return;
        };
        self.trait_uses_of(&class, t.members.as_slice());
        let saved = self.classes.len();
        self.enter_class(None, t.members.as_slice());
        self.classes[saved].name = Some(class.clone());
        self.decls.push(decl);
        self.walk.insert(Walk::IN_TARGET);
        self.source_types.get_mut().expect("source types").clear();
        self.types.extend(crate::codebase::trait_initializer_types(
            self.codebase,
            self.file,
            t.span(),
            mago_word::word(&class),
            self.php_version,
        ));
        let display = self
            .codebase
            .get_class_like(name.as_bytes())
            .map_or_else(|| name.clone(), |k| self.display(k));
        declarations::trait_duplicates(self, t.members.as_slice(), &display);
        attributes::trait_in_context(self, t);
        type_aliases::trait_use(self, t.span(), &name, &class);
        class_tags::trait_use(self, t.span(), &name, &class);
        invalid_throws::node(self, t.span());
        for member in &t.members {
            if let ClassLikeMember::Method(m) = member {
                let original = String::from_utf8_lossy(m.name.value).into_owned();
                let name = self
                    .trait_context
                    .as_ref()
                    .and_then(|t| t.renames.get(&original.to_ascii_lowercase()))
                    .cloned()
                    .unwrap_or(original);
                if self.shadowed(&class, &name, t.span()) {
                    continue;
                }
                self.renamed = Some((m.span(), name));
            }
            self.walk_class_like_member(member, c);
        }
        self.walk.remove(Walk::IN_TARGET);
        self.classes.truncate(saved);
        self.decls.pop();
    }

    pub(super) fn visit_anonymous_class(&mut self, a: &AnonymousClass<'arena>, c: &mut ()) {
        if let Some(arguments) = &a.argument_list {
            self.walk_partial_argument_list(arguments, c);
        }
        self.walk_in_anonymous_class(a, c);
        for attribute_list in &a.attribute_lists {
            self.walk_attribute_list(attribute_list, c);
        }
        for modifier in &a.modifiers {
            self.walk_modifier(modifier, c);
        }
        if let Some(extends) = &a.extends {
            self.walk_extends(extends, c);
        }
        if let Some(implements) = &a.implements {
            self.walk_implements(implements, c);
        }
        for member in &a.members {
            self.walk_class_like_member(member, c);
        }
        self.walk_out_anonymous_class(a, c);
    }

    pub(super) fn visit_in_anonymous_class(&mut self, a: &AnonymousClass<'arena>) {
        names::class_hierarchy(
            self,
            a.span(),
            "Anonymous class",
            a.extends.as_ref(),
            a.implements.as_ref(),
            false,
            a.modifiers.contains_readonly(),
        );
        let name = String::from_utf8_lossy(
            mago_codex::get_anonymous_class_name(self.file, a.span()).as_bytes(),
        )
        .into_owned();
        classes::anonymous(self, a, &name);
        if self.trait_context.is_none() {
            self.trait_uses_of(&name, a.members.as_slice());
        }
        self.enter_class(Some(name.clone()), a.members.as_slice());
        self.push_frame(FrameKind::Class);
        let mut decl = self.decl_of(
            a.span(),
            None,
            SymbolKind::Class,
            a.modifiers.as_slice(),
            a.extends.as_ref(),
        );
        let display = self.anonymous[name.as_bytes()].clone();
        decl.display = Some(display.clone());
        self.decls.push(decl);
        declarations::duplicates(self, a.members.as_slice());
        attributes::class_like(
            self,
            a.span(),
            a.attribute_lists.as_slice(),
            SymbolKind::Class,
            a.modifiers.contains_readonly(),
            &display,
        );
        if let Some(k) = self.codebase.get_class_like(name.as_bytes()) {
            hierarchy::in_class(self, a.span(), k);
            require_ancestors::check(self, a.span(), k);
            hierarchy::allowed_subtypes(self, a.span(), k);
            require_tags::class_like(self, a.span(), k);
            class_tags::class_like(self, a.span(), k);
            dead_code::unused_private(self, k, a.attribute_lists.as_slice(), a.members.as_slice());
            readonly_init::class(self, k, a.members.as_slice());
            types::too_wide_properties(self, k, a.members.as_slice());
        }
        generics::anonymous(self, a, &name, &display);
        type_coverage::properties(self, a.members.as_slice());
        invalid_throws::anonymous(self, a);
        constants::read_only_class(self, a.span(), a.modifiers.as_slice(), true);
    }

    pub(super) fn visit_in_property(&mut self, p: &Property<'arena>) {
        phpdoc_tags::node(self, p.span());
        invalid_throws::node(self, p.span());
        let names: Vec<&[u8]> = p.variables().iter().map(|v| v.name).collect();
        names::property_type(self, p.span(), p.hint(), &names);
        signatures::property(self, p.span(), p.hint(), &names);
        if let Property::Hooked(h) = p {
            signatures::hooks(self, h.item.variable().name, &h.hook_list);
            declaration_purity::hooks(self, h.item.variable().name, &h.hook_list);
            signatures::hook_phpdoc_classes(self, h.item.variable().name, &h.hook_list, p.span());
            set_hook::hooks(self, h.item.variable().name, p.hint(), &h.hook_list);
            set_hook::missing_types(
                self,
                h.item.variable().name,
                p.hint(),
                &h.hook_list,
                p.span(),
            );
            types::get_hooks(
                self,
                h.item.variable().name,
                p.hint(),
                p.span(),
                &h.hook_list,
            );
            types::hook_returns(
                self,
                h.item.variable().name,
                p.hint(),
                p.span(),
                &h.hook_list,
            );
            types::throws_void_hooks(self, h.item.variable().name, &h.hook_list);
            types::throws_hooks(self, h.item.variable().name, &h.hook_list);
            for hook in &h.hook_list.hooks {
                phpdoc_types::hook(self, hook, h.item.variable().name, p.hint());
            }
        }
        properties::property(self, p);
        missing_types::property(self, p);
        types::property_default(self, p);
        phpdoc_types::property(self, p);
        larastan::property(self, p);
        for v in p.variables() {
            hierarchy::callable_property(self, v.span(), v.name, p.hint());
        }
        if let Some(hint) = p.hint() {
            for _ in &names {
                syntax::complex_type(self, hint);
            }
        }
        let lists = match p {
            Property::Plain(p) => p.attribute_lists.as_slice(),
            Property::Hooked(h) => h.attribute_lists.as_slice(),
        };
        for v in p.variables() {
            attributes::property(self, v.span(), lists, false);
        }
    }

    pub(super) fn visit_in_property_hook(&mut self, h: &PropertyHook<'arena>) {
        phpdoc_tags::node(self, h.span());
        invalid_throws::node(self, h.span());
        self.enter_this(true);
        attributes::property_hook(self, h);
        if let Some(params) = &h.parameter_list {
            functions::function_like(self, h, params);
            declarations::promoted(self, h.span(), declarations::FunctionLike::Other, params);
            type_coverage::parameters(self, h.span(), params, None);
        }
        syntax::function_like_types(self, h.parameter_list.as_ref(), None);
        self.syntax.by_ref.push(h.ampersand.is_some());
        self.push_frame(FrameKind::Named);
        if let PropertyHookBody::Concrete(PropertyHookConcreteBody::Block(body)) = &h.body {
            let names: Vec<&[u8]> = match &h.parameter_list {
                Some(list) => var_names::params(list).collect(),
                None if h.name.value == b"set" => vec![b"$value"],
                None => Vec::new(),
            };
            self.frames.last_mut().expect("pushed").defined =
                Some(var_names::defined(self, names, body));
        }
    }
}
