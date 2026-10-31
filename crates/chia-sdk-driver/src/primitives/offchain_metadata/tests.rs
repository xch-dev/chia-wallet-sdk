use anyhow::Result;
use num_bigint::BigInt;
use rstest::rstest;
use serde_json::json;

use crate::{
    Chip0007Attribute, Chip0007Issue, Chip0007Metadata, Chip0007SensitiveContent, Chip0007Value,
    Chip0015IdentifierValue, Chip0015Object, DriverError,
};

const CHIP0007_EXAMPLE: &str = include_str!("fixtures/chip0007_example.json");
const CHIP0015_EXAMPLE: &str = include_str!("fixtures/png_w_integration3.json");
const CHIP0015_ADVANCED_EXAMPLE: &str = include_str!("fixtures/ArchiveComponent.json");

#[allow(clippy::needless_pass_by_value)]
fn base(overrides: serde_json::Value) -> String {
    let mut json = json!({
        "format": "CHIP-0007",
        "name": "Name",
        "description": "Description",
    });

    for (key, value) in overrides.as_object().unwrap() {
        json[key] = value.clone();
    }

    json.to_string()
}

fn issues(json: &str) -> Vec<Chip0007Issue> {
    Chip0007Metadata::parse_lenient(json).unwrap().issues
}

fn dropped(path: &str) -> Chip0007Issue {
    Chip0007Issue::Dropped(path.to_string())
}

#[test]
fn test_spec_example() -> Result<()> {
    let metadata = Chip0007Metadata::parse(CHIP0007_EXAMPLE)?;

    assert_eq!(metadata.format, "CHIP-0007");
    assert_eq!(metadata.name, "Pikachu");
    assert_eq!(
        metadata.description,
        "Electric-type Pok\u{e9}mon with stretchy cheeks"
    );
    assert_eq!(metadata.minting_tool.as_deref(), Some("SuperMinter/2.5.2"));
    assert_eq!(
        metadata.sensitive_content,
        Some(Chip0007SensitiveContent::Flag(false))
    );
    assert!(!metadata.is_sensitive());
    assert_eq!(metadata.series_number, Some(22));
    assert_eq!(metadata.series_total, Some(1000));
    assert_eq!(metadata.attributes.len(), 3);
    assert_eq!(
        metadata.attributes[2],
        Chip0007Attribute {
            trait_type: Chip0007Value::String("Friendship".to_string()),
            value: Chip0007Value::Integer(50.into()),
            min_value: Some(0.into()),
            max_value: Some(255.into()),
        }
    );

    let collection = metadata.collection.as_ref().unwrap();
    assert_eq!(collection.id, "e43fcfe6-1d5c-4d6e-82da-5de3aa8b3b57");
    assert_eq!(collection.name, "Example Pok\u{e9}mon Collection");
    assert_eq!(collection.attributes.len(), 5);
    assert_eq!(
        collection.attributes[1].kind,
        Chip0007Value::String("icon".to_string())
    );

    assert!(metadata.data.as_ref().unwrap().contains_key("example_data"));
    assert_eq!(metadata.schema_org(), None);

    Ok(())
}

#[rstest]
#[case(CHIP0007_EXAMPLE)]
#[case(CHIP0015_EXAMPLE)]
#[case(CHIP0015_ADVANCED_EXAMPLE)]
fn test_round_trip(#[case] json: &str) -> Result<()> {
    let metadata = Chip0007Metadata::parse(json)?;
    assert_eq!(Chip0007Metadata::parse(&metadata.to_json()?)?, metadata);
    Ok(())
}

#[test]
fn test_minimal_metadata() -> Result<()> {
    let metadata = Chip0007Metadata::new("Name".to_string(), "Description".to_string());

    assert_eq!(metadata.validate(), vec![]);
    assert_eq!(Chip0007Metadata::parse(&base(json!({})))?, metadata);
    assert_eq!(metadata.to_json()?, base(json!({})));

    Ok(())
}

#[test]
fn test_ignores_unknown_fields() -> Result<()> {
    let metadata = Chip0007Metadata::parse(&base(json!({ "unknown": [1, 2, 3] })))?;
    assert_eq!(metadata.name, "Name");
    Ok(())
}

