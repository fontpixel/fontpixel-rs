//! Serde helpers for four-byte OpenType tags.
//!
//! A tag is a four-character string whose characters are the tag bytes read as
//! Latin-1 (U+0000..=U+00FF), so `liga` stays `"liga"` and any byte value round
//! trips. Deserialization rejects any other length or character. Map keys use
//! the same string form because JSON object keys must be strings.
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};

type Tag = [u8; 4];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TagString(Tag);

impl Serialize for TagString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text: String = self.0.iter().map(|&b| char::from(b)).collect();
        serializer.serialize_str(&text)
    }
}

impl<'de> Deserialize<'de> for TagString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let mut tag = [0u8; 4];
        let mut count = 0;
        for c in text.chars() {
            let byte = u8::try_from(u32::from(c)).map_err(|_| {
                D::Error::custom(format!("tag '{text}' has a character above U+00FF"))
            })?;
            if count == 4 {
                return Err(D::Error::custom(format!(
                    "tag '{text}' must be four characters"
                )));
            }
            tag[count] = byte;
            count += 1;
        }
        if count != 4 {
            return Err(D::Error::custom(format!(
                "tag '{text}' must be four characters"
            )));
        }
        Ok(Self(tag))
    }
}

/// One tag field.
pub(crate) mod tag {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(tag: &Tag, serializer: S) -> Result<S::Ok, S::Error> {
        TagString(*tag).serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Tag, D::Error> {
        Ok(TagString::deserialize(deserializer)?.0)
    }
}

/// A list of tags, in order.
pub(crate) mod list {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(tags: &[Tag], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(tags.iter().map(|&t| TagString(t)))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Tag>, D::Error> {
        Ok(Vec::<TagString>::deserialize(deserializer)?
            .into_iter()
            .map(|t| t.0)
            .collect())
    }
}

/// An ordered set of tags.
pub(crate) mod set {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(
        tags: &BTreeSet<Tag>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(tags.iter().map(|&t| TagString(t)))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeSet<Tag>, D::Error> {
        Ok(BTreeSet::<TagString>::deserialize(deserializer)?
            .into_iter()
            .map(|t| t.0)
            .collect())
    }
}

/// A tag-keyed map, serialized as an object with tag-string keys.
pub(crate) mod map {
    use super::*;

    pub(crate) fn serialize<S: Serializer, V: Serialize>(
        map: &BTreeMap<Tag, V>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_map(map.iter().map(|(&k, v)| (TagString(k), v)))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<Tag, V>, D::Error> {
        Ok(BTreeMap::<TagString, V>::deserialize(deserializer)?
            .into_iter()
            .map(|(k, v)| (k.0, v))
            .collect())
    }
}
