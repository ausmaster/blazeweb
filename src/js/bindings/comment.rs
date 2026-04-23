/// Comment prototype bindings.
///
/// Inherits all CharacterData properties and methods (data, length,
/// substringData, appendData, insertData, deleteData, replaceData,
/// before, after, replaceWith, remove, previousElementSibling,
/// nextElementSibling).
///
/// Ported from Servo's `components/script/dom/comment.rs` — Comment has
/// no additional methods beyond CharacterData.

pub fn install(_scope: &mut v8::PinnedRef<v8::HandleScope<()>>, _proto: &v8::Local<v8::ObjectTemplate>) {
    // CharacterData methods are now inherited via FunctionTemplate chain
    // (Comment → CharacterData → Node). Comment has no additional methods.
}