#[rstest]
#[case("")]
#[case("{")]
#[case("not json")]
#[case(r#"{"name": "trailing comma",}"#)]
fn test_invalid_json(#[case] json: &str) {
    assert!(matches!(
        Chip0007Metadata::parse_lenient(json),
        Err(DriverError::InvalidChip0007Json(_))
    ));
}

#[rstest]
#[case("null")]
#[case("1")]
#[case(r#""CHIP-0007""#)]
#[case("[]")]
fn test_invalid_root(#[case] json: &str) {
    assert!(matches!(
        Chip0007Metadata::parse_lenient(json),
        Err(DriverError::InvalidChip0007Root)
    ));
}

#[test]
fn test_invalid_utf8() {
    assert!(matches!(
        Chip0007Metadata::from_bytes_lenient(b"{\"name\": \"\xFF\"}"),
        Err(DriverError::InvalidChip0007Json(_))
    ));
}

#[test]
fn test_excessive_nesting() {
    let json = format!(
        r#"{{"data": {}1{}}}"#,
        r#"{"a":"#.repeat(1000),
        "}".repeat(1000)
    );

    assert!(matches!(
        Chip0007Metadata::parse_lenient(&json),
        Err(DriverError::InvalidChip0007Json(_))
    ));
}

#[test]
fn test_byte_order_mark() -> Result<()> {
    let bytes = [b"\xEF\xBB\xBF", base(json!({})).as_bytes()].concat();

    assert!(Chip0007Metadata::from_bytes(&bytes).is_err());

    let result = Chip0007Metadata::from_bytes_lenient(&bytes)?;
    assert_eq!(result.metadata.name, "Name");
    assert_eq!(result.issues, vec![Chip0007Issue::Coerced("$".to_string())]);

    Ok(())
}

#[test]
fn test_empty_object() -> Result<()> {
    assert!(matches!(
        Chip0007Metadata::parse("{}"),
        Err(DriverError::Chip0007(Chip0007Issue::Missing(field))) if field == "format"
    ));

    let result = Chip0007Metadata::parse_lenient("{}")?;

    assert_eq!(result.metadata.name, "");
    assert_eq!(result.metadata.description, "");
    assert_eq!(
        result.issues,
        vec![
            Chip0007Issue::Missing("format".to_string()),
            Chip0007Issue::Missing("name".to_string()),
            Chip0007Issue::Missing("description".to_string()),
        ]
    );

    Ok(())
}

#[rstest]
#[case(json!({ "name": 5 }), dropped("name"))]
#[case(json!({ "minting_tool": 5 }), dropped("minting_tool"))]
#[case(json!({ "minting_tool": null }), dropped("minting_tool"))]
#[case(json!({ "sensitive_content": 1 }), dropped("sensitive_content"))]
#[case(json!({ "sensitive_content": [1] }), dropped("sensitive_content[0]"))]
#[case(json!({ "series_number": -1 }), dropped("series_number"))]
#[case(json!({ "series_total": 1.5 }), dropped("series_total"))]
#[case(json!({ "series_total": "ten" }), dropped("series_total"))]
#[case(json!({ "attributes": {} }), dropped("attributes"))]
#[case(json!({ "attributes": [1] }), dropped("attributes[0]"))]
#[case(
    json!({ "attributes": [{ "value": "Mouse" }] }),
    Chip0007Issue::Missing("attributes[0].trait_type".to_string())
)]
#[case(
    json!({ "attributes": [{ "trait_type": "Species", "value": null }] }),
    Chip0007Issue::Missing("attributes[0].value".to_string())
)]
#[case(
    json!({ "attributes": [{ "trait_type": "Species", "value": [] }] }),
    dropped("attributes[0].value")
)]
#[case(
    json!({ "attributes": [{ "trait_type": "A", "value": 1, "min_value": "low" }] }),
    dropped("attributes[0].min_value")
)]
#[case(json!({ "collection": "name" }), dropped("collection"))]
#[case(
    json!({ "collection": { "name": "Name" } }),
    Chip0007Issue::Missing("collection.id".to_string())
)]
#[case(
    json!({ "collection": { "id": "e43fcfe6-1d5c-4d6e-82da-5de3aa8b3b57" } }),
    Chip0007Issue::Missing("collection.name".to_string())
)]
#[case(
    json!({ "collection": {
        "id": "e43fcfe6-1d5c-4d6e-82da-5de3aa8b3b57",
        "name": "Name",
        "attributes": [{ "type": "icon" }],
    } }),
    Chip0007Issue::Missing("collection.attributes[0].value".to_string())
)]
#[case(json!({ "data": [] }), dropped("data"))]
#[case(json!({ "series_number": 0 }), Chip0007Issue::SeriesOutOfRange("series_number".to_string()))]
#[case(json!({ "series_total": 0 }), Chip0007Issue::SeriesOutOfRange("series_total".to_string()))]
#[case(json!({ "format": "CHIP-0123" }), Chip0007Issue::UnsupportedFormat("CHIP-0123".to_string()))]
#[case(
    json!({ "collection": { "id": "not-a-uuid", "name": "Name" } }),
    Chip0007Issue::InvalidCollectionId("not-a-uuid".to_string())
)]
#[case(
    json!({ "attributes": [{ "trait_type": "Rarity", "value": 1.5 }] }),
    Chip0007Issue::Coerced("attributes[0].value".to_string())
)]
#[case(
    json!({ "attributes": [{ "trait_type": "Big", "value": 1e30 }] }),
    Chip0007Issue::Coerced("attributes[0].value".to_string())
)]
fn test_issues(#[case] overrides: serde_json::Value, #[case] issue: Chip0007Issue) {
    let json = base(overrides);

    assert_eq!(issues(&json), vec![issue.clone()]);
    assert!(matches!(
        Chip0007Metadata::parse(&json),
        Err(DriverError::Chip0007(error)) if error == issue
    ));
}

