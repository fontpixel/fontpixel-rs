//! Strict deserialization of flat requests and option groups.
//!
//! Requests serialize as one flat map: every option group is flattened, so the
//! keys are the command-line option names in snake_case. serde cannot combine
//! `deny_unknown_fields` with `flatten`, so each request and group derives its
//! field-by-field code as inherent functions (`serde(remote = "Self")`) and
//! implements the serde traits through [`strict!`]:
//!
//! - `Deserialize` buffers the input map, rejects any key that is not a field
//!   of the type or of one of its flattened groups (the key set is the
//!   serialized form of `Default`), deserializes the fields and then runs the
//!   type's `validate()`, so choice lists, numeric ranges and the relational
//!   command-line rules are checked on deserialization.
//! - A flattened group field uses [`flattened`], which skips both checks: the
//!   containing type checks keys against its complete flat key set and its own
//!   `validate()` covers the group.
//! - `Serialize` forwards to the derived code.

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// Lenient deserialization of an option group inside a containing request.
pub trait Flattened: Sized {
    /// Deserialize the group's fields without the key check or validation.
    fn deserialize_flattened<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;
}
impl<T: Flattened> Flattened for Box<T> {
    fn deserialize_flattened<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize_flattened(deserializer).map(Box::new)
    }
}

/// `deserialize_with` target for flattened group fields.
pub(crate) fn flattened<'de, D: Deserializer<'de>, T: Flattened>(
    deserializer: D,
) -> Result<T, D::Error> {
    T::deserialize_flattened(deserializer)
}

/// Every key the flat serialized form of `T::default()` contains.
fn known_keys<T: Serialize + Default>() -> BTreeSet<String> {
    match serde_json::to_value(T::default()) {
        Ok(serde_json::Value::Object(map)) => map.into_iter().map(|(key, _)| key).collect(),
        _ => BTreeSet::new(),
    }
}

/// Buffer a map, reject unknown keys, build the value and validate it.
pub(crate) fn strict_map<'de, D, T>(
    deserializer: D,
    keys: &'static OnceLock<BTreeSet<String>>,
    build: impl FnOnce(serde_json::Value) -> Result<T, serde_json::Error>,
    validate: impl FnOnce(&T) -> fontpixel::Result<()>,
) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Serialize + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let serde_json::Value::Object(map) = &value else {
        return Err(D::Error::custom("expected a map of option names to values"));
    };
    let known = keys.get_or_init(known_keys::<T>);
    if let Some(key) = map.keys().find(|key| !known.contains(*key)) {
        return Err(D::Error::custom(format!("unknown field `{key}`")));
    }
    let built = build(value).map_err(D::Error::custom)?;
    validate(&built).map_err(D::Error::custom)?;
    Ok(built)
}

/// Implement `Serialize`, strict `Deserialize` and [`Flattened`] for a type
/// whose serde derive uses `remote = "Self"` and that has `Default` and
/// `validate()`.
macro_rules! strict {
    ($ty:ident) => {
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                $ty::serialize(self, serializer)
            }
        }
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> ::core::result::Result<Self, D::Error> {
                static KEYS: std::sync::OnceLock<std::collections::BTreeSet<String>> =
                    std::sync::OnceLock::new();
                $crate::serde_keys::strict_map(
                    deserializer,
                    &KEYS,
                    |value| $ty::deserialize(value),
                    $ty::validate,
                )
            }
        }
        impl $crate::serde_keys::Flattened for $ty {
            fn deserialize_flattened<'de, D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> ::core::result::Result<Self, D::Error> {
                $ty::deserialize(deserializer)
            }
        }
    };
}
pub(crate) use strict;
