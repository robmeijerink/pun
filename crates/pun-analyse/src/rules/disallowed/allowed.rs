//! `Allowed\Allowed::isAllowed` and `src/Params/*`: whether an entry's allow directives allow a use where it is.

use pun_phpstan::types::Type;
use pun_tokens::php_compat;

use super::{Arg, Entry, Param, ParamKind, Position, Site, Walker, matches};

impl Walker<'_, '_, '_> {
    /// `Allowed::isAllowed`; `args` is `None` where the use takes no arguments (or is a first-class callable).
    pub(super) fn is_allowed(
        &self,
        site: &Site,
        args: Option<&[Arg]>,
        e: &Entry,
        has_params: bool,
        position: Option<Position>,
    ) -> bool {
        let a = &e.allowed;
        let in_allowed = |default: bool| !has_params || self.params_in_allowed(args, e, default);
        if a.allow_in_calls
            .iter()
            .any(|call| self.call_matches(site, call))
        {
            return in_allowed(true);
        }
        if !a.allow_except_in_calls.is_empty() {
            if a.allow_except_in_calls
                .iter()
                .any(|call| self.call_matches(site, call))
            {
                return has_params && self.params_in_allowed(args, e, false);
            }
            return true;
        }
        if a.allow_in.iter().any(|p| self.path_matches(p)) {
            return in_allowed(true);
        }
        if !a.allow_except_in.is_empty() {
            if a.allow_except_in.iter().any(|p| self.path_matches(p)) {
                return has_params && self.params_in_allowed(args, e, false);
            }
            return true;
        }
        if self.type_hints(e) {
            match position {
                Some(Position::Param) if a.in_param_types => return true,
                Some(Position::Return) if a.in_return_type => return true,
                Some(Position::Param) if a.except_in_param_types => return false,
                Some(Position::Return) if a.except_in_return_type => return false,
                _ => {}
            }
            if a.except_in_param_types || a.except_in_return_type {
                return true;
            }
        }
        if !a.allow_in_instance_of.is_empty() && self.is_instance_of(&a.allow_in_instance_of) {
            return in_allowed(true);
        }
        if !a.allow_except_in_instance_of.is_empty() {
            if !self.is_instance_of(&a.allow_except_in_instance_of) {
                return true;
            }
            return has_params && self.params_in_allowed(args, e, false);
        }
        if has_params && !a.except_params.is_empty() {
            return self.allowed_params(args, &a.except_params, false);
        }
        if has_params
            && !a.params_anywhere.is_empty()
            && self.allowed_params(args, &a.params_anywhere, true)
        {
            return true;
        }
        let class_attributes = || self.class_attributes();
        if !a.allow_in_class_with_attributes.is_empty()
            && has_attribute(&class_attributes(), &a.allow_in_class_with_attributes)
        {
            return true;
        }
        if !a.allow_except_in_class_with_attributes.is_empty() {
            return !has_attribute(
                &class_attributes(),
                &a.allow_except_in_class_with_attributes,
            );
        }
        if !a.allow_in_calls_with_attributes.is_empty()
            && has_attribute(
                &self.call_attributes(site),
                &a.allow_in_calls_with_attributes,
            )
        {
            return true;
        }
        if !a.allow_except_in_calls_with_attributes.is_empty() {
            return !has_attribute(
                &self.call_attributes(site),
                &a.allow_except_in_calls_with_attributes,
            );
        }
        if !a.allow_in_class_with_method_attributes.is_empty()
            && has_attribute(
                &self.method_attributes(),
                &a.allow_in_class_with_method_attributes,
            )
        {
            return true;
        }
        if !a.allow_except_in_class_with_method_attributes.is_empty() {
            return !has_attribute(
                &self.method_attributes(),
                &a.allow_except_in_class_with_method_attributes,
            );
        }
        false
    }

    /// Namespace entries are `DisallowedWithTypeHints`: they read the type hint position directives.
    fn type_hints(&self, e: &Entry) -> bool {
        self.d
            .namespaces
            .iter()
            .chain(&self.d.classes)
            .any(|n| std::ptr::eq(n, e))
    }

    /// `Allowed::callMatches`.
    fn call_matches(&self, site: &Site, call: &str) -> bool {
        let name = (self.functions.last().map(|f| f.name.as_str()))
            .or(site.function.as_deref())
            .unwrap_or_default();
        matches(call, name, &[])
    }

    /// `AllowedPath::matches`: the trait's file inside a trait.
    fn path_matches(&self, path: &str) -> bool {
        self.d.path_matches(path, &self.c.file_name())
    }

    /// `Allowed::isInstanceOf`.
    fn is_instance_of(&self, patterns: &[String]) -> bool {
        let Some(k) = self.class() else {
            return false;
        };
        let fnmatch =
            |p: &str, n: &str| php_compat::fnmatch_flags(p.as_bytes(), n.as_bytes(), true, true);
        let names: Vec<String> = std::iter::once(&k.original_name)
            .chain(k.all_parent_classes.iter())
            .chain(k.all_parent_interfaces.iter())
            .map(|n| self.original_name(n.as_bytes()))
            .collect();
        patterns.iter().any(|p| names.iter().any(|n| fnmatch(p, n)))
    }

    pub(super) fn original_name(&self, name: &[u8]) -> String {
        (self.c.codebase.get_class_like(name)).map_or_else(
            || String::from_utf8_lossy(name).into_owned(),
            |k| String::from_utf8_lossy(k.original_name.as_bytes()).into_owned(),
        )
    }

    fn class_attributes(&self) -> Vec<String> {
        (self.class().into_iter())
            .flat_map(|k| &k.attributes)
            .map(|a| self.original_name(a.name.as_bytes()))
            .collect()
    }

    /// `Allowed::getCallAttributes`.
    fn call_attributes(&self, site: &Site) -> Vec<String> {
        match self.functions.last() {
            Some(f) => f.attributes.clone(),
            None => site.attributes.clone().unwrap_or_default(),
        }
    }

    /// `Allowed::getAllMethodAttributes`: the attributes of every method the class has.
    fn method_attributes(&self) -> Vec<String> {
        let Some(k) = self.class() else {
            return Vec::new();
        };
        (k.declaring_method_ids.values())
            .filter_map(|id| {
                self.c
                    .codebase
                    .function_likes
                    .get(&(id.get_class_name(), id.get_method_name()))
            })
            .flat_map(|f| &f.attributes)
            .chain(
                (k.properties.values())
                    .flat_map(|p| p.hooks.values())
                    .flat_map(|h| &h.attributes),
            )
            .map(|a| self.original_name(a.name.as_bytes()))
            .collect()
    }

    /// `Allowed::hasAllowedParamsInAllowed`.
    fn params_in_allowed(&self, args: Option<&[Arg]>, e: &Entry, default: bool) -> bool {
        let a = &e.allowed;
        if args.is_none() {
            if !a.except_params_in_allowed.is_empty() {
                return true;
            }
            if !a.params_in_allowed.is_empty() {
                return false;
            }
            return default;
        }
        if !a.except_params_in_allowed.is_empty() {
            return self.allowed_params(args, &a.except_params_in_allowed, false);
        }
        if !a.params_in_allowed.is_empty() {
            return self.allowed_params(args, &a.params_in_allowed, true);
        }
        default
    }

    /// `Allowed::hasAllowedParams`.
    fn allowed_params(
        &self,
        args: Option<&[Arg]>,
        config: &[(pun_phpstan::Key, Param)],
        required: bool,
    ) -> bool {
        let Some(args) = args else {
            return !required;
        };
        let mut disallowed = false;
        for (_, param) in config {
            let found = (args.iter())
                .find(|a| {
                    a.name.is_some()
                        && a.name.map(|n| String::from_utf8_lossy(n).into_owned()) == param.name
                })
                .or_else(|| {
                    let p = usize::try_from(param.position? - 1).ok()?;
                    args.get(p)
                });
            let Some(t) = found.and_then(|a| self.type_at(a.value)) else {
                return !required;
            };
            let types = match t {
                Type::Union(types) => types,
                t => vec![t],
            };
            for t in &types {
                match param_matches(&param.kind, t) {
                    Some(m) => disallowed = disallowed || !m,
                    None => return !required,
                }
            }
        }
        !disallowed
    }
}

