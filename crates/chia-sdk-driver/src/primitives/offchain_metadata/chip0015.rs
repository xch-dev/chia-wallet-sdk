use num_bigint::BigInt;
use serde_json::{Map, Value};

use super::{Chip0007Metadata, chip0007::number_to_integer};

/// The value of an identifier, which CHIP-0015 allows to be an integer, string or boolean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chip0015IdentifierValue {
    Integer(BigInt),
    String(String),
    Boolean(bool),
}

/// A schema.org `identifier`, such as a DID or social media account.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Chip0015Identifier {
    pub types: Vec<String>,
    pub property_id: Option<String>,
    pub value: Option<Chip0015IdentifierValue>,
}

/// The schema.org properties which
/// [CHIP-0015](https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0015.md) recommends
/// integrating. This is used for the `data` object of the metadata, as well as for any objects
/// nested inside of it, such as thumbnails and creators.
///
/// Every other schema.org property can be read from [`Chip0007Metadata::data`]. As with the
/// collection, none of this is verified and it should be combined with on-chain data before being
/// presented as authentic.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Chip0015Object {
    /// Only set on the `data` object itself.
    pub context: Option<String>,
    /// Usually a single type, but `@type` is allowed to be an array in JSON-LD.
    pub types: Vec<String>,
    pub encoding_format: Option<String>,
    pub name: Option<String>,
    pub url: Option<String>,
    /// CHIP-0015 lists this as an integer, but the examples are all strings and aren't always valid
    /// hex, so it's left up to the caller to interpret.
    pub sha256: Option<String>,
    pub thumbnail: Vec<Chip0015Object>,
    pub identifier: Vec<Chip0015Identifier>,
    pub creator: Vec<Chip0015Object>,
    pub contributor: Vec<Chip0015Object>,
    pub encodes_creative_work: Vec<Chip0015Object>,
    pub has_part: Vec<Chip0015Object>,
    pub associated_media: Vec<Chip0015Object>,
    pub is_part_of: Vec<Chip0015Object>,
}

impl Chip0007Metadata {
    /// Reads the schema.org properties from the `data` object. This is [`None`] if there's no
    /// schema.org `@context`, since the object is then free to contain anything else.
    pub fn schema_org(&self) -> Option<Chip0015Object> {
        Chip0015Object::from_data(self.data.as_ref()?)
    }
}

impl Chip0015Object {
    /// Reads the schema.org properties from a `data` object, if it has a schema.org `@context`.
    pub fn from_data(data: &Map<String, Value>) -> Option<Self> {
        Some(Self {
            context: Some(schema_org_context(data.get("@context")?)?.to_string()),
            ..Self::from_object(data)
        })
    }

    /// Whether the object has the properties which CHIP-0015 requires.
    pub fn is_compliant(&self) -> bool {
        self.context.is_some() && !self.types.is_empty() && self.encoding_format.is_some()
    }

    /// The URL, which has the `https://` protocol if it doesn't specify one.
    pub fn resolved_url(&self) -> Option<String> {
        let url = self.url.as_ref()?;

        Some(if url.contains("://") {
            url.clone()
        } else {
            format!("https://{url}")
        })
    }

    fn from_object(object: &Map<String, Value>) -> Self {
        let objects = |key, from_string: fn(&str) -> Self| {
            let mut items = Vec::new();

            for item in one_or_many(object.get(key)) {
                match item {
                    Value::Object(item) => items.push(Self::from_object(item)),
                    Value::String(item) => items.push(from_string(item)),
                    _ => {}
                }
            }

            items
        };

        Self {
            context: None,
            types: one_or_many(object.get("@type"))
                .into_iter()
                .filter_map(scalar_string)
                .collect(),
            // The spelling with an `@` is used by one of the CHIP-0015 examples.
            encoding_format: first_string(
                object
                    .get("encodingFormat")
                    .or_else(|| object.get("@encodingFormat")),
            ),
            name: first_string(object.get("name")),
            url: first_string(object.get("url")),
            sha256: first_string(object.get("sha256")),
            thumbnail: objects("thumbnail", Self::with_url),
            identifier: one_or_many(object.get("identifier"))
                .into_iter()
                .filter_map(Chip0015Identifier::from_json)
                .collect(),
            creator: objects("creator", Self::with_name),
            contributor: objects("contributor", Self::with_name),
            encodes_creative_work: objects("encodesCreativeWork", Self::with_url),
            has_part: objects("hasPart", Self::with_url),
            associated_media: objects("associatedMedia", Self::with_url),
            is_part_of: objects("isPartOf", Self::with_url),
        }
    }

    fn with_url(url: &str) -> Self {
        Self {
            url: Some(url.to_string()),
            ..Self::default()
        }
    }

    fn with_name(name: &str) -> Self {
        Self {
            name: Some(name.to_string()),
            ..Self::default()
        }
    }
}

impl Chip0015IdentifierValue {
    fn from_json(value: &Value) -> Option<Self> {
        match value {
            Value::String(value) => Some(Self::String(value.clone())),
            Value::Bool(value) => Some(Self::Boolean(*value)),
            Value::Number(value) => Some(
                number_to_integer(value)
                    .map_or_else(|| Self::String(value.to_string()), Self::Integer),
            ),
            _ => None,
        }
    }
}

impl Chip0015Identifier {
    fn from_json(value: &Value) -> Option<Self> {
        if let Value::Object(object) = value {
            return Some(Self {
                types: one_or_many(object.get("@type"))
                    .into_iter()
                    .filter_map(scalar_string)
                    .collect(),
                property_id: first_string(object.get("propertyID")),
                value: object
                    .get("value")
                    .and_then(Chip0015IdentifierValue::from_json),
            });
        }

        // In schema.org an identifier can also be plain text.
        Some(Self {
            value: Some(Chip0015IdentifierValue::from_json(value)?),
            ..Self::default()
        })
    }
}

/// CHIP-0015 only checks for `https://schema.org/`, but its examples also leave off the trailing
/// slash and `http` is common in existing schema.org data. A JSON-LD context can also be an array.
fn schema_org_context(context: &Value) -> Option<&str> {
    one_or_many(Some(context))
        .into_iter()
        .filter_map(Value::as_str)
        .find(|context| {
            matches!(
                context.trim_end_matches('/'),
                "https://schema.org" | "http://schema.org"
            )
        })
}

fn one_or_many(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(value) => vec![value],
        None => Vec::new(),
    }
}

fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn first_string(value: Option<&Value>) -> Option<String> {
    one_or_many(value).into_iter().find_map(scalar_string)
}
