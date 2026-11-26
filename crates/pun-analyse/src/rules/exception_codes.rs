use mago_analyzer::plugin::context::{InvocationInfo, ProviderContext};
use mago_analyzer::plugin::provider::method::{MethodReturnTypeProvider, MethodTarget};
use mago_analyzer::plugin::provider::{Provider, ProviderMeta};
use mago_codex::ttype::union::TUnion;
use mago_codex::ttype::{get_int, get_int_or_string};

static META: ProviderMeta = ProviderMeta::new(
    "pun-exception-codes",
    "pun",
    "PHPStan's Throwable::getCode() return type",
);

/// PHPStan's ThrowableReturnTypeExtension: a PDOException's code may be its SQLSTATE string, and so may the code of a
/// bare Throwable, Exception or RuntimeException, which could be one; those are the benevolent `(int|string)`. Any
/// other throwable's code is an `int`. A class's own `@method getCode()` tag wins, which Pun resolves to the tag's
/// pseudo method before asking this provider.
pub(super) struct ExceptionCodes;

impl Provider for ExceptionCodes {
    fn meta() -> &'static ProviderMeta {
        &META
    }
}

impl MethodReturnTypeProvider for ExceptionCodes {
    fn targets() -> &'static [MethodTarget] {
        static TARGETS: [MethodTarget; 1] = [MethodTarget::any_class(b"getcode")];
        &TARGETS
    }

    fn get_return_type(
        &self,
        context: &ProviderContext<'_, '_, '_>,
        _: &[u8],
        _: &[u8],
        invocation: &InvocationInfo<'_, '_, '_>,
    ) -> Option<TUnion> {
        let class = (invocation.inner().target.get_method_context()?).class_like_metadata;
        let codebase = context.codebase();
        let name = class.name.as_bytes();
        if !codebase.is_instance_of(name, b"Throwable") {
            return None;
        }
        if codebase.is_instance_of(name, b"PDOException")
            || [&b"throwable"[..], b"exception", b"runtimeexception"]
                .iter()
                .any(|base| name.eq_ignore_ascii_case(base))
        {
            return Some(super::return_types::benevolent(get_int_or_string()));
        }
        Some(get_int())
    }
}