#[test]
fn test_integers_with_zero_fraction() -> Result<()> {
    let metadata = Chip0007Metadata::parse(&base(json!({
        "series_number": 22.0,
        "attributes": [{ "trait_type": "Total", "value": 200.0 }],
    })))?;

    assert_eq!(metadata.series_number, Some(22));
    assert_eq!(
        metadata.attributes[0].value,
        Chip0007Value::Integer(200.into())
    );

    Ok(())
}

#[test]
fn test_lenient_keeps_valid_fields() -> Result<()> {
    let result = Chip0007Metadata::parse_lenient(&base(json!({
        "minting_tool": 5,
        "series_number": "22",
        "attributes": [
            1,
            { "trait_type": "Species", "value": "Mouse" },
            { "trait_type": 5, "value": true },
            { "trait_type": "Friendship", "value": 50, "min_value": "0", "max_value": "high" },
        ],
        "collection": { "id": "not-a-uuid", "name": "Name", "attributes": [{}] },
    })))?;

    let metadata = result.metadata;

    assert_eq!(metadata.name, "Name");
    assert_eq!(metadata.minting_tool, None);
    assert_eq!(metadata.series_number, Some(22));
    assert_eq!(metadata.attributes.len(), 3);
    assert_eq!(
        metadata.attributes[1].trait_type,
        Chip0007Value::Integer(5.into())
    );
    assert_eq!(
        metadata.attributes[1].value,
        Chip0007Value::String("true".to_string())
    );
    assert_eq!(metadata.attributes[2].min_value, Some(0.into()));
    assert_eq!(metadata.attributes[2].max_value, None);
    assert_eq!(metadata.collection.unwrap().id, "not-a-uuid");
    assert_eq!(result.issues.len(), 9);

    Ok(())
}

#[rstest]
#[case(json!(true), true)]
#[case(json!(false), false)]
#[case(json!([]), false)]
#[case(json!(["violence"]), true)]
#[case(json!("true"), true)]
#[case(json!("TRUE"), true)]
#[case(json!("false"), false)]
#[case(json!(""), false)]
#[case(json!("violence"), true)]
#[case(json!(0), false)]
fn test_sensitive_content(#[case] value: serde_json::Value, #[case] sensitive: bool) -> Result<()> {
    let json = base(json!({ "sensitive_content": value }));
    let result = Chip0007Metadata::parse_lenient(&json)?;
    assert_eq!(result.metadata.is_sensitive(), sensitive);
    Ok(())
}

