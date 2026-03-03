/// Comment prototype bindings.
///
/// Inherits all CharacterData properties and methods (data, length,
/// substringData, appendData, insertData, deleteData, replaceData,
/// before, after, replaceWith, remove, previousElementSibling,
/// nextElementSibling).
///
/// Ported from Servo's `components/script/dom/comment.rs` — Comment has
/// no additional methods beyond CharacterData.

pub fn install(scope: &mut v8::HandleScope<()>, proto: &v8::Local<v8::ObjectTemplate>) {
    super::characterdata::install(scope, proto);
}
