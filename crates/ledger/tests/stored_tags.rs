//! Tags survive being turned into stored text and back.

mod common;

use common::{CustomerId, PlaygroundTag, SubscriptionId};
use ledger::{StoredTag, Tag};

#[test]
fn every_playground_tag_round_trips() {
    for tag in [
        PlaygroundTag::Customer(CustomerId(42)),
        PlaygroundTag::Subscription(SubscriptionId(u64::MAX)),
    ] {
        assert_eq!(
            PlaygroundTag::from_stored(tag.kind(), &tag.value()),
            Some(tag)
        );
    }
}

/// A tag carrying free text, to prove separators and non-ASCII text survive.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TextTag {
    Note(String),
}

impl Tag for TextTag {
    fn kind(&self) -> &'static str {
        match self {
            TextTag::Note(_) => "note",
        }
    }
}

impl StoredTag for TextTag {
    fn value(&self) -> String {
        match self {
            TextTag::Note(text) => text.clone(),
        }
    }

    fn from_stored(kind: &str, value: &str) -> Option<Self> {
        (kind == "note").then(|| TextTag::Note(value.to_string()))
    }
}

#[test]
fn tag_values_with_separators_round_trip() {
    for text in ["a:b", "é", " "] {
        let tag = TextTag::Note(text.to_string());
        assert_eq!(TextTag::from_stored(tag.kind(), &tag.value()), Some(tag));
    }
}

#[test]
fn unknown_kind_is_none() {
    assert_eq!(PlaygroundTag::from_stored("invoice", "1"), None);
}

#[test]
fn unparseable_value_is_none() {
    assert_eq!(PlaygroundTag::from_stored("customer", "abc"), None);
    assert_eq!(PlaygroundTag::from_stored("customer", "-1"), None);
}
