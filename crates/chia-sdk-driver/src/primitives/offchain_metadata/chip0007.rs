use num_bigint::BigInt;
use serde_json::{Map, Number, Value, json};
use thiserror::Error;

use crate::DriverError;

/// The `format` values which are accepted. CHIP-0007 only lists its own number, but the examples
/// published with its extension, CHIP-0015, all use `CHIP-0015`.
pub const CHIP0007_FORMATS: [&str; 2] = ["CHIP-0007", "CHIP-0015"];

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// The largest magnitude at which every integer is exactly representable by an `f64`.
const MAX_EXACT_FLOAT: f64 = 9_007_199_254_740_992.0;

/// A way in which metadata departs from the CHIP-0007 specification. The strict parsers reject
/// metadata with any issue, and the lenient parsers return them alongside the metadata.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Chip0007Issue {
    #[error("missing required field `{0}`")]
    Missing(String),

    #[error("ignored invalid field `{0}`")]
    Dropped(String),

    #[error("converted invalid field `{0}`")]
    Coerced(String),

    #[error("unsupported format `{0}`")]
    UnsupportedFormat(String),

    #[error("collection id `{0}` is not a uuid")]
    InvalidCollectionId(String),

    #[error("field `{0}` must be at least 1")]
    SeriesOutOfRange(String),

    #[error("integer `{0}` cannot be represented in JSON")]
    IntegerOutOfRange(BigInt),
}

/// An integer or string, which is what CHIP-0007 allows for attribute names and values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chip0007Value {
    Integer(BigInt),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chip0007SensitiveContent {
    Flag(bool),
    Items(Vec<String>),
}

impl Chip0007SensitiveContent {
    pub fn is_sensitive(&self) -> bool {
        match self {
            Self::Flag(flag) => *flag,
            Self::Items(items) => !items.is_empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip0007Attribute {
    pub trait_type: Chip0007Value,
    pub value: Chip0007Value,
    pub min_value: Option<BigInt>,
    pub max_value: Option<BigInt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip0007CollectionAttribute {
    pub kind: Chip0007Value,
    pub value: Chip0007Value,
}

/// Information about the collection an NFT claims to be part of.
///
/// This is not verified by the metadata, anyone can reuse a collection's id. It should be combined
/// with on-chain data such as the minter's DID before it's presented as authentic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip0007Collection {
    pub id: String,
    pub name: String,
    pub attributes: Vec<Chip0007CollectionAttribute>,
}

/// The off-chain metadata of an NFT, as defined by
/// [CHIP-0007](https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0007.md).
///
/// Parsing and reserializing doesn't preserve the original bytes, so the original file should be
/// kept to check against the metadata hash stored on-chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip0007Metadata {
    pub format: String,
    pub name: String,
    pub description: String,
    pub minting_tool: Option<String>,
    pub sensitive_content: Option<Chip0007SensitiveContent>,
    pub series_number: Option<u64>,
    pub series_total: Option<u64>,
    pub attributes: Vec<Chip0007Attribute>,
    pub collection: Option<Chip0007Collection>,
    pub data: Option<Map<String, Value>>,
}

/// The result of leniently parsing metadata, with everything that was wrong with it. Required
/// fields which couldn't be read are empty strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip0007ParseResult {
    pub metadata: Chip0007Metadata,
    pub issues: Vec<Chip0007Issue>,
}

impl Chip0007Metadata {
    pub fn new(name: String, description: String) -> Self {
        Self {
            format: CHIP0007_FORMATS[0].to_string(),
            name,
            description,
            minting_tool: None,
            sensitive_content: None,
            series_number: None,
            series_total: None,
            attributes: Vec::new(),
            collection: None,
            data: None,
        }
    }

    /// Parses metadata which must comply with the specification.
    ///
    /// Integers outside of the range of `i64` and `u64` are rejected, since they can't be read
    /// without losing precision.
    pub fn parse(json: &str) -> Result<Self, DriverError> {
        Self::from_bytes(json.as_bytes())
    }

    /// The same as [`Self::parse`], for the contents of a metadata file.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DriverError> {
        let result = Self::from_bytes_lenient(bytes)?;

        match result.issues.into_iter().next() {
            Some(issue) => Err(issue.into()),
            None => Ok(result.metadata),
        }
    }

