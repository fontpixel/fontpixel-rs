//! Text representations for core option values in serialized requests.

/// `lr`/`rl`/`tb`/`bt` shortcuts or four-letter pairs such as `lrtb`.
pub(crate) mod direction {
    use fontpixel::bitmap::Direction;
    use fontpixel::render::WritingDirection;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    fn pair(direction: Direction) -> &'static str {
        match direction {
            Direction::Right => "lr",
            Direction::Left => "rl",
            Direction::Down => "tb",
            Direction::Up => "bt",
        }
    }
    pub(crate) fn serialize<S: Serializer>(
        value: &WritingDirection,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}{}", pair(value.glyphs), pair(value.lines)))
    }
    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<WritingDirection, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(D::Error::custom)
    }
}

/// `#RRGGBBAA`, accepting every `--color` spelling when deserializing.
pub(crate) mod rgba {
    use fontpixel::render::Rgba;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    pub(crate) fn serialize<S: Serializer>(value: &Rgba, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            value.red, value.green, value.blue, value.alpha
        ))
    }
    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Rgba, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(D::Error::custom)
    }
}

/// A merge selection expression such as `cjk,western`.
pub(crate) mod selection {
    use fontpixel::merge::Selection;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    pub(crate) fn serialize<S: Serializer>(
        value: &Selection,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }
    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Selection, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(D::Error::custom)
    }
}