#[test]
fn test_integer_limits() -> Result<()> {
    let attribute = |value: BigInt| Chip0007Attribute {
        trait_type: Chip0007Value::String("Value".to_string()),
        value: Chip0007Value::Integer(value),
        min_value: None,
        max_value: None,
    };

    let mut metadata = Chip0007Metadata::new("Name".to_string(), "Description".to_string());
    metadata.attributes = vec![attribute(u64::MAX.into()), attribute(i64::MIN.into())];
    assert_eq!(Chip0007Metadata::parse(&metadata.to_json()?)?, metadata);

    let too_large = BigInt::from(u64::MAX) + BigInt::from(1);
    metadata.attributes = vec![attribute(too_large.clone())];
    assert!(matches!(
        metadata.to_json(),
        Err(DriverError::Chip0007(Chip0007Issue::IntegerOutOfRange(value))) if value == too_large
    ));

    Ok(())
}

#[rstest]
#[case("E43FCFE6-1D5C-4D6E-82DA-5DE3AA8B3B57", true)]
#[case("e43fcfe6-1d5c-4d6e-82da-5de3aa8b3b57", true)]
#[case("e43fcfe61d5c4d6e82da5de3aa8b3b57", false)]
#[case("e43fcfe6-1d5c-4d6e-82da-5de3aa8b3b5", false)]
#[case("g43fcfe6-1d5c-4d6e-82da-5de3aa8b3b57", false)]
#[case("", false)]
fn test_collection_id(#[case] id: &str, #[case] valid: bool) {
    let json = base(json!({ "collection": { "id": id, "name": "Name" } }));
    assert_eq!(Chip0007Metadata::parse(&json).is_ok(), valid);
}

#[test]
fn test_chip0015_format_example() -> Result<()> {
    let metadata = Chip0007Metadata::parse(CHIP0015_EXAMPLE)?;
    assert_eq!(metadata.format, "CHIP-0015");

    let data = metadata.schema_org().unwrap();

    assert!(data.is_compliant());
    assert_eq!(data.context.as_deref(), Some("https://schema.org/"));
    assert_eq!(data.types, vec!["ImageObject"]);
    assert_eq!(data.encoding_format.as_deref(), Some("image/png"));
    assert_eq!(data.thumbnail.len(), 1);
    assert_eq!(data.thumbnail[0].types, vec!["ImageObject"]);
    assert_eq!(data.thumbnail[0].context, None);
    assert!(!data.thumbnail[0].is_compliant());
    assert!(data.thumbnail[0].sha256.is_some());
    assert_eq!(data.creator.len(), 1);
    assert_eq!(data.creator[0].identifier.len(), 1);
    assert_eq!(data.contributor.len(), 1);
    assert_eq!(data.encodes_creative_work.len(), 1);
    assert_eq!(data.associated_media.len(), 1);

    Ok(())
}

