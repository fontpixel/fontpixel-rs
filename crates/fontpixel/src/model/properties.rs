//! Ordered source properties are a sequence rather than a map: duplicate keys,
//! original case, unknown fields, flags and raw byte records must all survive.

/// Typed property value. Decimal text avoids lossy float parsing; raw records
/// can preserve exact spelling/quoting independently of this semantic value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum PropertyValue {
    /// Signed integer.
    Integer(i64),
    /// Unsigned integer (e.g. source fields above i64::MAX).
    Unsigned(u64),
    /// Exact decimal representation, interpreted by its owning format.
    Decimal(String),
    /// Text content, with source quoting removed when appropriate.
    Text(String),
    /// Opaque binary field.
    Bytes(Vec<u8>),
    /// Key-only property (BDF's None/null value).
    Flag,
    /// Ordered compound value, such as comments or numeric vectors.
    List(Vec<PropertyValue>),
}
impl From<String> for PropertyValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for PropertyValue {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}
impl From<i64> for PropertyValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

/// One original-cased source property.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Property {
    /// Exact source key spelling.
    pub name: String,
    /// Interpreted value.
    pub value: PropertyValue,
    /// Optional original record bytes, including delimiters/line ending if needed.
    /// This is provenance; writers must not blindly reuse it after model edits.
    #[cfg_attr(
        any(feature = "yaff", feature = "image"),
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub raw: Option<Vec<u8>>,
}
impl Property {
    /// Construct a newly authored property without stale source bytes.
    pub fn new(name: impl Into<String>, value: impl Into<PropertyValue>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            raw: None,
        }
    }
}
/// Insertion-ordered properties preserving duplicates; getters return the last match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "yaff", feature = "image"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Properties(Vec<Property>);
impl Properties {
    /// Number of records, including repeated names.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Whether no source records exist.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// All records in original order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Property> + ExactSizeIterator {
        self.0.iter()
    }
    /// Append without coalescing case variants or repeated names.
    pub fn push(&mut self, property: Property) {
        self.0.push(property);
    }
    /// Last exact-case match.
    pub fn get(&self, name: &str) -> Option<&PropertyValue> {
        self.0
            .iter()
            .rev()
            .find(|p| p.name == name)
            .map(|p| &p.value)
    }
    /// Last ASCII-case-insensitive match (BDF property convention).
    pub fn get_case_insensitive(&self, name: &str) -> Option<&PropertyValue> {
        self.0
            .iter()
            .rev()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| &p.value)
    }
    /// Update the last case-insensitive match in place, preserving its spelling and
    /// order, clearing stale raw bytes; append with the given spelling if new.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<PropertyValue>) {
        let name = name.into();
        let value = value.into();
        if let Some(property) = self
            .0
            .iter_mut()
            .rev()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
        {
            property.value = value;
            property.raw = None;
        } else {
            self.push(Property::new(name, value));
        }
    }
    /// Remove all case-insensitive matches. Returns how many records were removed.
    pub fn remove(&mut self, name: &str) -> usize {
        let before = self.0.len();
        self.0.retain(|p| !p.name.eq_ignore_ascii_case(name));
        before - self.0.len()
    }
    /// Consume the sequence without reordering.
    pub fn into_vec(self) -> Vec<Property> {
        self.0
    }
}
impl From<Vec<Property>> for Properties {
    fn from(properties: Vec<Property>) -> Self {
        Self(properties)
    }
}
impl FromIterator<Property> for Properties {
    fn from_iter<T: IntoIterator<Item = Property>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}
