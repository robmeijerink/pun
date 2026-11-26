//! The walker's entry into functions, methods, closures and arrow functions.

use super::super::*;

impl<'arena> Checker<'_, 'arena> {
    pub(super) fn visit_in_function(&mut self, f: &Function<'arena>) {
        dead_code::impure_function(self, f);
        dead_code::function_parameters(self, f);
        declaration_purity::function(self, f);
        callable_tags::function_like(self, f.span(), &f.parameter_list);
        conditional_return::function(self, f);
        assert_tags::function(self, f);
        phpdoc_types::function(self, f);
        invalid_throws::function(self, f);
        self.enter_this(false);
        functions::inner_function(self, f);
        self.larastan_enter_function(&f.parameter_list);
        missing_types::function(self, f);
        type_coverage::parameters(self, f.span(), &f.parameter_list, None);
        types::function(self, f);
        types::function_defaults(self, f);
        types::function_returns(self, f);
        types::too_wide_function(self, f);
        types::param_out_function(self, f);
        types::throws_void_function(self, f);
        types::throws_function(self, f);
        generics::function(self, f);
        variance::function(self, f);
        functions::function_like(self, f, &f.parameter_list);
        self.push_frame(FrameKind::Named);
        self.frames.last_mut().expect("pushed").defined = Some(var_names::defined(
            self,
            var_names::params(&f.parameter_list),
            &f.body,
        ));
        var_names::misplaced(self, f.span(), "a function");
        let name = self.resolved_span(f.name.span);
        names::typehints(
            self,
            names::FunctionKind::Function(&name),
            &f.parameter_list,
            f.return_type_hint.as_ref(),
        );
        signatures::check(
            self,
            names::FunctionKind::Function(&name),
            f.span(),
            &f.parameter_list,
            f.return_type_hint.as_ref(),
        );
        if self.level >= 2 {
            let types = declared::with_classes(self, || declared::function_phpdoc(self, f));
            let kind = names::FunctionKind::Function(&name);
            signatures::phpdoc_classes(
                self,
                kind,
                f.span(),
                &f.parameter_list,
                f.return_type_hint.as_ref(),
                &types,
            );
        }
        let docblock = self
            .codebase
            .get_function(name.as_bytes())
            .and_then(|f| f.return_type_metadata.as_ref())
            .filter(|t| t.from_docblock)
            .map(|t| &t.type_union);
        names::no_discard(
            self,
            names::FunctionKind::Function(&name),
            f.span(),
            f.attribute_lists.as_slice(),
            f.return_type_hint.as_ref(),
            false,
            docblock,
        );
        attributes::check(
            self,
            f.attribute_lists.as_slice(),
            attributes::TARGET_FUNCTION,
            "function",
        );
        declarations::promoted(
            self,
            f.span(),
            declarations::FunctionLike::Other,
            &f.parameter_list,
        );
        syntax::function_like_types(self, Some(&f.parameter_list), f.return_type_hint.as_ref());
        self.syntax.by_ref.push(f.ampersand.is_some());
        self.enter_function(Vec::new());
        self.var_tag_functions.push(f.span());
        let deprecated = self.entered_deprecated("", false, f.span());
        self.functions.push(FunctionScope {
            name: String::from_utf8_lossy(f.name.value).into_owned(),
            kind: FunctionScopeKind::Function,
            deprecated,
        });
    }

    pub(super) fn visit_in_method(&mut self, m: &Method<'arena>) {
        dead_code::impure_method(self, m);
        declaration_purity::method(self, m);
        phpdoc_tags::node(self, m.span());
        callable_tags::function_like(self, m.span(), &m.parameter_list);
        conditional_return::method(self, m);
        assert_tags::method(self, m);
        phpdoc_types::method(self, m);
        invalid_throws::method(self, m);
        phpdoc_types::promoted(self, m);
        self.enter_this(!m.modifiers.contains_static());
        functions::function_like(self, m, &m.parameter_list);
        statements::constructor_return_type(self, m);
        larastan::method(self, m);
        self.larastan_enter_function(&m.parameter_list);
        self.push_frame(FrameKind::Named);
        if let MethodBody::Concrete(body) = &m.body {
            self.frames.last_mut().expect("pushed").defined = Some(var_names::defined(
                self,
                var_names::params(&m.parameter_list),
                body,
            ));
        }
        var_names::misplaced(self, m.span(), "a method");
        for p in m
            .parameter_list
            .parameters
            .iter()
            .filter(|p| p.is_promoted_property())
        {
            names::property_type(self, p.span(), p.hint.as_ref(), &[p.variable.name]);
            signatures::property(self, p.span(), p.hint.as_ref(), &[p.variable.name]);
            if let Some(hooks) = &p.hooks {
                signatures::hooks(self, p.variable.name, hooks);
                signatures::hook_phpdoc_classes(self, p.variable.name, hooks, p.span());
                types::get_hooks(self, p.variable.name, p.hint.as_ref(), p.span(), hooks);
                types::hook_returns(self, p.variable.name, p.hint.as_ref(), p.span(), hooks);
                types::throws_void_hooks(self, p.variable.name, hooks);
                types::throws_hooks(self, p.variable.name, hooks);
                set_hook::hooks(self, p.variable.name, p.hint.as_ref(), hooks);
                set_hook::missing_types(self, p.variable.name, p.hint.as_ref(), hooks, p.span());
            }
        }
        let method = match &self.renamed {
            Some((span, alias)) if *span == m.span() => alias.clone(),
            _ => String::from_utf8_lossy(m.name.value).into_owned(),
        };
        declarations::method(self, m, &method);
        hierarchy::method(self, m, &method);
        overriding::method(self, m, &method);
        overriding::inherited_case(self, m, &method);
        strict::constructor(self, m);
        hierarchy::interface_method_visibility(self, m);
        dead_code::method_parameters(self, m, &method);
        missing_types::method(self, m);
        type_coverage::method(self, m, &method);
        type_coverage::parameters(self, m.span(), &m.parameter_list, Some(&method));
        types::method(self, m);
        types::method_defaults(self, m);
        types::method_returns(self, m);
        types::too_wide_method(self, m);
        types::param_out_method(self, m);
        types::self_out(self, m);
        types::throws_void_method(self, m);
        types::throws_method(self, m);
        types::throws_covariance(self, m);
        generics::method(self, m, &method);
        variance::method(self, m, &method);
        declarations::promoted(
            self,
            m.span(),
            declarations::FunctionLike::Method {
                name: &method,
                original: m.name.value,
                body: matches!(m.body, MethodBody::Concrete(_)),
            },
            &m.parameter_list,
        );
        if method.eq_ignore_ascii_case("__construct")
            || m.name.value.eq_ignore_ascii_case(b"__construct")
        {
            properties::promoted(self, m);
            variance::promoted(self, m);
            for p in m
                .parameter_list
                .parameters
                .iter()
                .filter(|p| p.is_promoted_property())
            {
                hierarchy::callable_property(self, p.span(), p.variable.name, p.hint.as_ref());
            }
        }
        let class = self
            .class_scope()
            .expect("a method is walked inside its class-like");
        let docblock = self
            .codebase
            .get_method(class.name.as_bytes(), method.as_bytes())
            .and_then(|f| f.return_type_metadata.as_ref())
            .filter(|t| t.from_docblock)
            .map(|t| &t.type_union);
        let class = self.display(class);
        let kind = names::FunctionKind::Method(&class, &method);
        names::typehints(self, kind, &m.parameter_list, m.return_type_hint.as_ref());
        signatures::check(
            self,
            kind,
            m.span(),
            &m.parameter_list,
            m.return_type_hint.as_ref(),
        );
        if self.level >= 2
            && let Some(context) = self.class_scope()
            && let Some(declaring) = missing_types::declaring(self, m.span())
        {
            let types = declared::with_classes(self, || {
                declared::method_phpdoc(self, m, declaring, context)
            });
            signatures::phpdoc_classes(
                self,
                kind,
                m.span(),
                &m.parameter_list,
                m.return_type_hint.as_ref(),
                &types,
            );
        }
        let forced_void = m.name.value.eq_ignore_ascii_case(b"__construct")
            || [
                "__construct",
                "__destruct",
                "__unset",
                "__wakeup",
                "__clone",
                "__set",
                "__unserialize",
            ]
            .iter()
            .any(|n| method.eq_ignore_ascii_case(n));
        names::no_discard(
            self,
            kind,
            m.span(),
            m.attribute_lists.as_slice(),
            m.return_type_hint.as_ref(),
            forced_void,
            docblock,
        );
        syntax::function_like_types(self, Some(&m.parameter_list), m.return_type_hint.as_ref());
        if m.name.value.eq_ignore_ascii_case(b"__construct") {
            for p in m
                .parameter_list
                .parameters
                .iter()
                .filter(|p| p.is_promoted_property())
            {
                if let Some(hint) = &p.hint {
                    syntax::complex_type(self, hint);
                }
            }
        }
        self.syntax.by_ref.push(m.ampersand.is_some());
        attributes::check(
            self,
            m.attribute_lists.as_slice(),
            attributes::TARGET_METHOD,
            "method",
        );
        let guards =
            if m.name.value.eq_ignore_ascii_case(b"__construct") || m.modifiers.contains_static() {
                Vec::new()
            } else {
                self.classes
                    .last()
                    .map(|c| c.constructor_guards.clone())
                    .unwrap_or_default()
            };
        self.enter_function(guards);
        self.var_tag_functions.push(m.span());
        let deprecated = self.entered_deprecated(&method, true, m.span());
        self.functions.push(FunctionScope {
            deprecated,
            name: method,
            kind: FunctionScopeKind::Method {
                is_static: m.modifiers.contains_static(),
            },
        });
    }

    pub(super) fn visit_in_closure(&mut self, f: &Closure<'arena>) {
        self.assign.enter_scope();
        strict::closure(self, f);
        if self.rebound.get(&f.span()).is_some_and(|b| b.defines_this) {
            self.enter_this(true);
        } else {
            self.enter_closure_this(f.r#static.is_some());
        }
        functions::function_like(self, f, &f.parameter_list);
        functions::closure_use(self, f);
        self.larastan_enter_closure(f);
        dead_code::closure(self, f);
        types::closure(self, f);
        type_coverage::parameters(self, f.span(), &f.parameter_list, None);
        types::anonymous_defaults(self, &f.parameter_list);
        types::closure_returns(self, f);
        types::too_wide_closure(self, f);
        syntax::function_like_types(self, Some(&f.parameter_list), f.return_type_hint.as_ref());
        self.syntax.parents.push(syntax::Parent::Closure);
        self.syntax.by_ref.push(f.ampersand.is_some());
        self.push_frame(FrameKind::Anonymous);
        self.frames.last_mut().expect("pushed").defined = Some(var_names::defined(
            self,
            var_names::params(&f.parameter_list).chain(
                f.use_clause
                    .iter()
                    .flat_map(|u| u.variables.iter())
                    .map(|u| u.variable.name),
            ),
            &f.body,
        ));
        if let Some(binding) = self.rebound.get(&f.span()) {
            self.classes.push(ClassScope {
                name: binding.class.clone(),
                constructor_guards: Vec::new(),
                bound: true,
            });
        }
        names::typehints(
            self,
            names::FunctionKind::Anonymous,
            &f.parameter_list,
            f.return_type_hint.as_ref(),
        );
        signatures::check(
            self,
            names::FunctionKind::Anonymous,
            f.span(),
            &f.parameter_list,
            f.return_type_hint.as_ref(),
        );
        names::no_discard(
            self,
            names::FunctionKind::Anonymous,
            f.span(),
            f.attribute_lists.as_slice(),
            f.return_type_hint.as_ref(),
            false,
            None,
        );
        attributes::check(
            self,
            f.attribute_lists.as_slice(),
            attributes::TARGET_FUNCTION,
            "function",
        );
        declarations::promoted(
            self,
            f.span(),
            declarations::FunctionLike::Other,
            &f.parameter_list,
        );
    }

    pub(super) fn visit_in_arrow_function(&mut self, f: &ArrowFunction<'arena>) {
        self.assign.enter_scope();
        types::anonymous_defaults(self, &f.parameter_list);
        types::arrow_function_returns(self, f);
        type_coverage::parameters(self, f.span(), &f.parameter_list, None);
        types::too_wide_arrow_function(self, f);
        if self.rebound.get(&f.span()).is_some_and(|b| b.defines_this) {
            self.enter_this(true);
        } else {
            self.enter_closure_this(f.r#static.is_some());
        }
        functions::function_like(self, f, &f.parameter_list);
        syntax::function_like_types(self, Some(&f.parameter_list), f.return_type_hint.as_ref());
        syntax::arrow_function(self, f);
        signatures::arrow_never(self, f);
        self.push_frame(FrameKind::Anonymous);
        if let Some(binding) = self.rebound.get(&f.span()) {
            self.classes.push(ClassScope {
                name: binding.class.clone(),
                constructor_guards: Vec::new(),
                bound: true,
            });
        }
        names::typehints(
            self,
            names::FunctionKind::Anonymous,
            &f.parameter_list,
            f.return_type_hint.as_ref(),
        );
        signatures::check(
            self,
            names::FunctionKind::Anonymous,
            f.span(),
            &f.parameter_list,
            f.return_type_hint.as_ref(),
        );
        names::no_discard(
            self,
            names::FunctionKind::Anonymous,
            f.span(),
            f.attribute_lists.as_slice(),
            f.return_type_hint.as_ref(),
            false,
            None,
        );
        attributes::check(
            self,
            f.attribute_lists.as_slice(),
            attributes::TARGET_FUNCTION,
            "function",
        );
        declarations::promoted(
            self,
            f.span(),
            declarations::FunctionLike::Other,
            &f.parameter_list,
        );
        self.mark_top_level(f.expression);
    }
}