    /// Parses metadata as well as it can be, which is what should be used for displaying NFTs
    /// since a lot of existing metadata doesn't comply with the specification. Anything that can't
    /// be read is ignored, so this only fails if the input isn't a JSON object.
    pub fn parse_lenient(json: &str) -> Result<Chip0007ParseResult, DriverError> {
        Self::from_bytes_lenient(json.as_bytes())
    }

    /// The same as [`Self::parse_lenient`], for the contents of a metadata file.
    pub fn from_bytes_lenient(bytes: &[u8]) -> Result<Chip0007ParseResult, DriverError> {
        let mut issues = Vec::new();

        let bytes = if let Some(bytes) = bytes.strip_prefix(UTF8_BOM) {
            issues.push(Chip0007Issue::Coerced("$".to_string()));
            bytes
        } else {
            bytes
        };

        let Value::Object(root) = serde_json::from_slice(bytes)? else {
            return Err(DriverError::InvalidChip0007Root);
        };

        let mut reader = Reader { issues };

        let metadata = Self {
            format: reader
                .required_string(&root, "format", "format")
                .unwrap_or_default(),
            name: reader
                .required_string(&root, "name", "name")
                .unwrap_or_default(),
            description: reader
                .required_string(&root, "description", "description")
                .unwrap_or_default(),
            minting_tool: reader.optional_string(&root, "minting_tool"),
            sensitive_content: reader.sensitive_content(&root),
            series_number: reader.series(&root, "series_number"),
            series_total: reader.series(&root, "series_total"),
            attributes: reader.attributes(&root),
            collection: reader.collection(&root),
            data: reader.data(&root),
        };

        let mut issues = reader.issues;

        for issue in metadata.validate() {
            if !issues.contains(&issue) {
                issues.push(issue);
            }
        }

        Ok(Chip0007ParseResult { metadata, issues })
    }

    /// Checks everything that isn't already guaranteed by the types.
    pub fn validate(&self) -> Vec<Chip0007Issue> {
        let mut issues = Vec::new();

        if self.format.is_empty() {
            issues.push(Chip0007Issue::Missing("format".to_string()));
        } else if !CHIP0007_FORMATS.contains(&self.format.as_str()) {
            issues.push(Chip0007Issue::UnsupportedFormat(self.format.clone()));
        }

        for (field, value) in [
            ("series_number", self.series_number),
            ("series_total", self.series_total),
        ] {
            if value == Some(0) {
                issues.push(Chip0007Issue::SeriesOutOfRange(field.to_string()));
            }
        }

        if let Some(collection) = &self.collection
            && !is_uuid(&collection.id)
        {
            issues.push(Chip0007Issue::InvalidCollectionId(collection.id.clone()));
        }

        issues
    }

    pub fn is_sensitive(&self) -> bool {
        self.sensitive_content
            .as_ref()
            .is_some_and(Chip0007SensitiveContent::is_sensitive)
    }

    /// Serializes the metadata to JSON. This doesn't reproduce the file it was parsed from.
    pub fn to_json(&self) -> Result<String, DriverError> {
        let mut root = Map::new();

        root.insert("format".to_string(), json!(self.format));
        root.insert("name".to_string(), json!(self.name));
        root.insert("description".to_string(), json!(self.description));

        if let Some(minting_tool) = &self.minting_tool {
            root.insert("minting_tool".to_string(), json!(minting_tool));
        }

        match &self.sensitive_content {
            Some(Chip0007SensitiveContent::Flag(flag)) => {
                root.insert("sensitive_content".to_string(), json!(flag));
            }
            Some(Chip0007SensitiveContent::Items(items)) => {
                root.insert("sensitive_content".to_string(), json!(items));
            }
            None => {}
        }

        if let Some(series_number) = self.series_number {
            root.insert("series_number".to_string(), json!(series_number));
        }

        if let Some(series_total) = self.series_total {
            root.insert("series_total".to_string(), json!(series_total));
        }

        if !self.attributes.is_empty() {
            let attributes = self
                .attributes
                .iter()
                .map(Chip0007Attribute::to_json)
                .collect::<Result<Vec<_>, _>>()?;

            root.insert("attributes".to_string(), Value::Array(attributes));
        }

        if let Some(collection) = &self.collection {
            root.insert("collection".to_string(), collection.to_json()?);
        }

        if let Some(data) = &self.data {
            root.insert("data".to_string(), Value::Object(data.clone()));
        }

        Ok(serde_json::to_string(&root)?)
    }
}

impl Chip0007Value {
    fn to_json(&self) -> Result<Value, Chip0007Issue> {
        match self {
            Self::Integer(value) => integer_to_json(value),
            Self::String(value) => Ok(json!(value)),
        }
    }
}

