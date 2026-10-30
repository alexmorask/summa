//! How a tag is written to, and read back from, the `entry_tags` table.

use crate::tag::Tag;

/// A [`Tag`] that can be stored: it turns into a `(kind, value)` pair of text
/// and back. A kind is the string [`Tag::kind`] returns; the value is the text
/// the business gives its payload, such as a decimal customer ID.
///
/// The round-trip law: for every tag `t`, `from_stored(t.kind(), &t.value())`
/// must return `Some(t)`. Storage relies on it to read back what it wrote.
///
/// `from_stored` returns `None` for a kind it doesn't know or a value it
/// can't parse. Loading treats that as corrupt data and fails; it never
/// guesses or skips the tag.
pub trait StoredTag: Tag {
    /// The tag's value as text, without its kind.
    fn value(&self) -> String;

    /// Rebuilds a tag from its stored kind and value, or `None` if the kind is
    /// unknown or the value doesn't parse.
    fn from_stored(kind: &str, value: &str) -> Option<Self>;
}