#[test]
fn test_chip0015_spec_data() {
    let json = base(json!({
        "data": {
            "@context": "https://schema.org/",
            "@type": "VideoObject",
            "encodingFormat": "video/mp4",
            "thumbnail": {
                "@type": "ImageObject",
                "encodingFormat": "image/png",
                "name": "Example #1 Thumbnail",
                "url": "https://www.fakeurl.com/example1_thumbnail.png",
                "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            },
            "creator": {
                "@type": "Organization",
                "name": "NFTr",
                "url": "https://nftr.pro/",
                "identifier": {
                    "@type": "PropertyValue",
                    "propertyID": "Twitter",
                    "value": "@nftr_pro",
                },
            },
            "contributor": {
                "@type": "Person",
                "name": "ClydeWallace",
                "identifier": {
                    "@type": "PropertyValue",
                    "propertyID": "DID",
                    "value": "did:chia:1cmxxr9snvz8rv084p2fhpa7je4z8sc9dapwtqxdwghcg53my3slsp902fe",
                },
            },
            "encodesCreativeWork": {
                "@type": "ImageObject",
                "encodingFormat": "image/png",
                "name": "Example #1 Component #1",
                "url": "https://www.fakeurl.com/example1_component1.png",
                "sha256": "9123dcbb0b42652b0e105956c68d3ca2ff34584f324fa41a29aedd32b883e131",
            },
            "associatedMedia": {
                "@type": "3DModel",
                "encodingFormat": "model/gltf-binary",
                "name": "Example #1 3D Model",
                "url": "https://www.fakeurl.com/example1_3dmodel.png",
                "sha256": "e87v0oi3h497vsoij40t98hae4oiu5y08f4hoq34t5j094834qht09jq34g08qu7",
            },
        },
    }));

    let data = Chip0007Metadata::parse(&json)
        .unwrap()
        .schema_org()
        .unwrap();

    assert!(data.is_compliant());
    assert_eq!(data.types, vec!["VideoObject"]);
    assert_eq!(
        data.thumbnail[0].name.as_deref(),
        Some("Example #1 Thumbnail")
    );
    assert_eq!(data.creator[0].types, vec!["Organization"]);
    assert_eq!(data.creator[0].identifier[0].types, vec!["PropertyValue"]);
    assert_eq!(
        data.creator[0].identifier[0].property_id.as_deref(),
        Some("Twitter")
    );
    assert_eq!(
        data.creator[0].identifier[0].value,
        Some(Chip0015IdentifierValue::String("@nftr_pro".to_string()))
    );
    assert_eq!(data.contributor[0].name.as_deref(), Some("ClydeWallace"));
    assert_eq!(data.encodes_creative_work[0].types, vec!["ImageObject"]);
    assert_eq!(data.associated_media[0].types, vec!["3DModel"]);
    // This isn't valid hex, but it's what the specification uses.
    assert_eq!(
        data.associated_media[0].sha256.as_deref(),
        Some("e87v0oi3h497vsoij40t98hae4oiu5y08f4hoq34t5j094834qht09jq34g08qu7")
    );
}

#[test]
fn test_chip0015_advanced_example() -> Result<()> {
    let data = Chip0007Metadata::parse(CHIP0015_ADVANCED_EXAMPLE)?
        .schema_org()
        .unwrap();

    // This is an example from the CHIP, but it doesn't have the required `encodingFormat`.
    assert!(!data.is_compliant());
    assert_eq!(data.types, vec!["CreativeWork", "ArchiveComponent"]);

    Ok(())
}

#[rstest]
#[case(json!({}), false)]
#[case(json!({ "example_data": "abc" }), false)]
#[case(json!({ "@context": "https://example.com/" }), false)]
#[case(json!({ "@context": 1 }), false)]
#[case(json!({ "@context": "https://schema.org/" }), true)]
#[case(json!({ "@context": "https://schema.org" }), true)]
#[case(json!({ "@context": "http://schema.org/" }), true)]
#[case(json!({ "@context": ["https://example.com/", "https://schema.org/"] }), true)]
fn test_chip0015_context(#[case] data: serde_json::Value, #[case] is_schema_org: bool) {
    let result = Chip0007Metadata::parse(&base(json!({ "data": data }))).unwrap();
    assert_eq!(result.schema_org().is_some(), is_schema_org);
}

#[test]
fn test_chip0015_malformed_properties() {
    let data = Chip0015Object::from_data(
        json!({
            "@context": "https://schema.org/",
            "@type": 5,
            "encodingFormat": { "not": "a string" },
            "name": ["First", "Second"],
            "sha256": 1234,
            "url": "example.com/image.png",
            "thumbnail": ["https://example.com/thumbnail.png", 5, { "url": "second.png" }],
            "creator": "ClydeWallace",
            "contributor": null,
            "identifier": ["plain", 5, true, null, { "propertyID": "DID" }],
        })
        .as_object()
        .unwrap(),
    )
    .unwrap();

    assert!(!data.is_compliant());
    assert_eq!(data.types, vec!["5"]);
    assert_eq!(data.encoding_format, None);
    assert_eq!(data.name.as_deref(), Some("First"));
    assert_eq!(data.sha256.as_deref(), Some("1234"));
    assert_eq!(
        data.resolved_url().as_deref(),
        Some("https://example.com/image.png")
    );
    assert_eq!(data.thumbnail.len(), 2);
    assert_eq!(
        data.thumbnail[0].resolved_url().as_deref(),
        Some("https://example.com/thumbnail.png")
    );
    assert_eq!(data.creator[0].name.as_deref(), Some("ClydeWallace"));
    assert_eq!(data.contributor, vec![]);
    assert_eq!(
        data.identifier
            .iter()
            .map(|identifier| identifier.value.clone())
            .collect::<Vec<_>>(),
        vec![
            Some(Chip0015IdentifierValue::String("plain".to_string())),
            Some(Chip0015IdentifierValue::Integer(5.into())),
            Some(Chip0015IdentifierValue::Boolean(true)),
            None,
        ]
    );
}
