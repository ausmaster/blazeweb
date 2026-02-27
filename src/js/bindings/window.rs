/// Window/global setup.
///
/// Installs document, window, self, and console on the global object.

use crate::js::templates::{arena_ref, wrap_node};

/// Install global objects on the context's global object.
/// Must be called after context creation (inside ContextScope).
pub fn install_globals(scope: &mut v8::HandleScope) {
    let context = scope.get_current_context();
    let global = context.global(scope);

    // document = the arena's Document node
    let arena = arena_ref(scope);
    let doc_id = arena.document;
    let doc_obj = wrap_node(scope, doc_id);
    let key = v8::String::new(scope, "document").unwrap();
    global.set(scope, key.into(), doc_obj.into());

    // window = globalThis (self-reference)
    let key = v8::String::new(scope, "window").unwrap();
    global.set(scope, key.into(), global.into());

    // self = globalThis
    let key = v8::String::new(scope, "self").unwrap();
    global.set(scope, key.into(), global.into());

    // console
    super::console::install(scope, global);
}
