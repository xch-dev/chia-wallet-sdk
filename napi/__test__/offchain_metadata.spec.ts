import test from "ava";

import {
  Chip0007Attribute,
  Chip0007Metadata,
  Chip0007SensitiveContent,
  Chip0007Value,
} from "../index.js";

const example = JSON.stringify({
  format: "CHIP-0015",
  name: "Example",
  description: "An example NFT",
  sensitive_content: ["violence"],
  series_number: 22,
  series_total: 1000,
  attributes: [
    { trait_type: "Species", value: "Mouse" },
    { trait_type: "Friendship", value: 50, min_value: 0, max_value: 255 },
  ],
  collection: {
    id: "e43fcfe6-1d5c-4d6e-82da-5de3aa8b3b57",
    name: "Example Collection",
    attributes: [{ type: "icon", value: "https://example.com/icon.png" }],
  },
  data: {
    "@context": "https://schema.org/",
    "@type": "VideoObject",
    encodingFormat: "video/mp4",
    thumbnail: { "@type": "ImageObject", url: "example.com/thumbnail.png" },
    creator: {
      "@type": "Person",
      name: "Creator",
      identifier: { "@type": "PropertyValue", propertyID: "Twitter", value: "@creator" },
    },
  },
});

test("parses metadata", (t) => {
  const metadata = Chip0007Metadata.parse(example);

  t.is(metadata.format, "CHIP-0015");
  t.is(metadata.name, "Example");
  t.is(metadata.seriesNumber, 22n);
  t.true(metadata.isSensitive());
  t.deepEqual(metadata.sensitiveContent?.asItems(), ["violence"]);
  t.is(metadata.attributes[0].value.asString(), "Mouse");
  t.is(metadata.attributes[1].value.asInteger(), 50n);
  t.is(metadata.attributes[1].maxValue, 255n);
  t.is(metadata.collection?.attributes[0].kind.asString(), "icon");
  t.deepEqual(metadata.validate(), []);
  t.deepEqual(Chip0007Metadata.parse(metadata.toJson()).validate(), []);
});

test("reads schema.org data", (t) => {
  const data = Chip0007Metadata.parse(example).schemaOrg();

  t.truthy(data);
  t.true(data!.isCompliant());
  t.deepEqual(data!.types, ["VideoObject"]);
  t.is(data!.thumbnail[0].resolvedUrl(), "https://example.com/thumbnail.png");
  t.is(data!.creator[0].identifier[0].propertyId, "Twitter");
  t.is(data!.creator[0].identifier[0].value?.asString(), "@creator");
});

test("rejects malformed metadata unless parsing leniently", (t) => {
  const json = JSON.stringify({ name: "Partial", series_number: "two", attributes: [1] });

  t.throws(() => Chip0007Metadata.parse(json));
  t.throws(() => Chip0007Metadata.parseLenient("not json"));

  const result = Chip0007Metadata.parseLenient(json);

  t.is(result.metadata.name, "Partial");
  t.is(result.metadata.seriesNumber, null);
  t.deepEqual(result.issues, [
    "missing required field `format`",
    "missing required field `description`",
    "ignored invalid field `series_number`",
    "ignored invalid field `attributes[0]`",
  ]);
});

test("builds metadata", (t) => {
  const metadata = new Chip0007Metadata(
    "CHIP-0007",
    "Name",
    "Description",
    null,
    Chip0007SensitiveContent.fromFlag(false),
    null,
    null,
    [
      new Chip0007Attribute(
        Chip0007Value.fromString("Level"),
        Chip0007Value.fromInteger(5n),
        1n,
        null,
      ),
    ],
    null,
    null,
  );

  t.deepEqual(JSON.parse(metadata.toJson()), {
    format: "CHIP-0007",
    name: "Name",
    description: "Description",
    sensitive_content: false,
    attributes: [{ trait_type: "Level", value: 5, min_value: 1 }],
  });
});