/// `Allowed::hasAllowedAttribute`.
fn has_attribute(names: &[String], config: &[String]) -> bool {
    config
        .iter()
        .any(|c| names.iter().any(|n| matches(c, n, &[])))
}

/// `Param::matches`; `None` where it throws `UnsupportedParamTypeException`.
fn param_matches(kind: &ParamKind, t: &Type) -> Option<bool> {
    let flag_set = |config: &Type| match t {
        Type::ConstInt(v) => Some(
            super::constant_scalars(config)
                .iter()
                .any(|c| matches!(c, Type::ConstInt(c) if c & v != 0)),
        ),
        _ => None,
    };
    let class_matches = |pattern: &str| {
        (t.object_class_names().iter())
            .any(|n| php_compat::fnmatch_flags(pattern.as_bytes(), n.as_bytes(), true, true))
    };
    Some(match kind {
        ParamKind::Specific(c) => pun_phpstan::types::is_super_type_of(c, t).yes(),
        ParamKind::Except(c) => !pun_phpstan::types::is_super_type_of(c, t).yes(),
        ParamKind::Any => true,
        ParamKind::ExceptAny => false,
        ParamKind::FlagSpecific(c) => flag_set(c)?,
        ParamKind::FlagExcept(c) => !flag_set(c)?,
        ParamKind::CaseInsensitiveExcept(c) => {
            let lower = |t: &Type| -> Vec<Vec<u8>> {
                t.constant_strings()
                    .iter()
                    .map(|s| s.to_ascii_lowercase())
                    .collect()
            };
            let theirs = lower(c);
            !lower(t).iter().any(|s| theirs.contains(s))
        }
        ParamKind::ClassPattern(p) => class_matches(p),
        ParamKind::ClassPatternExcept(p) => !class_matches(p),
    })
}