impl Chip0007Attribute {
    fn to_json(&self) -> Result<Value, Chip0007Issue> {
        let mut attribute = Map::new();

        attribute.insert("trait_type".to_string(), self.trait_type.to_json()?);
        attribute.insert("value".to_string(), self.value.to_json()?);

        if let Some(min_value) = &self.min_value {
            attribute.insert("min_value".to_string(), integer_to_json(min_value)?);
        }

        if let Some(max_value) = &self.max_value {
            attribute.insert("max_value".to_string(), integer_to_json(max_value)?);
        }

        Ok(Value::Object(attribute))
    }
}

impl Chip0007Collection {
    fn to_json(&self) -> Result<Value, Chip0007Issue> {
        let mut collection = Map::new();

        collection.insert("id".to_string(), json!(self.id));
        collection.insert("name".to_string(), json!(self.name));

        if !self.attributes.is_empty() {
            let attributes = self
                .attributes
                .iter()
                .map(|attribute| {
                    Ok(json!({
                        "type": attribute.kind.to_json()?,
                        "value": attribute.value.to_json()?,
                    }))
                })
                .collect::<Result<Vec<_>, Chip0007Issue>>()?;

            collection.insert("attributes".to_string(), Value::Array(attributes));
        }

        Ok(Value::Object(collection))
    }
}

fn integer_to_json(value: &BigInt) -> Result<Value, Chip0007Issue> {
    i64::try_from(value)
        .map(Number::from)
        .or_else(|_| u64::try_from(value).map(Number::from))
        .map(Value::Number)
        .map_err(|_| Chip0007Issue::IntegerOutOfRange(value.clone()))
}

/// JSON Schema considers numbers with a zero fractional part, such as `200.0`, to be integers.
/// Anything outside of the range `f64` can represent exactly is not read, to avoid changing it.
#[allow(clippy::cast_possible_truncation)]
pub(super) fn number_to_integer(value: &Number) -> Option<BigInt> {
    value
        .as_i64()
        .map(BigInt::from)
        .or_else(|| value.as_u64().map(BigInt::from))
        .or_else(|| {
            let value = value.as_f64()?;
            (value.fract() == 0.0 && value.abs() <= MAX_EXACT_FLOAT)
                .then(|| BigInt::from(value as i64))
        })
}

