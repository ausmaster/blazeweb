pub mod bindings;
pub mod events;
pub mod executor;
pub mod fetch;
pub mod modules;
pub mod mutation_observer;
pub mod runtime;
pub mod templates;
pub mod timers;

/// Create a pinned `v8::TryCatch` scope, mirroring the `v8::scope!` macro.
///
/// In v8 147 `TryCatch::new` returns `ScopeStorage<TryCatch>` which must be
/// pinned and initialized before use. This macro encapsulates the unsafe
/// Pin+init dance so call sites remain readable.
///
/// Usage:
/// ```rust,ignore
/// crate::js::try_catch!(let tc, scope);
/// match something.compile(tc) { ... }
/// ```
#[macro_export]
macro_rules! try_catch {
    (let $name:ident, $parent:expr $(,)?) => {
        let mut $name = v8::TryCatch::new($parent);
        #[allow(unused_mut)]
        let mut $name = {
            let pinned = unsafe { std::pin::Pin::new_unchecked(&mut $name) };
            pinned.init()
        };
        let $name = &mut $name;
    };
}
