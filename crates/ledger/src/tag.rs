//! Tags: labels on entries saying who or what each entry is about, like
//! customer 42. The ledger defines what a tag is; the business supplies its
//! kinds (ADR 0005).

use std::fmt::Debug;
use std::hash::Hash;

/// A tag on an entry. The business implements this on its own enum: one
/// variant per tag kind, each carrying a typed value. A misspelled kind is a
/// variant that doesn't exist, so it doesn't compile.
///
/// `Hash` lets balances be grouped by tag.
///
/// ```
/// use ledger::Tag;
///
/// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// enum ExampleTag {
///     Customer(u64),
///     Project(u64),
/// }
///
/// impl Tag for ExampleTag {
///     fn kind(&self) -> &'static str {
///         match self {
///             ExampleTag::Customer(_) => "customer",
///             ExampleTag::Project(_) => "project",
///         }
///     }
/// }
///
/// let tag = ExampleTag::Customer(42);
/// assert_eq!(tag.kind(), "customer");
/// ```
///
/// The same code with the kind misspelled doesn't compile:
///
/// ```compile_fail
/// use ledger::Tag;
///
/// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// enum ExampleTag {
///     Customer(u64),
///     Project(u64),
/// }
///
/// impl Tag for ExampleTag {
///     fn kind(&self) -> &'static str {
///         match self {
///             ExampleTag::Customer(_) => "customer",
///             ExampleTag::Project(_) => "project",
///         }
///     }
/// }
///
/// let tag = ExampleTag::Custmer(42);
/// assert_eq!(tag.kind(), "customer");
/// ```
pub trait Tag: Clone + Eq + Hash + Debug {
    /// The tag's kind, like `"customer"`. Every value of one variant returns
    /// the same kind, and different variants return different kinds.
    fn kind(&self) -> &'static str;
}