/// Whether the value is a uuid in the `8-4-4-4-12` hex format, which is what the JSON schema
/// published with CHIP-0007 requires of a collection id.
fn is_uuid(value: &str) -> bool {
    let groups = value.split('-').collect::<Vec<_>>();

    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Reads fields from JSON objects, keeping track of the issues it comes across.
struct Reader {
    issues: Vec<Chip0007Issue>,
}

impl Reader {
    fn dropped(&mut self, path: &str) {
        self.issues.push(Chip0007Issue::Dropped(path.to_string()));
    }

    fn coerced(&mut self, path: &str) {
        self.issues.push(Chip0007Issue::Coerced(path.to_string()));
    }

    fn required_string(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<String> {
        match object.get(key) {
            Some(Value::String(value)) => return Some(value.clone()),
            Some(Value::Null) | None => {
                self.issues.push(Chip0007Issue::Missing(path.to_string()));
            }
            Some(_) => self.dropped(path),
        }

        None
    }

    fn optional_string(&mut self, object: &Map<String, Value>, key: &str) -> Option<String> {
        if let Value::String(value) = object.get(key)? {
            Some(value.clone())
        } else {
            self.dropped(key);
            None
        }
    }

    fn sensitive_content(
        &mut self,
        object: &Map<String, Value>,
    ) -> Option<Chip0007SensitiveContent> {
        let key = "sensitive_content";

        match object.get(key)? {
            Value::Bool(flag) => Some(Chip0007SensitiveContent::Flag(*flag)),
            Value::Array(items) => {
                let mut strings = Vec::new();

                for (index, item) in items.iter().enumerate() {
                    if let Value::String(item) = item {
                        strings.push(item.clone());
                    } else {
                        self.dropped(&format!("{key}[{index}]"));
                    }
                }

                Some(Chip0007SensitiveContent::Items(strings))
            }
            // Any other string is taken as a type of sensitive content rather than being ignored,
            // since hiding an NFT by mistake is better than showing one by mistake.
            Value::String(value) => {
                self.coerced(key);

                Some(match value.trim().to_lowercase().as_str() {
                    "true" => Chip0007SensitiveContent::Flag(true),
                    "false" | "" => Chip0007SensitiveContent::Flag(false),
                    _ => Chip0007SensitiveContent::Items(vec![value.clone()]),
                })
            }
            _ => {
                self.dropped(key);
                None
            }
        }
    }

    fn series(&mut self, object: &Map<String, Value>, key: &str) -> Option<u64> {
        match object.get(key)? {
            Value::Number(value)
                if number_to_integer(value).is_some_and(|value| u64::try_from(value).is_ok()) =>
            {
                number_to_integer(value).and_then(|value| u64::try_from(value).ok())
            }
            Value::String(value) if value.trim().parse::<u64>().is_ok() => {
                self.coerced(key);
                value.trim().parse().ok()
            }
            _ => {
                self.dropped(key);
                None
            }
        }
    }

    fn integer(&mut self, object: &Map<String, Value>, key: &str, path: &str) -> Option<BigInt> {
        match object.get(key)? {
            Value::Number(value) if number_to_integer(value).is_some() => number_to_integer(value),
            Value::String(value) if value.trim().parse::<BigInt>().is_ok() => {
                self.coerced(path);
                value.trim().parse().ok()
            }
            _ => {
                self.dropped(path);
                None
            }
        }
    }

    /// Other scalars are converted to strings, since they're still meaningful to display.
    fn value(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<Chip0007Value> {
        match object.get(key) {
            Some(Value::String(value)) => Some(Chip0007Value::String(value.clone())),
            Some(Value::Number(value)) => {
                if let Some(value) = number_to_integer(value) {
                    return Some(Chip0007Value::Integer(value));
                }

                self.coerced(path);
                Some(Chip0007Value::String(value.to_string()))
            }
            Some(Value::Bool(value)) => {
                self.coerced(path);
                Some(Chip0007Value::String(value.to_string()))
            }
            Some(Value::Null) | None => {
                self.issues.push(Chip0007Issue::Missing(path.to_string()));
                None
            }
            Some(_) => {
                self.dropped(path);
                None
            }
        }
    }

    fn objects<'a>(
        &mut self,
        object: &'a Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Vec<(String, &'a Map<String, Value>)> {
        let Some(value) = object.get(key) else {
            return Vec::new();
        };

        let Value::Array(items) = value else {
            self.dropped(path);
            return Vec::new();
        };

        let mut objects = Vec::new();

        for (index, item) in items.iter().enumerate() {
            let path = format!("{path}[{index}]");

            if let Value::Object(item) = item {
                objects.push((path, item));
            } else {
                self.dropped(&path);
            }
        }

        objects
    }

    fn attributes(&mut self, object: &Map<String, Value>) -> Vec<Chip0007Attribute> {
        let mut attributes = Vec::new();

        for (path, item) in self.objects(object, "attributes", "attributes") {
            let trait_type = self.value(item, "trait_type", &format!("{path}.trait_type"));
            let value = self.value(item, "value", &format!("{path}.value"));
            let min_value = self.integer(item, "min_value", &format!("{path}.min_value"));
            let max_value = self.integer(item, "max_value", &format!("{path}.max_value"));

            if let (Some(trait_type), Some(value)) = (trait_type, value) {
                attributes.push(Chip0007Attribute {
                    trait_type,
                    value,
                    min_value,
                    max_value,
                });
            }
        }

        attributes
    }

    fn collection(&mut self, object: &Map<String, Value>) -> Option<Chip0007Collection> {
        let value = object.get("collection")?;

        let Value::Object(collection) = value else {
            self.dropped("collection");
            return None;
        };

        let mut attributes = Vec::new();

        for (path, item) in self.objects(collection, "attributes", "collection.attributes") {
            let kind = self.value(item, "type", &format!("{path}.type"));
            let value = self.value(item, "value", &format!("{path}.value"));

            if let (Some(kind), Some(value)) = (kind, value) {
                attributes.push(Chip0007CollectionAttribute { kind, value });
            }
        }

        let id = self.required_string(collection, "id", "collection.id");
        let name = self.required_string(collection, "name", "collection.name");

        // Collections are grouped by these, so it's not useful without both of them.
        Some(Chip0007Collection {
            id: id?,
            name: name?,
            attributes,
        })
    }

    fn data(&mut self, object: &Map<String, Value>) -> Option<Map<String, Value>> {
        if let Value::Object(data) = object.get("data")? {
            Some(data.clone())
        } else {
            self.dropped("data");
            None
        }
    }
}
