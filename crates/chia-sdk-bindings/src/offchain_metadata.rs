use bindy::{Error, Result};
use chia_protocol::Bytes;
use chia_sdk_driver::{
    self as sdk, Chip0007Attribute, Chip0007Collection, Chip0007SensitiveContent, Chip0007Value,
    Chip0015IdentifierValue, Chip0015Object, DriverError,
};
use num_bigint::BigInt;
use serde_json::{Map, Value};

/// The off-chain metadata of an NFT, as defined by CHIP-0007. The `data` object is represented
/// as a JSON string.
#[derive(Clone)]
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
    pub data: Option<String>,
}

#[derive(Clone)]
pub struct Chip0007ParseResult {
    pub metadata: Chip0007Metadata,
    pub issues: Vec<String>,
}

impl From<sdk::Chip0007Metadata> for Chip0007Metadata {
    fn from(value: sdk::Chip0007Metadata) -> Self {
        Self {
            format: value.format,
            name: value.name,
            description: value.description,
            minting_tool: value.minting_tool,
            sensitive_content: value.sensitive_content,
            series_number: value.series_number,
            series_total: value.series_total,
            attributes: value.attributes,
            collection: value.collection,
            data: value.data.map(|data| Value::Object(data).to_string()),
        }
    }
}

impl From<sdk::Chip0007ParseResult> for Chip0007ParseResult {
    fn from(value: sdk::Chip0007ParseResult) -> Self {
        Self {
            metadata: value.metadata.into(),
            issues: value.issues.iter().map(ToString::to_string).collect(),
        }
    }
}

impl TryFrom<Chip0007Metadata> for sdk::Chip0007Metadata {
    type Error = Error;

    fn try_from(value: Chip0007Metadata) -> Result<Self> {
        let data = value
            .data
            .map(|data| serde_json::from_str::<Map<String, Value>>(&data))
            .transpose()
            .map_err(DriverError::from)?;

        Ok(Self {
            format: value.format,
            name: value.name,
            description: value.description,
            minting_tool: value.minting_tool,
            sensitive_content: value.sensitive_content,
            series_number: value.series_number,
            series_total: value.series_total,
            attributes: value.attributes,
            collection: value.collection,
            data,
        })
    }
}

impl Chip0007Metadata {
    /// Parses metadata which must comply with the specification.
    pub fn parse(json: String) -> Result<Self> {
        Ok(sdk::Chip0007Metadata::parse(&json)?.into())
    }

    /// Parses the contents of a metadata file which must comply with the specification.
    pub fn from_bytes(bytes: Bytes) -> Result<Self> {
        Ok(sdk::Chip0007Metadata::from_bytes(&bytes)?.into())
    }

    /// Parses metadata as well as it can be, returning everything that was wrong with it.
    pub fn parse_lenient(json: String) -> Result<Chip0007ParseResult> {
        Ok(sdk::Chip0007Metadata::parse_lenient(&json)?.into())
    }

    /// Parses the contents of a metadata file as well as it can be, returning everything that was
    /// wrong with it.
    pub fn from_bytes_lenient(bytes: Bytes) -> Result<Chip0007ParseResult> {
        Ok(sdk::Chip0007Metadata::from_bytes_lenient(&bytes)?.into())
    }

    pub fn validate(&self) -> Result<Vec<String>> {
        let metadata = sdk::Chip0007Metadata::try_from(self.clone())?;
        Ok(metadata
            .validate()
            .iter()
            .map(ToString::to_string)
            .collect())
    }

    pub fn is_sensitive(&self) -> Result<bool> {
        Ok(self
            .sensitive_content
            .as_ref()
            .is_some_and(Chip0007SensitiveContent::is_sensitive))
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(sdk::Chip0007Metadata::try_from(self.clone())?.to_json()?)
    }

    /// Reads the CHIP-0015 schema.org properties from the `data` object, if it has a schema.org
    /// `@context`.
    pub fn schema_org(&self) -> Result<Option<Chip0015Object>> {
        Ok(sdk::Chip0007Metadata::try_from(self.clone())?.schema_org())
    }
}

pub trait Chip0007ValueExt: Sized {
    fn from_string(value: String) -> Result<Self>;
    fn from_integer(value: BigInt) -> Result<Self>;
    fn as_string(&self) -> Result<Option<String>>;
    fn as_integer(&self) -> Result<Option<BigInt>>;
}

impl Chip0007ValueExt for Chip0007Value {
    fn from_string(value: String) -> Result<Self> {
        Ok(Self::String(value))
    }

    fn from_integer(value: BigInt) -> Result<Self> {
        Ok(Self::Integer(value))
    }

    fn as_string(&self) -> Result<Option<String>> {
        match self {
            Self::String(value) => Ok(Some(value.clone())),
            Self::Integer(_) => Ok(None),
        }
    }

    fn as_integer(&self) -> Result<Option<BigInt>> {
        match self {
            Self::Integer(value) => Ok(Some(value.clone())),
            Self::String(_) => Ok(None),
        }
    }
}

pub trait Chip0007SensitiveContentExt: Sized {
    fn from_flag(flag: bool) -> Result<Self>;
    fn from_items(items: Vec<String>) -> Result<Self>;
    fn as_flag(&self) -> Result<Option<bool>>;
    fn as_items(&self) -> Result<Option<Vec<String>>>;
    fn is_sensitive(&self) -> Result<bool>;
}

impl Chip0007SensitiveContentExt for Chip0007SensitiveContent {
    fn from_flag(flag: bool) -> Result<Self> {
        Ok(Self::Flag(flag))
    }

    fn from_items(items: Vec<String>) -> Result<Self> {
        Ok(Self::Items(items))
    }

    fn as_flag(&self) -> Result<Option<bool>> {
        match self {
            Self::Flag(flag) => Ok(Some(*flag)),
            Self::Items(_) => Ok(None),
        }
    }

    fn as_items(&self) -> Result<Option<Vec<String>>> {
        match self {
            Self::Items(items) => Ok(Some(items.clone())),
            Self::Flag(_) => Ok(None),
        }
    }

    fn is_sensitive(&self) -> Result<bool> {
        Ok(Chip0007SensitiveContent::is_sensitive(self))
    }
}

pub trait Chip0015ObjectExt: Sized {
    fn is_compliant(&self) -> Result<bool>;
    fn resolved_url(&self) -> Result<Option<String>>;
}

impl Chip0015ObjectExt for Chip0015Object {
    fn is_compliant(&self) -> Result<bool> {
        Ok(Chip0015Object::is_compliant(self))
    }

    fn resolved_url(&self) -> Result<Option<String>> {
        Ok(Chip0015Object::resolved_url(self))
    }
}

pub trait Chip0015IdentifierValueExt: Sized {
    fn as_string(&self) -> Result<Option<String>>;
    fn as_integer(&self) -> Result<Option<BigInt>>;
    fn as_boolean(&self) -> Result<Option<bool>>;
}

impl Chip0015IdentifierValueExt for Chip0015IdentifierValue {
    fn as_string(&self) -> Result<Option<String>> {
        match self {
            Self::String(value) => Ok(Some(value.clone())),
            _ => Ok(None),
        }
    }

    fn as_integer(&self) -> Result<Option<BigInt>> {
        match self {
            Self::Integer(value) => Ok(Some(value.clone())),
            _ => Ok(None),
        }
    }

    fn as_boolean(&self) -> Result<Option<bool>> {
        match self {
            Self::Boolean(value) => Ok(Some(*value)),
            _ => Ok(None),
        }
    }
}
