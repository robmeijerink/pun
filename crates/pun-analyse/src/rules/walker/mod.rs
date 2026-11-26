use super::*;

mod call_sites;
mod class_likes;
mod control_flow;
mod function_likes;

impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for Checker<'_, 'arena> {
    fn walk_expression(&mut self, e: &'ast Expression<'arena>, context: &mut ()) {
        assign::enter_expression(self, e);
        mago_syntax::walker::walk_expression_mut(self, e, context);
        assign::exit_expression(self, e);
    }
    fn walk_class(&mut self, class: &'ast Class<'arena>, c: &mut ()) {
        if !self.outside_target_trait() && self.utf8_name(class.name.span) {
            mago_syntax::walker::walk_class_mut(self, class, c);
        }
    }
    fn walk_interface(&mut self, i: &'ast Interface<'arena>, c: &mut ()) {
        if !self.outside_target_trait() && self.utf8_name(i.name.span) {
            mago_syntax::walker::walk_interface_mut(self, i, c);
        }
    }
    fn walk_enum(&mut self, e: &'ast Enum<'arena>, c: &mut ()) {
        if !self.outside_target_trait() && self.utf8_name(e.name.span) {
            mago_syntax::walker::walk_enum_mut(self, e, c);
        }
    }
    fn walk_function(&mut self, f: &'ast Function<'arena>, c: &mut ()) {
        if !self.outside_target_trait() {
            mago_syntax::walker::walk_function_mut(self, f, c);
        }
    }
    fn walk_in_class(&mut self, class: &'ast Class<'arena>, (): &mut ()) {
        self.visit_in_class(class);
    }
    fn walk_out_class(&mut self, class: &'ast Class<'arena>, (): &mut ()) {
        phpdoc_tags::nop(self, class.right_brace);
        self.classes.pop();
        self.decls.pop();
        self.pop_frame();
    }
    fn walk_in_interface(&mut self, i: &'ast Interface<'arena>, (): &mut ()) {
        self.visit_in_interface(i);
    }
    fn walk_out_interface(&mut self, i: &'ast Interface<'arena>, (): &mut ()) {
        phpdoc_tags::nop(self, i.right_brace);
        self.classes.pop();
        self.decls.pop();
        self.pop_frame();
    }
    fn walk_in_enum(&mut self, e: &'ast Enum<'arena>, (): &mut ()) {
        self.visit_in_enum(e);
    }
    fn walk_out_enum(&mut self, e: &'ast Enum<'arena>, (): &mut ()) {
        phpdoc_tags::nop(self, e.right_brace);
        self.classes.pop();
        self.decls.pop();
        self.pop_frame();
    }
    fn walk_trait(&mut self, t: &'ast Trait<'arena>, c: &mut ()) {
        if self.utf8_name(t.name.span) {
            self.visit_trait(t, c);
        }
    }
    fn walk_anonymous_class(&mut self, a: &'ast AnonymousClass<'arena>, c: &mut ()) {
        self.visit_anonymous_class(a, c);
    }
    fn walk_in_anonymous_class(&mut self, a: &'ast AnonymousClass<'arena>, (): &mut ()) {
        self.visit_in_anonymous_class(a);
    }
    fn walk_in_function(&mut self, f: &'ast Function<'arena>, (): &mut ()) {
        self.visit_in_function(f);
    }
    fn walk_out_function(&mut self, _: &'ast Function<'arena>, (): &mut ()) {
        self.var_tag_functions.pop();
        self.leave_this();
        self.larastan_leave_function();
        self.syntax.by_ref.pop();
        self.pop_frame();
        self.leave_function();
        self.functions.pop();
    }
    fn walk_in_method(&mut self, m: &'ast Method<'arena>, (): &mut ()) {
        self.visit_in_method(m);
    }
    fn walk_out_method(&mut self, _: &'ast Method<'arena>, (): &mut ()) {
        self.var_tag_functions.pop();
        self.leave_this();
        self.larastan_leave_function();
        self.syntax.by_ref.pop();
        self.pop_frame();
        self.leave_function();
        self.functions.pop();
    }
    fn walk_in_block(&mut self, _: &'ast Block<'arena>, (): &mut ()) {
        self.marks.push(self.guards.len());
    }
    fn walk_out_block(&mut self, b: &'ast Block<'arena>, (): &mut ()) {
        phpdoc_tags::nop(self, b.right_brace);
        self.end_guard_block();
    }
    fn walk_if(&mut self, r#if: &'ast If<'arena>, c: &mut ()) {
        self.visit_if(r#if, c);
    }
    fn walk_conditional(&mut self, cond: &'ast Conditional<'arena>, c: &mut ()) {
        self.visit_conditional(cond, c);
    }
    fn walk_binary(&mut self, b: &'ast Binary<'arena>, c: &mut ()) {
        self.visit_binary(b, c);
    }
    fn walk_match(&mut self, m: &'ast Match<'arena>, c: &mut ()) {
        self.visit_match(m, c);
    }
    fn walk_assignment(&mut self, a: &'ast Assignment<'arena>, context: &mut ()) {
        self.walk_in_assignment(a, context);
        if assign::is_list_target(a.lhs) {
            self.walk_expression(a.rhs, context);
            self.walk_expression(a.lhs, context);
        } else {
            self.walk_expression(a.lhs, context);
            self.walk_expression(a.rhs, context);
        }
        self.walk_assignment_operator(&a.operator, context);
        self.walk_out_assignment(a, context);
    }
    fn walk_in_assignment(&mut self, a: &'ast Assignment<'arena>, (): &mut ()) {
        self.visit_in_assignment(a);
    }
    fn walk_out_assignment(&mut self, a: &'ast Assignment<'arena>, (): &mut ()) {
        self.assigned_expression(a);
        property_access::assigned(self, a);
        include_file::assigned(self, a.lhs);
    }
    fn walk_in_construct(&mut self, construct: &'ast Construct<'arena>, (): &mut ()) {
        include_file::construct(self, construct);
        match construct {
            Construct::Isset(i) => dead_code::isset(self, i),
            Construct::Empty(e) => dead_code::empty(self, e),
            _ => {}
        }
    }
    fn walk_in_property_access(&mut self, p: &'ast PropertyAccess<'arena>, (): &mut ()) {
        property_access::read(self, p);
        write_only::read(self, p.span(), p.object, &p.property);
        internal::property_fetch(self, p);
    }
    fn walk_in_null_safe_property_access(
        &mut self,
        p: &'ast NullSafePropertyAccess<'arena>,
        (): &mut (),
    ) {
        property_access::null_safe_read(self, p);
        write_only::null_safe(self, p);
        types::nullsafe_property(self, p);
    }
    fn walk_in_static_property_access(
        &mut self,
        p: &'ast StaticPropertyAccess<'arena>,
        (): &mut (),
    ) {
        self.variables.property_selectors.insert(p.property.span());
        property_access::static_read(self, p);
        through_static::property(self, p);
        internal::static_property_fetch(self, p);
    }
    fn walk_out_anonymous_class(&mut self, a: &'ast AnonymousClass<'arena>, (): &mut ()) {
        phpdoc_tags::nop(self, a.right_brace);
        self.classes.pop();
        self.pop_frame();
        self.decls.pop();
    }
    fn walk_in_variable(&mut self, variable: &'ast Variable<'arena>, (): &mut ()) {
        variables::variable(self, variable);
    }
    fn walk_in_closure_use_clause_variable(
        &mut self,
        capture: &'ast ClosureUseClauseVariable<'arena>,
        (): &mut (),
    ) {
        variables::capture(self, capture);
    }
    fn walk_in_function_call(&mut self, call: &'ast FunctionCall<'arena>, (): &mut ()) {
        self.visit_in_function_call(call);
    }
    fn walk_out_function_call(&mut self, call: &'ast FunctionCall<'arena>, (): &mut ()) {
        self.in_exists_call -= usize::from(calls::is_class_exists_family(self, call));
        let references = assign::by_ref_arguments(self, call);
        self.invalidate_call_references(&call.argument_list, references);
    }
    fn walk_in_instantiation(&mut self, new: &'ast Instantiation<'arena>, (): &mut ()) {
        new_static_abstract::instantiation(self, new);
        if statements::instantiation_callable(self, new) {
            return;
        }
        classes::instantiation(self, new);
        date_interval::instantiation(self, new);
        date_time::instantiation(self, new);
        assign::arguments(self, assign::Callee::New(new));
    }
    fn walk_in_null_safe_method_call(
        &mut self,
        call: &'ast NullSafeMethodCall<'arena>,
        (): &mut (),
    ) {
        self.visit_in_null_safe_method_call(call);
    }
    fn walk_in_method_call(&mut self, call: &'ast MethodCall<'arena>, (): &mut ()) {
        self.visit_in_method_call(call);
    }
    fn walk_out_method_call(&mut self, call: &'ast MethodCall<'arena>, (): &mut ()) {
        self.invalidate_called_receiver(call.object, &call.method);
        let references =
            assign::by_ref_call_arguments(self, assign::Callee::Method(call), &call.argument_list);
        self.invalidate_call_references(&call.argument_list, references);
    }
    fn walk_in_pipe(&mut self, p: &'ast Pipe<'arena>, (): &mut ()) {
        self.piped.insert(p.callable.span());
        types::pipe_by_ref(self, p);
        calls::pipe(self, p);
        types::callable_pipe(self, p);
        function_checks::pipe(self, p);
        restricted_functions::pipe(self, p);
        types::pipe_operation(self, p);
    }
    fn walk_in_function_partial_application(
        &mut self,
        f: &'ast FunctionPartialApplication<'arena>,
        (): &mut (),
    ) {
        let whole = f.function.span().join(f.argument_list.span());
        if !statements::first_class_callable(self, whole, &f.argument_list) {
            function_callable::check(self, f);
            restricted_functions::callable(self, f);
        }
    }
    fn walk_in_method_partial_application(
        &mut self,
        m: &'ast MethodPartialApplication<'arena>,
        (): &mut (),
    ) {
        if !statements::first_class_callable(self, m.span(), &m.argument_list) {
            method_callable::method(self, m);
        }
        internal::method_callable(self, m);
        if internal::is_callable(&m.argument_list) {
            strict::dynamic_call(self, m.span(), m.object, &m.method, false);
        }
    }
    fn walk_in_static_method_partial_application(
        &mut self,
        m: &'ast StaticMethodPartialApplication<'arena>,
        (): &mut (),
    ) {
        if !statements::first_class_callable(self, m.span(), &m.argument_list) {
            method_callable::static_method(self, m);
        }
        internal::static_method_callable(self, m);
    }
    fn walk_in_static_method_call(&mut self, call: &'ast StaticMethodCall<'arena>, (): &mut ()) {
        self.visit_in_static_method_call(call);
    }
    fn walk_out_static_method_call(&mut self, call: &'ast StaticMethodCall<'arena>, (): &mut ()) {
        self.invalidate_static_call(call);
        let references = assign::by_ref_call_arguments(
            self,
            assign::Callee::StaticMethod(call),
            &call.argument_list,
        );
        self.invalidate_call_references(&call.argument_list, references);
        if self.is_closure_bind(call) {
            self.closure_bind -= 1;
        }
    }
    fn walk_out_closure(&mut self, f: &'ast Closure<'arena>, (): &mut ()) {
        self.assign.leave_scope();
        self.leave_this();
        self.larastan_leave_function();
        self.syntax.parents.pop();
        self.syntax.by_ref.pop();
        self.pop_frame();
        if self.rebound.contains_key(&f.span()) {
            self.classes.pop();
        }
    }
    fn walk_out_arrow_function(&mut self, f: &'ast ArrowFunction<'arena>, (): &mut ()) {
        self.assign.leave_scope();
        self.leave_this();
        self.pop_frame();
        if self.rebound.contains_key(&f.span()) {
            self.classes.pop();
        }
    }
    fn walk_in_statement_expression(&mut self, s: &'ast ExpressionStatement<'arena>, (): &mut ()) {
        self.mark_top_level(s.expression);
        statements::standalone_throw(self, s);
        dead_code::statement(self, s);
    }
    fn walk_in_clone(&mut self, k: &'ast Clone<'arena>, (): &mut ()) {
        types::clone(self, k);
    }
    fn walk_in_array_access(&mut self, a: &'ast ArrayAccess<'arena>, (): &mut ()) {
        types::array_access(self, a);
        types::dim_fetch(self, a.span(), a.array, Some(a.index));
    }
    fn walk_in_key_value_array_element(
        &mut self,
        kv: &'ast KeyValueArrayElement<'arena>,
        (): &mut (),
    ) {
        types::array_item(self, kv);
    }
    fn walk_in_throw(&mut self, t: &'ast Throw<'arena>, (): &mut ()) {
        statements::throw_expression(self, t);
        types::throw(self, t);
    }
    fn walk_in_array(&mut self, a: &'ast Array<'arena>, (): &mut ()) {
        arrays::literal_array(self, a.span(), &a.elements);
        types::array(self, a.elements.as_slice());
    }
    fn walk_in_legacy_array(&mut self, a: &'ast LegacyArray<'arena>, (): &mut ()) {
        arrays::literal_array(self, a.span(), &a.elements);
        types::array(self, a.elements.as_slice());
    }
    fn walk_in_for(&mut self, f: &'ast For<'arena>, (): &mut ()) {
        self.syntax.parents.push(syntax::Parent::Loop);
        for e in f.initializations.iter().chain(f.increments.iter()) {
            self.mark_top_level(e);
        }
    }
    fn walk_in_trait_use(&mut self, u: &'ast mago_syntax::cst::TraitUse<'arena>, (): &mut ()) {
        phpdoc_tags::clause(self, u.span());
        hierarchy::trait_use(self, u);
        generics::trait_use(self, u);
    }
    fn walk_in_use(&mut self, u: &'ast Use<'arena>, (): &mut ()) {
        names::use_functions(self, u);
        names::use_classes(self, u);
        signatures::use_statement(self, u);
        class_names::use_statement(self, u);
    }
    fn walk_in_property(&mut self, p: &'ast Property<'arena>, (): &mut ()) {
        self.visit_in_property(p);
    }
    fn walk_in_class_like_constant(&mut self, k: &'ast ClassLikeConstant<'arena>, (): &mut ()) {
        phpdoc_tags::node(self, k.span());
        invalid_throws::node(self, k.span());
        declarations::constant(self, k);
        constants::class_constant(self, k);
        missing_types::class_constant(self, k);
        type_coverage::constant(self, k);
        phpdoc_types::class_constant(self, k);
        types::class_constant(self, k);
        attributes::check(
            self,
            k.attribute_lists.as_slice(),
            attributes::TARGET_CLASS_CONSTANT,
            "class constant",
        );
    }
    fn walk_in_enum_case(&mut self, case: &'ast EnumCase<'arena>, (): &mut ()) {
        phpdoc_tags::clause(self, case.span());
        enums::case_outside(self, case);
        attributes::check(
            self,
            case.attribute_lists.as_slice(),
            attributes::TARGET_CLASS_CONSTANT,
            "class constant",
        );
    }
    fn walk_in_constant(&mut self, k: &'ast Constant<'arena>, (): &mut ()) {
        attributes::constant(self, k);
        types::global_constant(self, k);
    }
    fn walk_in_closure(&mut self, f: &'ast Closure<'arena>, (): &mut ()) {
        self.visit_in_closure(f);
    }
    fn walk_in_class_constant_access(&mut self, a: &'ast ClassConstantAccess<'arena>, (): &mut ()) {
        through_static::constant(self, a);
        names::class_constant(self, a);
        if let Some(name) = types::dynamic_names::constant(&a.constant) {
            types::dynamic_names::check(
                self,
                name,
                false,
                "Class constant",
                a.class,
                false,
                "classConstant.nameNotString",
            );
        }
        dynamic_fetch::class_constant(self, a);
        internal_usage::class_constant(self, a);
    }
    fn walk_in_try(&mut self, t: &'ast Try<'arena>, (): &mut ()) {
        dead_code::try_statement(self, t);
        types::dead_catches(self, t);
        types::unthrown_catches(self, t);
    }
    fn walk_in_try_catch_clause(&mut self, clause: &'ast TryCatchClause<'arena>, (): &mut ()) {
        phpdoc_tags::clause(self, clause.span());
        statements::noncapturing_catch(self, clause);
        names::catch(self, clause);
        assign::catch(self, clause);
    }
    fn walk_in_arrow_function(&mut self, f: &'ast ArrowFunction<'arena>, (): &mut ()) {
        self.visit_in_arrow_function(f);
    }
    fn walk_in_program(&mut self, program: &'ast Program<'arena>, (): &mut ()) {
        self.walk.set(
            Walk::COMPILER_HALT,
            names::file_has_compiler_halt_statement_calls(program),
        );
        syntax::used_names(self, program);
        self.unreachable = dead_code::unreachable_statements(self, program);
        self.conditions = dead_code::condition_facts(self, program);
        if self
            .toggled
            .on
            .contains(crate::rules::driver::Toggles::BY_REF_FOREACH)
        {
            self.by_ref_foreach = by_ref_foreach::entries(program);
        }
        self.push_frame(FrameKind::Top);
        types::yields_outside(self, program);
    }
    fn walk_out_program(&mut self, program: &'ast Program<'arena>, (): &mut ()) {
        let end = mago_span::Position::new(program.source_text.len() as u32);
        phpdoc_tags::nop(self, Span::new(program.file_id, end, end));
        self.pop_frame();
        disallowed::check(self, program);
    }
    fn walk_in_namespace(&mut self, n: &'ast Namespace<'arena>, (): &mut ()) {
        let name = n.name.as_ref().map(|id| self.resolved(id));
        signatures::enter_namespace(self, name.as_deref());
        class_names::enter_namespace(self);
        self.namespaces.push(name);
    }
    fn walk_out_namespace(&mut self, _: &'ast Namespace<'arena>, (): &mut ()) {
        signatures::leave_namespace(self);
        self.namespaces.pop();
    }
    fn walk_in_property_hook(&mut self, h: &'ast PropertyHook<'arena>, (): &mut ()) {
        self.visit_in_property_hook(h);
    }
    fn walk_out_property_hook(&mut self, _: &'ast PropertyHook<'arena>, (): &mut ()) {
        self.leave_this();
        self.syntax.by_ref.pop();
        self.pop_frame();
    }
    fn walk_in_function_like_parameter(
        &mut self,
        p: &'ast FunctionLikeParameter<'arena>,
        (): &mut (),
    ) {
        functions::parameter(self, p);
        statements::parameter_default(self, p);
        attributes::parameter(self, p);
        if !p.modifiers.is_empty() {
            attributes::property(self, p.span(), p.attribute_lists.as_slice(), true);
        }
        self.push_frame(FrameKind::Param);
    }
    fn walk_out_function_like_parameter(
        &mut self,
        _: &'ast FunctionLikeParameter<'arena>,
        (): &mut (),
    ) {
        self.pop_frame();
    }
    fn walk_statement(&mut self, s: &'ast Statement<'arena>, context: &mut ()) {
        if self.unreachable.reported.contains(&s.span()) {
            self.report(
                s,
                "deadCode.unreachable",
                "Unreachable statement - code above always terminates.".into(),
                None,
            );
        }
        if self.unreachable.nodes.contains(&s.span()) {
            invalid_throws::node(self, s.span());
            var_tags::node(self, s.span());
        }
        if !self.unreachable.skipped.contains(&s.span()) {
            mago_syntax::walker::walk_statement_mut(self, s, context);
        }
    }
    fn walk_in_statement(&mut self, s: &'ast Statement<'arena>, (): &mut ()) {
        invalid_throws::statement(self, s);
        phpdoc_tags::statement(self, s);
        statements::statement_position(self, s);
    }
    fn walk_in_declare(&mut self, d: &'ast Declare<'arena>, (): &mut ()) {
        statements::declare(self, d);
    }
    fn walk_in_goto(&mut self, g: &'ast Goto<'arena>, (): &mut ()) {
        self.goto(g);
    }
    fn walk_in_label(&mut self, l: &'ast Label<'arena>, (): &mut ()) {
        self.label(l);
    }
    fn walk_in_constant_access(&mut self, a: &'ast ConstantAccess<'arena>, (): &mut ()) {
        deprecations::constant_fetch(self, a.span(), &a.name);
        names::constant_fetch(self, &a.name);
    }
    fn walk_in_magic_constant(&mut self, m: &'ast MagicConstant<'arena>, (): &mut ()) {
        statements::magic_constant(self, m);
    }
    fn walk_in_echo(&mut self, e: &'ast Echo<'arena>, (): &mut ()) {
        types::echo(self, e);
    }
    fn walk_in_print_construct(&mut self, p: &'ast PrintConstruct<'arena>, (): &mut ()) {
        types::print(self, p);
    }
    fn walk_in_interpolated_string(&mut self, s: &'ast InterpolatedString<'arena>, (): &mut ()) {
        types::interpolated(self, s.parts.as_slice());
    }
    fn walk_in_document_string(&mut self, s: &'ast DocumentString<'arena>, (): &mut ()) {
        types::interpolated(self, s.parts.as_slice());
    }
    fn walk_in_unary_prefix(&mut self, u: &'ast UnaryPrefix<'arena>, (): &mut ()) {
        self.visit_in_unary_prefix(u);
    }
    fn walk_in_unary_postfix(&mut self, u: &'ast UnaryPostfix<'arena>, (): &mut ()) {
        types::postfix(self, u);
        operators::unary_postfix(self, u);
        assign::increment(self, u.operand);
        self.incremented(u.operand);
        self.invalidate_expression(u.operand);
        types::property_increment(self, u.operand, u.operator.span(), u.span());
    }
    fn walk_out_for(&mut self, _: &'ast For<'arena>, (): &mut ()) {
        self.syntax.parents.pop();
    }
    fn walk_in_foreach(&mut self, f: &'ast Foreach<'arena>, (): &mut ()) {
        self.visit_in_foreach(f);
    }
    fn walk_out_foreach(&mut self, _: &'ast Foreach<'arena>, (): &mut ()) {
        self.syntax.parents.pop();
    }
    fn walk_while(&mut self, w: &'ast While<'arena>, c: &mut ()) {
        self.visit_while(w, c);
    }
    fn walk_in_do_while(&mut self, d: &'ast DoWhile<'arena>, (): &mut ()) {
        dead_code::do_while(self, d);
        if self
            .strict
            .contains(crate::StrictRules::BOOLEANS_IN_LOOP_CONDITIONS)
        {
            operators::condition(
                self,
                d.span(),
                d.condition,
                "a do-while condition",
                "doWhile.condNotBoolean",
            );
        }
        self.syntax.parents.push(syntax::Parent::Loop);
    }
    fn walk_out_do_while(&mut self, _: &'ast DoWhile<'arena>, (): &mut ()) {
        self.syntax.parents.pop();
    }
    fn walk_in_switch(&mut self, s: &'ast Switch<'arena>, (): &mut ()) {
        dead_code::switch_statement(self, s);
        strict::switch(self, s);
        self.syntax.parents.push(syntax::Parent::Loop);
    }
    fn walk_out_switch(&mut self, _: &'ast Switch<'arena>, (): &mut ()) {
        self.syntax.parents.pop();
    }
    fn walk_in_break(&mut self, b: &'ast Break<'arena>, (): &mut ()) {
        syntax::continue_break(self, b, b.level, "break");
    }
    fn walk_in_continue(&mut self, k: &'ast Continue<'arena>, (): &mut ()) {
        syntax::continue_break(self, k, k.level, "continue");
    }
    fn walk_in_global(&mut self, g: &'ast Global<'arena>, (): &mut ()) {
        syntax::global(self, g);
    }
    fn walk_in_static(&mut self, s: &'ast Static<'arena>, (): &mut ()) {
        syntax::r#static(self, s);
    }
    fn walk_in_return(&mut self, r: &'ast Return<'arena>, (): &mut ()) {
        syntax::r#return(self, r);
    }
    fn walk_in_unset(&mut self, u: &'ast Unset<'arena>, (): &mut ()) {
        assign::unset(self, u);
        types::property_unset(self, u);
        unset::check(self, u);
        for value in &u.values {
            self.invalidate_expression(value);
        }
    }
    fn walk_in_array_append(&mut self, a: &'ast ArrayAppend<'arena>, (): &mut ()) {
        assign::array_append(self, a);
        types::dim_fetch(self, a.span(), a.array, None);
    }
    fn walk_in_shell_execute_string(&mut self, s: &'ast ShellExecuteString<'arena>, (): &mut ()) {
        statements::backtick(self, s);
    }
}
