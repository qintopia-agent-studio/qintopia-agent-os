use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use anyhow::{bail, Context, Result};
use base64ct::{Base64, Encoding};
use chrono::{DateTime, TimeZone, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use tracing::warn;
use uuid::Uuid;

use crate::{
    event::RawQiweEvent,
    strict_json::{
        parse_strict_bounded_str, registry_json_limits, JsonLimits, QIWE_STRING_DATA_LIMITS,
    },
};

const MAX_DATA_ITEMS: usize = 64;
const MAX_PREDICATE_DEPTH: usize = 8;
const MAX_PREDICATES: usize = 64;
const MAX_TRANSFORMS: usize = 8;
const MAX_SUBJECTS: usize = 64;
const MAX_IDENTIFIER_BYTES: usize = 128;
const MAX_JSON_POINTER_BYTES: usize = 256;
const MAX_OPAQUE_ID_BYTES: usize = 256;
const MAX_PREDICATE_STRING_BYTES: usize = 16 * 1024;
const MAX_RESTRICTED_PRIMITIVE_OPERATIONS: usize = 8;
const MAX_EXPANDED_TRANSFORMS: usize = 16;
const MAX_RESTRICTED_JSON_DEPTH: usize = 16;
const MAX_RESTRICTED_JSON_NODES: usize = 1_024;
const MAX_RESTRICTED_JSON_STRING_BYTES: usize = 16 * 1024;
const MAX_MAPPING_SOURCE_BYTES: usize = 128 * 1024;
const MAX_FIXTURE_SOURCE_BYTES: usize = 256 * 1024;
const MAX_EXPECTATION_SOURCE_BYTES: usize = 128 * 1024;
const MAX_PRIMITIVE_SOURCE_BYTES: usize = 64 * 1024;
const MAX_REGISTRY_SOURCE_COUNT: usize = 1_024;
const MAX_MAPPING_AGGREGATE_BYTES: usize = 16 * 1024 * 1024;
const MAX_FIXTURE_AGGREGATE_BYTES: usize = 32 * 1024 * 1024;
const MAX_EXPECTATION_AGGREGATE_BYTES: usize = 16 * 1024 * 1024;
const MAX_PRIMITIVE_AGGREGATE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Predicate {
    Equals {
        pointer: String,
        value: Value,
    },
    In {
        pointer: String,
        values: Vec<Value>,
    },
    Exists {
        pointer: String,
        #[serde(default = "default_true")]
        value: bool,
    },
    TypeIs {
        pointer: String,
        value: JsonType,
    },
    All {
        rules: Vec<Predicate>,
    },
    Any {
        rules: Vec<Predicate>,
    },
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum JsonType {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ExtractorSpec {
    event_type: String,
    event_id: ValueExtractor,
    space_chat_id: ValueExtractor,
    subject_user_ids: ValueExtractor,
    occurred_at: ValueExtractor,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ValueExtractor {
    pointer: String,
    #[serde(default)]
    transforms: Vec<Transform>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Transform {
    Base64Utf8,
    Split {
        delimiter: String,
        max_parts: usize,
    },
    Dedupe,
    OpaqueId,
    UnixTimestamp {
        #[serde(default)]
        milliseconds: bool,
    },
    RestrictedPrimitive {
        primitive_ref: String,
    },
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum RestrictedPrimitiveOperation {
    Base64Utf8,
    JsonParse,
    JsonPointer { pointer: String },
    Split { delimiter: String, max_parts: usize },
    StringTrim,
    ArrayFlatten,
}

#[derive(Debug, Clone)]
struct MappingVersion {
    id: Uuid,
    provider: String,
    definition_key: String,
    version: i32,
    definition_digest: String,
    selector: Predicate,
    extractor: ExtractorSpec,
    status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CanonicalChannelEvent {
    pub event_type: String,
    pub event_id: String,
    pub space_chat_id: String,
    pub subject_user_ids: Vec<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
struct EmbeddedJsonSource {
    path: &'static str,
    contents: &'static str,
}

include!(concat!(
    env!("OUT_DIR"),
    "/channel_event_fixture_registry.rs"
));

static RESTRICTED_PRIMITIVE_REGISTRY: OnceLock<
    std::result::Result<HashMap<String, RestrictedPrimitive>, String>,
> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisteredMappingDocument {
    schema_version: u32,
    provider: String,
    definition_key: String,
    selector: Value,
    extractor: Value,
    official_sources: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisteredRestrictedPrimitiveDocument {
    schema_version: u32,
    provider: String,
    definition_key: String,
    operations: Vec<RestrictedPrimitiveOperation>,
    official_sources: Vec<String>,
}

#[derive(Debug)]
struct RestrictedPrimitive {
    operations: Vec<RestrictedPrimitiveOperation>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisteredFixtureDocument {
    fixture_metadata: RegisteredFixtureMetadata,
    event: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisteredFixtureMetadata {
    sanitized: bool,
    synthetic: bool,
    mapping_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisteredExpectationDocument {
    expectation_metadata: RegisteredExpectationMetadata,
    events: Vec<RegisteredExpectedEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisteredExpectationMetadata {
    sanitized: bool,
    synthetic: bool,
    mapping_ref: String,
    fixture_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisteredExpectedEvent {
    event_type: String,
    event_id: String,
    space_id: String,
    subject_user_ids: Vec<String>,
    occurred_at: String,
}

#[derive(Debug)]
struct RegisteredMapping {
    path: String,
    provider: String,
    definition_key: String,
    source_sha256: String,
    selector: Predicate,
    extractor: ExtractorSpec,
}

#[derive(Debug)]
struct RegisteredFixture {
    path: String,
    mapping_ref: String,
    event: Value,
}

#[derive(Debug)]
struct RegisteredExpectation {
    mapping_ref: String,
    fixture_ref: String,
    events: Vec<CanonicalChannelEvent>,
}

#[derive(Debug)]
struct RegisteredFixtureCase {
    fixture_path: String,
    event: Value,
    expected_events: Vec<CanonicalChannelEvent>,
}

#[derive(Debug)]
struct RegisteredFixtureSuite {
    mapping: RegisteredMapping,
    fixtures: Vec<RegisteredFixtureCase>,
}

fn validate_embedded_source_set(
    kind: &str,
    sources: &[EmbeddedJsonSource],
    max_source_bytes: usize,
    max_aggregate_bytes: usize,
) -> Result<()> {
    if sources.len() > MAX_REGISTRY_SOURCE_COUNT {
        bail!("embedded {kind} source count exceeds {MAX_REGISTRY_SOURCE_COUNT}");
    }
    let mut aggregate_bytes = 0usize;
    for source in sources {
        let source_bytes = source.contents.len();
        if source_bytes > max_source_bytes {
            bail!(
                "embedded {kind} source exceeds its byte limit: {}",
                source.path
            );
        }
        aggregate_bytes = aggregate_bytes
            .checked_add(source_bytes)
            .context("embedded registry aggregate byte count overflowed")?;
        if aggregate_bytes > max_aggregate_bytes {
            bail!("embedded {kind} registry exceeds its aggregate byte limit");
        }
    }
    Ok(())
}

fn parse_embedded_document<T: DeserializeOwned>(
    source: &EmbeddedJsonSource,
    max_bytes: usize,
) -> Result<T> {
    let value = parse_strict_bounded_str(source.contents, registry_json_limits(max_bytes))
        .with_context(|| format!("strictly parse embedded JSON {}", source.path))?;
    serde_json::from_value(value)
        .with_context(|| format!("deserialize embedded JSON {}", source.path))
}

fn restricted_primitive_registry() -> Result<&'static HashMap<String, RestrictedPrimitive>> {
    let result = RESTRICTED_PRIMITIVE_REGISTRY.get_or_init(|| {
        load_registered_restricted_primitives(EMBEDDED_RESTRICTED_PRIMITIVE_SOURCES)
            .map_err(|error| format!("{error:#}"))
    });
    result
        .as_ref()
        .map_err(|message| anyhow::anyhow!(message.clone()))
}

fn load_registered_restricted_primitives(
    sources: &[EmbeddedJsonSource],
) -> Result<HashMap<String, RestrictedPrimitive>> {
    validate_embedded_source_set(
        "restricted primitive",
        sources,
        MAX_PRIMITIVE_SOURCE_BYTES,
        MAX_PRIMITIVE_AGGREGATE_BYTES,
    )?;
    let mut primitives = HashMap::with_capacity(sources.len());
    let mut definition_keys = HashSet::with_capacity(sources.len());
    for source in sources {
        if !safe_restricted_primitive_ref(source.path) {
            bail!("restricted primitive path is invalid: {}", source.path);
        }
        let document: RegisteredRestrictedPrimitiveDocument =
            parse_embedded_document(source, MAX_PRIMITIVE_SOURCE_BYTES)
                .with_context(|| format!("parse restricted primitive {}", source.path))?;
        validate_registered_restricted_primitive_document(&document, source.path)?;
        if !definition_keys.insert(document.definition_key.clone()) {
            bail!(
                "restricted primitive definition key is duplicated: {}",
                document.definition_key
            );
        }
        if primitives
            .insert(
                source.path.to_string(),
                RestrictedPrimitive {
                    operations: document.operations,
                },
            )
            .is_some()
        {
            bail!("restricted primitive path is duplicated: {}", source.path);
        }
    }
    Ok(primitives)
}

fn validate_registered_restricted_primitive_document(
    document: &RegisteredRestrictedPrimitiveDocument,
    path: &str,
) -> Result<()> {
    if document.schema_version != 1
        || document.provider != "qiwe"
        || !safe_mapping_identifier(&document.definition_key)
    {
        bail!("restricted primitive metadata is invalid: {path}");
    }
    if document.operations.is_empty()
        || document.operations.len() > MAX_RESTRICTED_PRIMITIVE_OPERATIONS
    {
        bail!("restricted primitive operation count is invalid: {path}");
    }
    for operation in &document.operations {
        validate_restricted_primitive_operation(operation)
            .with_context(|| format!("validate restricted primitive operation: {path}"))?;
    }
    if document.official_sources.is_empty() || document.official_sources.len() > 8 {
        bail!("restricted primitive official source count is invalid: {path}");
    }
    let mut sources = HashSet::new();
    for source in &document.official_sources {
        if !sources.insert(source) {
            bail!("restricted primitive official sources are duplicated: {path}");
        }
        validate_official_qiwe_document_url(source)
            .with_context(|| format!("validate restricted primitive source: {path}"))?;
    }
    Ok(())
}

fn validate_restricted_primitive_operation(operation: &RestrictedPrimitiveOperation) -> Result<()> {
    match operation {
        RestrictedPrimitiveOperation::JsonPointer { pointer } => validate_pointer(pointer),
        RestrictedPrimitiveOperation::Split {
            delimiter,
            max_parts,
        } => validate_split(delimiter, *max_parts),
        RestrictedPrimitiveOperation::Base64Utf8
        | RestrictedPrimitiveOperation::JsonParse
        | RestrictedPrimitiveOperation::StringTrim
        | RestrictedPrimitiveOperation::ArrayFlatten => Ok(()),
    }
}

fn validate_official_qiwe_document_url(value: &str) -> Result<()> {
    let url = url::Url::parse(value).context("parse QiWe official document URL")?;
    let document_id = url.path().strip_prefix("/doc-").unwrap_or_default();
    if url.scheme() != "https"
        || url.host_str() != Some("doc.qiweapi.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || document_id.is_empty()
        || !document_id.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("only registered HTTPS QiWe documentation URLs are allowed");
    }
    Ok(())
}

fn safe_restricted_primitive_ref(value: &str) -> bool {
    let Some(relative) = value.strip_prefix("fixtures/qiwe/event-mappings/_primitives/") else {
        return false;
    };
    relative.ends_with(".primitive.json")
        && value.len() <= 512
        && relative.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment.len() <= 128
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
}

pub(crate) fn replay_registered_fixtures(
    provider: &str,
    selector: &Value,
    extractor: &Value,
) -> Result<Value> {
    replay_registered_fixture_sources(
        provider,
        selector,
        extractor,
        EMBEDDED_EVENT_MAPPING_SOURCES,
        EMBEDDED_EVENT_FIXTURE_SOURCES,
        EMBEDDED_EVENT_EXPECTATION_SOURCES,
    )
}

pub(crate) fn registered_mapping_source_sha256(
    provider: &str,
    definition_key: &str,
) -> Result<Option<String>> {
    if provider != "qiwe" || !safe_mapping_identifier(definition_key) {
        bail!("registered event mapping identity is invalid");
    }
    let suites = load_registered_fixture_suites(
        EMBEDDED_EVENT_MAPPING_SOURCES,
        EMBEDDED_EVENT_FIXTURE_SOURCES,
        EMBEDDED_EVENT_EXPECTATION_SOURCES,
    )?;
    let mut matches = suites.into_iter().filter(|suite| {
        suite.mapping.provider == provider && suite.mapping.definition_key == definition_key
    });
    let source_sha256 = matches.next().map(|suite| suite.mapping.source_sha256);
    if matches.next().is_some() {
        bail!("registered event mapping identity is ambiguous");
    }
    Ok(source_sha256)
}

fn replay_registered_fixture_sources(
    provider: &str,
    selector: &Value,
    extractor: &Value,
    mapping_sources: &[EmbeddedJsonSource],
    fixture_sources: &[EmbeddedJsonSource],
    expectation_sources: &[EmbeddedJsonSource],
) -> Result<Value> {
    let (selector, extractor) = parse_definition(selector.clone(), extractor.clone())?;
    let suites =
        load_registered_fixture_suites(mapping_sources, fixture_sources, expectation_sources)?;
    let matching_suites = suites
        .iter()
        .filter(|suite| {
            suite.mapping.provider == provider
                && suite.mapping.selector == selector
                && suite.mapping.extractor == extractor
        })
        .collect::<Vec<_>>();
    if matching_suites.is_empty() {
        bail!(
            "event mapping has no registered trusted fixture suite; create a bounded programming extension"
        );
    }

    let mut fixture_count = 0usize;
    let mut input_item_count = 0usize;
    let mut expected_event_count = 0usize;
    for suite in &matching_suites {
        for fixture in &suite.fixtures {
            let events = replay_mapping(&selector, &extractor, &fixture.event)
                .with_context(|| format!("replay registered fixture {}", fixture.fixture_path))?;
            if events != fixture.expected_events {
                bail!(
                    "registered fixture output mismatch for {}",
                    fixture.fixture_path
                );
            }
            fixture_count += 1;
            input_item_count += expand_data_items(&fixture.event)?.len();
            expected_event_count += events.len();
        }
    }
    let negative_record_count = input_item_count
        .checked_sub(expected_event_count)
        .context("registered fixture emitted more events than input records")?;

    Ok(json!({
        "schema": "registered-event-fixture-replay-v2",
        "provider": provider,
        "event_type": extractor.event_type,
        "fixture_replay_passed": true,
        "registered_mapping_count": matching_suites.len(),
        "fixture_count": fixture_count,
        "positive_fixture_count": expected_event_count,
        "negative_fixture_count": negative_record_count,
        "all_data_items_processed": true,
        "all_expected_events_matched": true,
        "evidence_source": "sidecar_registered_fixtures",
        "real_event_verified": false
    }))
}

fn load_registered_fixture_suites(
    mapping_sources: &[EmbeddedJsonSource],
    fixture_sources: &[EmbeddedJsonSource],
    expectation_sources: &[EmbeddedJsonSource],
) -> Result<Vec<RegisteredFixtureSuite>> {
    validate_embedded_source_set(
        "mapping",
        mapping_sources,
        MAX_MAPPING_SOURCE_BYTES,
        MAX_MAPPING_AGGREGATE_BYTES,
    )?;
    validate_embedded_source_set(
        "fixture",
        fixture_sources,
        MAX_FIXTURE_SOURCE_BYTES,
        MAX_FIXTURE_AGGREGATE_BYTES,
    )?;
    validate_embedded_source_set(
        "expectation",
        expectation_sources,
        MAX_EXPECTATION_SOURCE_BYTES,
        MAX_EXPECTATION_AGGREGATE_BYTES,
    )?;

    let mut mappings = Vec::with_capacity(mapping_sources.len());
    for source in mapping_sources {
        let document: RegisteredMappingDocument =
            parse_embedded_document(source, MAX_MAPPING_SOURCE_BYTES)
                .with_context(|| format!("parse registered mapping {}", source.path))?;
        validate_registered_mapping_document(&document, source.path)?;
        let (selector, extractor) = parse_definition(document.selector, document.extractor)
            .with_context(|| format!("validate registered mapping {}", source.path))?;
        validate_registered_space_extractor(&document.provider, &extractor, source.path)?;
        mappings.push(RegisteredMapping {
            path: source.path.to_string(),
            provider: document.provider,
            definition_key: document.definition_key,
            source_sha256: format!("{:x}", Sha256::digest(source.contents.as_bytes())),
            selector,
            extractor,
        });
    }

    let mapping_paths = mappings
        .iter()
        .enumerate()
        .map(|(index, mapping)| (mapping.path.clone(), index))
        .collect::<HashMap<_, _>>();
    let mut fixtures = Vec::with_capacity(fixture_sources.len());
    for source in fixture_sources {
        let document: RegisteredFixtureDocument =
            parse_embedded_document(source, MAX_FIXTURE_SOURCE_BYTES)
                .with_context(|| format!("parse registered fixture {}", source.path))?;
        if !document.fixture_metadata.sanitized || !document.fixture_metadata.synthetic {
            bail!(
                "registered fixture must assert sanitized and synthetic: {}",
                source.path
            );
        }
        if !mapping_paths.contains_key(&document.fixture_metadata.mapping_ref) {
            bail!(
                "registered fixture references an unknown mapping: {}",
                source.path
            );
        }
        if !document.event.is_object() {
            bail!(
                "registered fixture event must be an object: {}",
                source.path
            );
        }
        expand_data_items(&document.event)
            .with_context(|| format!("validate registered fixture {}", source.path))?;
        fixtures.push(RegisteredFixture {
            path: source.path.to_string(),
            mapping_ref: document.fixture_metadata.mapping_ref,
            event: document.event,
        });
    }

    let fixture_paths = fixtures
        .iter()
        .map(|fixture| fixture.path.clone())
        .collect::<HashSet<_>>();
    let mut expectations = Vec::with_capacity(expectation_sources.len());
    for source in expectation_sources {
        let document: RegisteredExpectationDocument =
            parse_embedded_document(source, MAX_EXPECTATION_SOURCE_BYTES)
                .with_context(|| format!("parse registered expectation {}", source.path))?;
        let metadata = document.expectation_metadata;
        if !metadata.sanitized || !metadata.synthetic {
            bail!(
                "registered expectation must assert sanitized and synthetic: {}",
                source.path
            );
        }
        if !mapping_paths.contains_key(&metadata.mapping_ref)
            || !fixture_paths.contains(&metadata.fixture_ref)
        {
            bail!(
                "registered expectation references an unknown mapping or fixture: {}",
                source.path
            );
        }
        if document.events.is_empty() || document.events.len() > MAX_DATA_ITEMS {
            bail!(
                "registered expectation event count is outside bounded limits: {}",
                source.path
            );
        }
        let events = document
            .events
            .into_iter()
            .map(|event| canonicalize_registered_expectation(event, source.path))
            .collect::<Result<Vec<_>>>()?;
        expectations.push(RegisteredExpectation {
            mapping_ref: metadata.mapping_ref,
            fixture_ref: metadata.fixture_ref,
            events,
        });
    }

    let mut expectations_by_fixture: HashMap<String, Vec<&RegisteredExpectation>> = HashMap::new();
    for expectation in &expectations {
        expectations_by_fixture
            .entry(expectation.fixture_ref.clone())
            .or_default()
            .push(expectation);
    }
    let mut fixtures_by_mapping: HashMap<String, Vec<RegisteredFixtureCase>> = HashMap::new();
    for fixture in fixtures {
        let matching_expectations = expectations_by_fixture
            .get(&fixture.path)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if matching_expectations.len() != 1 {
            bail!(
                "registered fixture must have exactly one expectation: {}",
                fixture.path
            );
        }
        let expectation = matching_expectations[0];
        if expectation.mapping_ref != fixture.mapping_ref {
            bail!(
                "registered fixture and expectation mapping references differ: {}",
                fixture.path
            );
        }
        let mapping_index = mapping_paths[&fixture.mapping_ref];
        if expectation
            .events
            .iter()
            .any(|event| event.event_type != mappings[mapping_index].extractor.event_type)
        {
            bail!(
                "registered expectation event type differs from its mapping: {}",
                fixture.path
            );
        }
        fixtures_by_mapping
            .entry(fixture.mapping_ref.clone())
            .or_default()
            .push(RegisteredFixtureCase {
                fixture_path: fixture.path,
                event: fixture.event,
                expected_events: expectation.events.clone(),
            });
    }

    if expectations_by_fixture
        .keys()
        .any(|fixture_path| !fixture_paths.contains(fixture_path))
    {
        bail!("registered expectation references a fixture outside the registry");
    }

    mappings
        .into_iter()
        .map(|mapping| {
            let fixtures = fixtures_by_mapping.remove(&mapping.path).with_context(|| {
                format!(
                    "registered mapping has no fixture bundle: {}:{}",
                    mapping.provider, mapping.definition_key
                )
            })?;
            validate_fixture_suite_negative_coverage(&mapping, &fixtures)?;
            Ok(RegisteredFixtureSuite { mapping, fixtures })
        })
        .collect()
}

fn validate_fixture_suite_negative_coverage(
    mapping: &RegisteredMapping,
    fixtures: &[RegisteredFixtureCase],
) -> Result<()> {
    let mut input_record_count = 0usize;
    let mut emitted_event_count = 0usize;
    for fixture in fixtures {
        let input_count = expand_data_items(&fixture.event)?.len();
        let actual = replay_mapping(&mapping.selector, &mapping.extractor, &fixture.event)
            .with_context(|| format!("replay registered fixture {}", fixture.fixture_path))?;
        if actual != fixture.expected_events {
            bail!(
                "registered fixture output mismatch for {}",
                fixture.fixture_path
            );
        }
        input_record_count = input_record_count
            .checked_add(input_count)
            .context("registered fixture input count overflowed")?;
        emitted_event_count = emitted_event_count
            .checked_add(actual.len())
            .context("registered fixture output count overflowed")?;
    }
    if input_record_count <= emitted_event_count {
        bail!(
            "registered mapping fixture suite must include at least one negative input record: {}",
            mapping.path
        );
    }
    Ok(())
}

fn validate_registered_space_extractor(
    provider: &str,
    extractor: &ExtractorSpec,
    source_path: &str,
) -> Result<()> {
    if provider == "qiwe"
        && (extractor.space_chat_id.pointer != "/fromRoomId"
            || extractor.space_chat_id.transforms.as_slice() != [Transform::OpaqueId])
    {
        bail!("registered QiWe mapping must bind Space only to /fromRoomId: {source_path}");
    }
    Ok(())
}

fn validate_registered_mapping_document(
    document: &RegisteredMappingDocument,
    path: &str,
) -> Result<()> {
    if document.schema_version != 1
        || document.provider != "qiwe"
        || !safe_mapping_identifier(&document.definition_key)
    {
        bail!("registered mapping metadata is invalid: {path}");
    }
    if document.official_sources.is_empty() || document.official_sources.len() > 8 {
        bail!("registered mapping official source count is invalid: {path}");
    }
    let mut sources = HashSet::new();
    for source in &document.official_sources {
        if !sources.insert(source) {
            bail!("registered mapping official sources are duplicated: {path}");
        }
        let url = url::Url::parse(source)
            .with_context(|| format!("parse registered mapping official source: {path}"))?;
        if url.scheme() != "https"
            || url.host_str() != Some("doc.qiweapi.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
        {
            bail!("registered mapping source is not an approved official URL: {path}");
        }
    }
    Ok(())
}

fn safe_mapping_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(character) if character.is_ascii_lowercase() || character.is_ascii_digit())
        && value.len() <= MAX_IDENTIFIER_BYTES
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || "._:-".contains(character)
        })
}

fn canonicalize_registered_expectation(
    event: RegisteredExpectedEvent,
    source_path: &str,
) -> Result<CanonicalChannelEvent> {
    if !safe_mapping_identifier(&event.event_type) {
        bail!("registered expectation event type is invalid: {source_path}");
    }
    let event_id =
        bounded_registered_id(event.event_id, MAX_OPAQUE_ID_BYTES, "event id", source_path)?;
    let space_chat_id =
        bounded_registered_id(event.space_id, MAX_OPAQUE_ID_BYTES, "Space id", source_path)?;
    if space_chat_id == "0" {
        bail!("registered expectation Space id cannot be zero: {source_path}");
    }
    if event.subject_user_ids.is_empty() || event.subject_user_ids.len() > MAX_SUBJECTS {
        bail!("registered expectation subject count is invalid: {source_path}");
    }
    let mut subjects = HashSet::new();
    let subject_user_ids = event
        .subject_user_ids
        .into_iter()
        .map(|subject| {
            let subject =
                bounded_registered_id(subject, MAX_OPAQUE_ID_BYTES, "subject id", source_path)?;
            if !subjects.insert(subject.clone()) {
                bail!("registered expectation subject ids are duplicated: {source_path}");
            }
            Ok(subject)
        })
        .collect::<Result<Vec<_>>>()?;
    let occurred_at = DateTime::parse_from_rfc3339(&event.occurred_at)
        .with_context(|| format!("parse registered expectation occurred_at: {source_path}"))?
        .with_timezone(&Utc);
    Ok(CanonicalChannelEvent {
        event_type: event.event_type,
        event_id,
        space_chat_id,
        subject_user_ids,
        occurred_at,
    })
}

fn bounded_registered_id(
    value: String,
    maximum: usize,
    field: &str,
    source_path: &str,
) -> Result<String> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        bail!("registered expectation {field} is invalid: {source_path}");
    }
    Ok(value)
}

fn replay_mapping(
    selector: &Predicate,
    extractor: &ExtractorSpec,
    payload: &Value,
) -> Result<Vec<CanonicalChannelEvent>> {
    let mut events = Vec::new();
    for item in expand_data_items(payload)? {
        if evaluate_predicate(selector, &item, 0)? {
            events.push(extract_event(extractor, &item)?);
        }
    }
    Ok(events)
}

pub(crate) fn validate_definition(selector: &Value, extractor: &Value) -> Result<()> {
    parse_definition(selector.clone(), extractor.clone()).map(|_| ())
}

#[cfg(test)]
pub(crate) fn replay_definition(
    selector: &Value,
    extractor: &Value,
    payload: &Value,
) -> Result<Vec<CanonicalChannelEvent>> {
    let (selector, extractor) = parse_definition(selector.clone(), extractor.clone())?;
    let mut events = Vec::new();
    for item in expand_data_items(payload)? {
        if evaluate_predicate(&selector, &item, 0)? {
            events.push(extract_event(&extractor, &item)?);
        }
    }
    Ok(events)
}

fn parse_definition(selector: Value, extractor: Value) -> Result<(Predicate, ExtractorSpec)> {
    validate_selector_shape(&selector, 0)?;
    validate_extractor_shape(&extractor)?;
    let selector = serde_json::from_value(selector).context("parse channel event selector")?;
    validate_predicate(&selector, 0, &mut 0usize)?;
    let extractor = serde_json::from_value(extractor).context("parse channel event extractor")?;
    validate_extractor(&extractor)?;
    Ok((selector, extractor))
}

fn validate_selector_shape(value: &Value, depth: usize) -> Result<()> {
    if depth > MAX_PREDICATE_DEPTH {
        bail!("event selector nesting exceeds {MAX_PREDICATE_DEPTH}");
    }
    let object = value
        .as_object()
        .context("event selector rule must be an object")?;
    let op = object
        .get("op")
        .and_then(Value::as_str)
        .context("event selector rule op is required")?;
    let allowed = match op {
        "equals" => &["op", "pointer", "value"][..],
        "in" => &["op", "pointer", "values"][..],
        "exists" | "type_is" => &["op", "pointer", "value"][..],
        "all" | "any" => &["op", "rules"][..],
        _ => bail!("event selector op is not allowed"),
    };
    reject_unknown_fields(object, allowed, "event selector rule")?;
    if matches!(op, "all" | "any") {
        let rules = object
            .get("rules")
            .and_then(Value::as_array)
            .context("event selector rules must be an array")?;
        if rules.is_empty() || rules.len() > MAX_PREDICATES {
            bail!("event selector boolean group size is invalid");
        }
        for rule in rules {
            validate_selector_shape(rule, depth + 1)?;
        }
    }
    Ok(())
}

fn validate_extractor_shape(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .context("event extractor must be an object")?;
    reject_unknown_fields(
        object,
        &[
            "event_type",
            "event_id",
            "space_chat_id",
            "subject_user_ids",
            "occurred_at",
        ],
        "event extractor",
    )?;
    for key in [
        "event_id",
        "space_chat_id",
        "subject_user_ids",
        "occurred_at",
    ] {
        let field = object
            .get(key)
            .and_then(Value::as_object)
            .with_context(|| format!("event extractor {key} must be an object"))?;
        reject_unknown_fields(field, &["pointer", "transforms"], "event extractor field")?;
        if let Some(transforms) = field.get("transforms") {
            let transforms = transforms
                .as_array()
                .context("event extractor transforms must be an array")?;
            if transforms.len() > MAX_TRANSFORMS {
                bail!("event extractor has too many transforms");
            }
            for transform in transforms {
                validate_transform_shape(transform)?;
            }
        }
    }
    Ok(())
}

fn validate_transform_shape(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .context("event extractor transform must be an object")?;
    let op = object
        .get("op")
        .and_then(Value::as_str)
        .context("event extractor transform op is required")?;
    let allowed = match op {
        "base64_utf8" | "dedupe" | "opaque_id" => &["op"][..],
        "split" => &["op", "delimiter", "max_parts"][..],
        "unix_timestamp" => &["op", "milliseconds"][..],
        "restricted_primitive" => &["op", "primitive_ref"][..],
        _ => bail!("event extractor transform op is not allowed"),
    };
    reject_unknown_fields(object, allowed, "event extractor transform")
}

fn reject_unknown_fields(
    object: &serde_json::Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> Result<()> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        bail!("{context} contains an unknown field");
    }
    Ok(())
}

pub(crate) async fn process_persisted_raw_event(
    pool: &PgPool,
    raw_event_id: Uuid,
    raw_event: &RawQiweEvent,
) -> Result<()> {
    if !raw_event.ingress_auth_verified {
        return Ok(());
    }
    let persisted_space_id = load_authenticated_raw_event_space(pool, raw_event_id).await?;
    let Some(persisted_space_id) = persisted_space_id else {
        warn!(
            raw_event_id = %raw_event_id,
            "authenticated channel event has no persisted Space binding"
        );
        return Ok(());
    };
    let mappings = load_mappings(pool, &raw_event.source).await?;
    if mappings.is_empty() {
        return Ok(());
    }

    let items = match expand_data_items(&raw_event.payload) {
        Ok(items) => items,
        Err(error) => {
            warn!(
                raw_event_id = %raw_event_id,
                error = %error,
                "channel event payload cannot be interpreted; raw capture remains acknowledged"
            );
            return Ok(());
        }
    };

    for mapping in mappings {
        for item in &items {
            let matched = match evaluate_predicate(&mapping.selector, item, 0) {
                Ok(matched) => matched,
                Err(error) => {
                    warn!(mapping_id = %mapping.id, error = %error, "invalid channel event selector");
                    continue;
                }
            };
            if !matched {
                continue;
            }
            let canonical = match extract_event(&mapping.extractor, item) {
                Ok(event) => event,
                Err(error) => {
                    warn!(
                        mapping_id = %mapping.id,
                        raw_event_id = %raw_event_id,
                        error = %error,
                        "channel event extraction failed closed"
                    );
                    continue;
                }
            };
            let Some(space_id) =
                resolve_space(pool, &mapping.provider, &canonical.space_chat_id).await?
            else {
                warn!(
                    mapping_id = %mapping.id,
                    room_ref = %sha256_marker(&canonical.space_chat_id),
                    "channel event room does not resolve to a known Space"
                );
                continue;
            };
            if space_id != persisted_space_id {
                warn!(
                    mapping_id = %mapping.id,
                    raw_event_id = %raw_event_id,
                    persisted_space_ref = %sha256_marker(&persisted_space_id.to_string()),
                    extracted_space_ref = %sha256_marker(&space_id.to_string()),
                    "channel event mapping attempted to cross the persisted Space boundary"
                );
                continue;
            }

            if mapping.status == "shadow" {
                create_shadow_observation(
                    pool,
                    space_id,
                    raw_event_id,
                    &mapping,
                    &canonical,
                    "mapping_shadow",
                    None,
                )
                .await?;
                dispatch_event_automations(pool, space_id, raw_event_id, &mapping, &canonical)
                    .await?;
                continue;
            }

            dispatch_event_automations(pool, space_id, raw_event_id, &mapping, &canonical).await?;
        }
    }
    Ok(())
}

async fn load_authenticated_raw_event_space(
    pool: &PgPool,
    raw_event_id: Uuid,
) -> Result<Option<Uuid>> {
    let row = sqlx::query(
        r#"
        SELECT space_id
        FROM qintopia_messages.raw_events
        WHERE id = $1
          AND ingress_auth_verified
        "#,
    )
    .bind(raw_event_id)
    .fetch_optional(pool)
    .await
    .context("load authenticated raw event Space binding")?;
    row.map(|row| row.try_get("space_id"))
        .transpose()
        .context("read authenticated raw event Space binding")
        .map(Option::flatten)
}

async fn load_mappings(pool: &PgPool, provider: &str) -> Result<Vec<MappingVersion>> {
    let rows = sqlx::query(
        r#"
        SELECT id, provider, definition_key, version, definition_digest,
               selector, extractor, status
        FROM qintopia_agent_os.channel_event_mapping_versions
        WHERE provider = $1 AND status IN ('shadow', 'active')
        ORDER BY definition_key, version DESC
        "#,
    )
    .bind(provider)
    .fetch_all(pool)
    .await
    .context("load active channel event mappings")?;

    let mut mappings = Vec::with_capacity(rows.len());
    for row in rows {
        let id: Uuid = row.try_get("id")?;
        let provider: String = row.try_get("provider")?;
        let definition_key: String = row.try_get("definition_key")?;
        let version: i32 = row.try_get("version")?;
        let definition_digest: String = row.try_get("definition_digest")?;
        let status: String = row.try_get("status")?;
        let selector_value: Value = row.try_get("selector")?;
        let extractor_value: Value = row.try_get("extractor")?;

        let parsed = parse_definition(selector_value, extractor_value);
        let (selector, extractor) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                warn!(
                    mapping_id = %id,
                    mapping_key = %definition_key,
                    mapping_version = version,
                    error = %error,
                    "invalid stored channel event mapping skipped"
                );
                continue;
            }
        };

        mappings.push(MappingVersion {
            id,
            provider,
            definition_key,
            version,
            definition_digest,
            selector,
            extractor,
            status,
        });
    }
    Ok(mappings)
}

async fn resolve_space(pool: &PgPool, provider: &str, chat_id: &str) -> Result<Option<Uuid>> {
    sqlx::query_scalar(
        r#"
        SELECT id
        FROM qintopia_messages.conversations
        WHERE tenant_id = 'qintopia'
          AND platform = $1
          AND chat_id = $2
          AND chat_type = 'group'
          AND status = 'active'
        "#,
    )
    .bind(provider)
    .bind(chat_id)
    .fetch_optional(pool)
    .await
    .context("resolve event room to Space")
}

async fn dispatch_event_automations(
    pool: &PgPool,
    space_id: Uuid,
    raw_event_id: Uuid,
    mapping: &MappingVersion,
    event: &CanonicalChannelEvent,
) -> Result<()> {
    let rows = sqlx::query(
        r#"
        SELECT automation.id, automation.definition_key, automation.version,
               automation.definition_digest AS automation_digest,
               automation.business_definition_id, automation.status,
               (SELECT business.definition_digest
                FROM qintopia_agent_os.business_definition_versions business
                WHERE business.id = automation.business_definition_id
                  AND business.space_id = automation.space_id
                  AND business.status = 'active') AS business_digest,
               (SELECT policy.id
                FROM qintopia_agent_os.space_policy_versions policy
                WHERE policy.space_id = automation.space_id
                  AND policy.definition_key = 'default'
                  AND policy.status = 'active') AS policy_id,
               (SELECT policy.definition_digest
                FROM qintopia_agent_os.space_policy_versions policy
                WHERE policy.space_id = automation.space_id
                  AND policy.definition_key = 'default'
                  AND policy.status = 'active') AS policy_digest
        FROM qintopia_agent_os.automation_definition_versions automation
        JOIN qintopia_messages.raw_events raw_event
          ON raw_event.id = $3
         AND raw_event.space_id = automation.space_id
         AND raw_event.ingress_auth_verified
        JOIN qintopia_agent_os.channel_event_mapping_versions stored_mapping
          ON stored_mapping.id = automation.channel_event_mapping_id
        WHERE automation.space_id = $1
          AND automation.channel_event_mapping_id = $2
          AND automation.trigger_kind = 'event'
          AND automation.status IN ('shadow', 'active')
          AND (
              (
                  automation.status = 'shadow'
                  AND raw_event.created_at > automation.created_at
                  AND raw_event.created_at > stored_mapping.created_at
              )
              OR (
                  automation.activated_at IS NOT NULL
                  AND raw_event.created_at > automation.activated_at
                  AND EXISTS (
                      SELECT 1
                      FROM qintopia_agent_os.business_definition_versions business
                      JOIN qintopia_agent_os.space_policy_versions policy
                        ON policy.space_id = business.space_id
                       AND policy.definition_key = 'default'
                       AND policy.status = 'active'
                      JOIN qintopia_agent_os.capabilities selected
                        ON selected.capability_key = CASE business.execution_mode
                            WHEN 'deterministic' THEN business.definition->>'capability_key'
                            WHEN 'agent_turn' THEN 'erhua.space_agent_turn'
                            ELSE NULL
                          END
                       AND selected.enabled
                       AND selected.provider_agent = 'erhua'
                       AND selected.metadata ->> 'space_invocable' = 'true'
                       AND selected.metadata ->> 'space_scope_binding' = 'work_item_space_id'
                       AND selected.metadata ->> 'invocation_boundary' = 'erhua.execute_space_business'
                       AND (business.execution_mode <> 'agent_turn' OR $4::boolean)
                       AND 'system' = ANY(selected.allowed_callers)
                       AND (
                            (business.execution_mode = 'deterministic'
                             AND selected.metadata ? 'space_execution_recipe'
                             AND 'space_automation_run' = ANY(selected.allowed_work_item_types)) OR
                            (business.execution_mode = 'agent_turn'
                             AND 'space_agent_turn' = ANY(selected.allowed_work_item_types))
                          )
                      WHERE business.id = automation.business_definition_id
                        AND business.space_id = automation.space_id
                        AND business.status = 'active'
                        AND selected.capability_key = ANY(business.allowed_capabilities)
                        AND COALESCE(policy.policy_config->'capability_grants', '[]'::jsonb)
                            ? selected.capability_key
                  )
                  AND EXISTS (
                      SELECT 1
                      FROM qintopia_agent_os.channel_event_mapping_versions live_mapping
                      WHERE live_mapping.id = automation.channel_event_mapping_id
                        AND live_mapping.status = 'active'
                  )
                  AND EXISTS (
                      SELECT 1 FROM qintopia_agent_os.capabilities capability
                      WHERE capability.capability_key = 'erhua.execute_space_business'
                        AND capability.enabled
                        AND 'system' = ANY(capability.allowed_callers)
                        AND 'space_automation_run' = ANY(capability.allowed_work_item_types)
                  )
              )
          )
        ORDER BY automation.definition_key, automation.version DESC
        "#,
    )
    .bind(space_id)
    .bind(mapping.id)
    .bind(raw_event_id)
    .bind(crate::space_agent_turn::runtime_readiness_is_approved())
    .fetch_all(pool)
    .await
    .context("load Space event automations")?;

    for row in rows {
        let automation_id: Uuid = row.try_get("id")?;
        let automation_key: String = row.try_get("definition_key")?;
        let automation_version: i32 = row.try_get("version")?;
        let business_definition_id: Uuid = row.try_get("business_definition_id")?;
        let automation_digest: String = row.try_get("automation_digest")?;
        let status: String = row.try_get("status")?;
        if status == "shadow" {
            create_shadow_observation(
                pool,
                space_id,
                raw_event_id,
                mapping,
                event,
                &format!("automation_shadow:{automation_id}"),
                Some(automation_id),
            )
            .await?;
            continue;
        }

        let provider_event_ref = format!("{}:{}", mapping.provider, event.event_id);
        let business_digest: String = row
            .try_get::<Option<String>, _>("business_digest")?
            .context("active event automation business digest is missing")?;
        let policy_id: Uuid = row
            .try_get::<Option<Uuid>, _>("policy_id")?
            .context("active event automation policy id is missing")?;
        let policy_digest: String = row
            .try_get::<Option<String>, _>("policy_digest")?
            .context("active event automation policy digest is missing")?;
        let idempotency_key =
            event_automation_idempotency_key(space_id, &automation_key, &provider_event_ref);
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (space_id, work_item_type, status, requester_agent, target_agent,
                 capability_key, human_owner, priority, available_at, brief_summary,
                 purpose, source_type, source_refs, dedupe_key, idempotency_key,
                 risk_level, information_class, payload, payload_redaction_policy,
                 review_policy, metadata)
            VALUES
                ($1, 'space_automation_run', 'queued', 'system', 'erhua',
                 'erhua.execute_space_business', '', 'normal', now(),
                 'Execute a confirmed Space event automation.',
                 'space_automation_event', 'space_automation_event', $2,
                 $3, $3, 'medium', 'internal_ops', $4,
                 'summary_only', 'not_required', $5)
            ON CONFLICT (idempotency_key) DO NOTHING
            "#,
        )
        .bind(space_id)
        .bind(json!({
            "raw_event_id": raw_event_id,
            "mapping_version_id": mapping.id,
            "automation_definition_id": automation_id
        }))
        .bind(idempotency_key)
        .bind(json!({
            "automation_definition_id": automation_id,
            "automation_definition_digest": automation_digest,
            "automation_key": automation_key,
            "automation_version": automation_version,
            "business_definition_id": business_definition_id,
            "business_definition_digest": business_digest,
            "space_policy_version_id": policy_id,
            "space_policy_digest": policy_digest,
            "channel_event_mapping_id": mapping.id,
            "channel_event_mapping_digest": mapping.definition_digest,
            "trigger": {
                "kind": "event",
                "event_type": event.event_type,
                "provider_event_ref": provider_event_ref,
                "subject_user_ids": event.subject_user_ids,
                "occurred_at": event.occurred_at.to_rfc3339()
            }
        }))
        .bind(json!({
            "external_send_executed": false,
            "space_bound": true,
            "event_mapping_version_id": mapping.id
        }))
        .execute(pool)
        .await
        .context("create Space event automation work item")?;
    }
    Ok(())
}

async fn create_shadow_observation(
    pool: &PgPool,
    space_id: Uuid,
    raw_event_id: Uuid,
    mapping: &MappingVersion,
    event: &CanonicalChannelEvent,
    scope: &str,
    automation_id: Option<Uuid>,
) -> Result<()> {
    let idempotency_key = shadow_observation_idempotency_key(
        &mapping.provider,
        space_id,
        mapping.id,
        &event.event_id,
        scope,
    );
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_items
            (space_id, work_item_type, status, requester_agent, target_agent,
             capability_key, human_owner, priority, available_at, brief_summary,
             purpose, source_type, source_refs, dedupe_key, idempotency_key,
             risk_level, information_class, payload, payload_redaction_policy,
             review_policy, metadata)
        SELECT
            $1, 'space_event_shadow_observation', 'completed', 'system', 'erhua',
            'erhua.execute_space_business', '', 'low', now(),
            'Observed a Space event mapping in shadow mode.',
            'space_event_shadow', 'space_event_shadow', $2,
            $3, $3, 'low', 'internal_ops', $4,
            'summary_only', 'not_required', $5
        FROM qintopia_messages.raw_events raw_event
        JOIN qintopia_agent_os.channel_event_mapping_versions stored_mapping
          ON stored_mapping.id = $7
         AND stored_mapping.provider = raw_event.source
        WHERE raw_event.id = $6
          AND raw_event.space_id = $1
          AND raw_event.ingress_auth_verified
          AND raw_event.created_at > stored_mapping.created_at
          AND (
              ($8::uuid IS NULL AND stored_mapping.status = 'shadow')
              OR (
                  $8::uuid IS NOT NULL
                  AND stored_mapping.status IN ('shadow', 'active')
                  AND EXISTS (
                      SELECT 1
                      FROM qintopia_agent_os.automation_definition_versions automation
                      WHERE automation.id = $8
                        AND automation.space_id = $1
                        AND automation.channel_event_mapping_id = stored_mapping.id
                        AND automation.status = 'shadow'
                        AND raw_event.created_at > automation.created_at
                  )
              )
          )
        ON CONFLICT (idempotency_key) DO NOTHING
        "#,
    )
    .bind(space_id)
    .bind(json!({
        "raw_event_id": raw_event_id,
        "mapping_version_id": mapping.id
    }))
    .bind(idempotency_key)
    .bind(json!({
        "mapping_definition_key": mapping.definition_key,
        "mapping_version": mapping.version,
        "event_type": event.event_type,
        "decode_success": true,
        "subject_count": event.subject_user_ids.len(),
        "event_ref": sha256_marker(&event.event_id),
        "room_ref": sha256_marker(&event.space_chat_id),
        "observed_at": event.occurred_at.to_rfc3339(),
        "raw_payload_in_evidence": false
    }))
    .bind(json!({
        "external_send_executed": false,
        "shadow_replay_suppressed": true,
        "space_bound": true,
        "scope": scope
    }))
    .bind(raw_event_id)
    .bind(mapping.id)
    .bind(automation_id)
    .execute(pool)
    .await
    .context("record Space event shadow observation")?;
    Ok(())
}

fn expand_data_items(payload: &Value) -> Result<Vec<Value>> {
    let data = match payload {
        Value::Object(object) => object.get("data").unwrap_or(payload),
        _ => payload,
    };
    let parsed;
    let data = if let Value::String(text) = data {
        parsed = parse_strict_bounded_str(text, QIWE_STRING_DATA_LIMITS)
            .context("parse string-encoded data")?;
        &parsed
    } else {
        data
    };
    let items = match data {
        Value::Array(items) => items.clone(),
        Value::Object(_) => vec![data.clone()],
        _ => bail!("channel event data must be an object or array"),
    };
    if items.is_empty() || items.len() > MAX_DATA_ITEMS {
        bail!("channel event data item count is outside 1..={MAX_DATA_ITEMS}");
    }
    if items.iter().any(|item| !item.is_object()) {
        bail!("channel event data items must be objects");
    }
    Ok(items)
}

fn validate_predicate(predicate: &Predicate, depth: usize, count: &mut usize) -> Result<()> {
    if depth > MAX_PREDICATE_DEPTH {
        bail!("event selector nesting exceeds {MAX_PREDICATE_DEPTH}");
    }
    *count += 1;
    if *count > MAX_PREDICATES {
        bail!("event selector exceeds {MAX_PREDICATES} predicates");
    }
    match predicate {
        Predicate::Equals { pointer, value } => {
            validate_pointer(pointer)?;
            validate_predicate_value(value)
        }
        Predicate::In { pointer, values } => {
            validate_pointer(pointer)?;
            if values.is_empty() || values.len() > MAX_PREDICATES {
                bail!("event selector in values count is invalid");
            }
            for value in values {
                validate_predicate_value(value)?;
            }
            Ok(())
        }
        Predicate::Exists { pointer, .. } | Predicate::TypeIs { pointer, .. } => {
            validate_pointer(pointer)
        }
        Predicate::All { rules } | Predicate::Any { rules } => {
            if rules.is_empty() || rules.len() > MAX_PREDICATES {
                bail!("event selector boolean group size is invalid");
            }
            for rule in rules {
                validate_predicate(rule, depth + 1, count)?;
            }
            Ok(())
        }
    }
}

fn validate_predicate_value(value: &Value) -> Result<()> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
        Value::String(text)
            if text.len() <= MAX_PREDICATE_STRING_BYTES
                && !text.bytes().any(|byte| byte.is_ascii_control()) =>
        {
            Ok(())
        }
        _ => bail!("event selector values must be bounded scalars"),
    }
}

fn validate_extractor(extractor: &ExtractorSpec) -> Result<()> {
    if !safe_mapping_identifier(&extractor.event_type) {
        bail!("canonical event_type is invalid");
    }
    let primitives = restricted_primitive_registry()?;
    for field in [
        &extractor.event_id,
        &extractor.space_chat_id,
        &extractor.subject_user_ids,
        &extractor.occurred_at,
    ] {
        validate_pointer(&field.pointer)?;
        if field.transforms.len() > MAX_TRANSFORMS {
            bail!("event extractor has too many transforms");
        }
        let mut expanded_transform_count = field.transforms.len();
        let mut restricted_primitive_count = 0usize;
        for transform in &field.transforms {
            match transform {
                Transform::Split {
                    delimiter,
                    max_parts,
                } => validate_split(delimiter, *max_parts)?,
                Transform::RestrictedPrimitive { primitive_ref } => {
                    if !safe_restricted_primitive_ref(primitive_ref) {
                        bail!("restricted primitive reference is invalid");
                    }
                    let primitive = primitives
                        .get(primitive_ref)
                        .context("restricted primitive is not registered in this release")?;
                    restricted_primitive_count += 1;
                    expanded_transform_count = expanded_transform_count
                        .saturating_add(primitive.operations.len().saturating_sub(1));
                }
                Transform::Base64Utf8
                | Transform::Dedupe
                | Transform::OpaqueId
                | Transform::UnixTimestamp { .. } => {}
            }
        }
        if restricted_primitive_count > 1 {
            bail!("an extractor field may invoke at most one restricted primitive");
        }
        if expanded_transform_count > MAX_EXPANDED_TRANSFORMS {
            bail!("event extractor expanded transform count exceeds the limit");
        }
    }
    Ok(())
}

fn validate_split(delimiter: &str, max_parts: usize) -> Result<()> {
    if delimiter.is_empty()
        || delimiter.len() > 8
        || !delimiter.is_ascii()
        || delimiter.bytes().any(|byte| byte.is_ascii_control())
        || !(1..=MAX_SUBJECTS).contains(&max_parts)
    {
        bail!("split transform is outside bounded limits");
    }
    Ok(())
}

fn validate_pointer(pointer: &str) -> Result<()> {
    if pointer.is_empty()
        || pointer.len() > MAX_JSON_POINTER_BYTES
        || !pointer.starts_with('/')
        || pointer.bytes().any(|byte| byte.is_ascii_control())
        || has_invalid_pointer_escape(pointer)
    {
        bail!("JSON Pointer is invalid");
    }
    Ok(())
}

fn has_invalid_pointer_escape(pointer: &str) -> bool {
    let mut bytes = pointer.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
            return true;
        }
    }
    false
}

fn evaluate_predicate(predicate: &Predicate, item: &Value, depth: usize) -> Result<bool> {
    if depth > MAX_PREDICATE_DEPTH {
        bail!("event selector nesting exceeds limit");
    }
    Ok(match predicate {
        Predicate::Equals { pointer, value } => item.pointer(pointer) == Some(value),
        Predicate::In { pointer, values } => item
            .pointer(pointer)
            .is_some_and(|candidate| values.iter().any(|value| value == candidate)),
        Predicate::Exists { pointer, value } => item.pointer(pointer).is_some() == *value,
        Predicate::TypeIs { pointer, value } => item
            .pointer(pointer)
            .is_some_and(|candidate| json_type_matches(candidate, *value)),
        Predicate::All { rules } => rules
            .iter()
            .map(|rule| evaluate_predicate(rule, item, depth + 1))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .all(|matched| matched),
        Predicate::Any { rules } => rules
            .iter()
            .map(|rule| evaluate_predicate(rule, item, depth + 1))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .any(|matched| matched),
    })
}

fn json_type_matches(value: &Value, expected: JsonType) -> bool {
    matches!(
        (value, expected),
        (Value::Null, JsonType::Null)
            | (Value::Bool(_), JsonType::Boolean)
            | (Value::Number(_), JsonType::Number)
            | (Value::String(_), JsonType::String)
            | (Value::Array(_), JsonType::Array)
            | (Value::Object(_), JsonType::Object)
    )
}

fn extract_event(extractor: &ExtractorSpec, item: &Value) -> Result<CanonicalChannelEvent> {
    validate_extractor(extractor)?;
    let event_id = extract_scalar_string(
        &apply_extractor(&extractor.event_id, item)?,
        MAX_OPAQUE_ID_BYTES,
    )?;
    let space_chat_id = extract_scalar_string(
        &apply_extractor(&extractor.space_chat_id, item)?,
        MAX_OPAQUE_ID_BYTES,
    )?;
    if space_chat_id == "0" {
        bail!("event Space chat id cannot be zero");
    }
    let subject_value = apply_extractor(&extractor.subject_user_ids, item)?;
    let subject_user_ids = subject_value
        .as_array()
        .context("subject_user_ids must be an array after transforms")?
        .iter()
        .map(|value| extract_scalar_string(value, MAX_OPAQUE_ID_BYTES))
        .collect::<Result<Vec<_>>>()?;
    if subject_user_ids.is_empty() || subject_user_ids.len() > MAX_SUBJECTS {
        bail!("subject_user_ids count is outside bounded limits");
    }
    let occurred_value = apply_extractor(&extractor.occurred_at, item)?;
    let occurred_at = DateTime::parse_from_rfc3339(&extract_scalar_string(&occurred_value, 64)?)
        .context("occurred_at is not RFC3339 after transforms")?
        .with_timezone(&Utc);
    Ok(CanonicalChannelEvent {
        event_type: extractor.event_type.clone(),
        event_id,
        space_chat_id,
        subject_user_ids,
        occurred_at,
    })
}

fn apply_extractor(extractor: &ValueExtractor, item: &Value) -> Result<Value> {
    let mut value = item
        .pointer(&extractor.pointer)
        .cloned()
        .with_context(|| format!("required pointer {} is missing", extractor.pointer))?;
    for transform in &extractor.transforms {
        value = apply_transform(value, transform)?;
    }
    Ok(value)
}

fn apply_transform(value: Value, transform: &Transform) -> Result<Value> {
    match transform {
        Transform::Base64Utf8 => decode_base64_utf8(value),
        Transform::Split {
            delimiter,
            max_parts,
        } => split_value(value, delimiter, *max_parts),
        Transform::Dedupe => {
            let values = value.as_array().context("dedupe requires an array")?;
            let mut seen = std::collections::BTreeSet::new();
            let mut result = Vec::new();
            for value in values {
                let normalized = opaque_id(value)?;
                if seen.insert(normalized.clone()) {
                    result.push(Value::String(normalized));
                }
            }
            Ok(Value::Array(result))
        }
        Transform::OpaqueId => match value {
            Value::Array(values) => Ok(Value::Array(
                values
                    .iter()
                    .map(opaque_id)
                    .map(|value| value.map(Value::String))
                    .collect::<Result<Vec<_>>>()?,
            )),
            value => Ok(Value::String(opaque_id(&value)?)),
        },
        Transform::UnixTimestamp { milliseconds } => {
            let raw = opaque_id(&value)?;
            let timestamp = raw
                .parse::<i64>()
                .context("unix timestamp is not an integer")?;
            let datetime = if *milliseconds {
                Utc.timestamp_millis_opt(timestamp).single()
            } else {
                Utc.timestamp_opt(timestamp, 0).single()
            }
            .context("unix timestamp is outside supported range")?;
            Ok(Value::String(datetime.to_rfc3339()))
        }
        Transform::RestrictedPrimitive { primitive_ref } => {
            let registry = restricted_primitive_registry()?;
            let primitive = registry
                .get(primitive_ref)
                .context("restricted primitive is not registered in this release")?;
            apply_restricted_primitive(value, primitive)
        }
    }
}

fn apply_restricted_primitive(mut value: Value, primitive: &RestrictedPrimitive) -> Result<Value> {
    for operation in &primitive.operations {
        value = match operation {
            RestrictedPrimitiveOperation::Base64Utf8 => decode_base64_utf8(value)?,
            RestrictedPrimitiveOperation::JsonParse => parse_strict_bounded_json(value)?,
            RestrictedPrimitiveOperation::JsonPointer { pointer } => value
                .pointer(pointer)
                .cloned()
                .with_context(|| format!("restricted primitive pointer {pointer} is missing"))?,
            RestrictedPrimitiveOperation::Split {
                delimiter,
                max_parts,
            } => split_value(value, delimiter, *max_parts)?,
            RestrictedPrimitiveOperation::StringTrim => {
                let text = value
                    .as_str()
                    .context("string_trim requires a string")?
                    .trim();
                if text.len() > MAX_RESTRICTED_JSON_STRING_BYTES {
                    bail!("string_trim output exceeds limit");
                }
                Value::String(text.to_string())
            }
            RestrictedPrimitiveOperation::ArrayFlatten => flatten_array_once(value)?,
        };
    }
    Ok(value)
}

fn decode_base64_utf8(value: Value) -> Result<Value> {
    let encoded = value.as_str().context("base64_utf8 requires a string")?;
    if encoded.len() > MAX_RESTRICTED_JSON_STRING_BYTES {
        bail!("base64_utf8 input exceeds limit");
    }
    let bytes =
        Base64::decode_vec(encoded).map_err(|_| anyhow::anyhow!("strict base64 decode failed"))?;
    let decoded = String::from_utf8(bytes).context("base64 value is not UTF-8")?;
    Ok(Value::String(decoded))
}

fn split_value(value: Value, delimiter: &str, max_parts: usize) -> Result<Value> {
    validate_split(delimiter, max_parts)?;
    let text = value.as_str().context("split requires a string")?;
    if text.len() > MAX_RESTRICTED_JSON_STRING_BYTES {
        bail!("split input exceeds limit");
    }
    let parts = text
        .split(delimiter)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .take(max_parts + 1)
        .collect::<Vec<_>>();
    if parts.len() > max_parts {
        bail!("split output exceeds max_parts");
    }
    Ok(Value::Array(
        parts
            .into_iter()
            .map(|part| Value::String(part.to_string()))
            .collect(),
    ))
}

fn parse_strict_bounded_json(value: Value) -> Result<Value> {
    let text = value.as_str().context("json_parse requires a string")?;
    let parsed = parse_strict_bounded_str(
        text,
        JsonLimits {
            max_bytes: MAX_RESTRICTED_JSON_STRING_BYTES,
            max_depth: MAX_RESTRICTED_JSON_DEPTH,
            max_nodes: MAX_RESTRICTED_JSON_NODES,
            max_string_bytes: MAX_RESTRICTED_JSON_STRING_BYTES,
            max_key_bytes: MAX_JSON_POINTER_BYTES,
        },
    )?;
    validate_restricted_json_tree(&parsed)?;
    Ok(parsed)
}

fn validate_restricted_json_tree(value: &Value) -> Result<()> {
    let mut stack = vec![(value, 0usize)];
    let mut nodes = 0usize;
    while let Some((current, depth)) = stack.pop() {
        nodes += 1;
        if nodes > MAX_RESTRICTED_JSON_NODES {
            bail!("restricted JSON exceeds node limit");
        }
        if depth > MAX_RESTRICTED_JSON_DEPTH {
            bail!("restricted JSON exceeds depth limit");
        }
        match current {
            Value::String(text) => {
                if text.len() > MAX_RESTRICTED_JSON_STRING_BYTES {
                    bail!("restricted JSON string exceeds limit");
                }
            }
            Value::Array(values) => {
                if values.len() > MAX_RESTRICTED_JSON_NODES {
                    bail!("restricted JSON array exceeds limit");
                }
                stack.extend(values.iter().map(|child| (child, depth + 1)));
            }
            Value::Object(values) => {
                if values.len() > MAX_RESTRICTED_JSON_NODES {
                    bail!("restricted JSON object exceeds limit");
                }
                for (key, child) in values {
                    if key.len() > MAX_JSON_POINTER_BYTES
                        || key.bytes().any(|byte| byte.is_ascii_control())
                    {
                        bail!("restricted JSON object key exceeds limit");
                    }
                    stack.push((child, depth + 1));
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
    Ok(())
}

fn flatten_array_once(value: Value) -> Result<Value> {
    let values = value
        .as_array()
        .context("array_flatten requires an array")?;
    let mut flattened = Vec::new();
    for value in values {
        if let Some(nested) = value.as_array() {
            if flattened.len().saturating_add(nested.len()) > MAX_SUBJECTS {
                bail!("array_flatten output exceeds limit");
            }
            flattened.extend(nested.iter().cloned());
        } else {
            if flattened.len() >= MAX_SUBJECTS {
                bail!("array_flatten output exceeds limit");
            }
            flattened.push(value.clone());
        }
    }
    Ok(Value::Array(flattened))
}

fn opaque_id(value: &Value) -> Result<String> {
    let text = match value {
        Value::String(value) => value.trim().to_string(),
        Value::Number(value) if value.is_i64() || value.is_u64() => value.to_string(),
        _ => bail!("opaque id must be a string or integer"),
    };
    if text.is_empty() || text.len() > MAX_OPAQUE_ID_BYTES || text.chars().any(char::is_control) {
        bail!("opaque id is outside bounded limits");
    }
    Ok(text)
}

fn extract_scalar_string(value: &Value, maximum: usize) -> Result<String> {
    let value = opaque_id(value)?;
    if value.len() > maximum {
        bail!("extracted value exceeds length limit");
    }
    Ok(value)
}

fn default_true() -> bool {
    true
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn event_automation_idempotency_key(
    space_id: Uuid,
    automation_key: &str,
    provider_event_ref: &str,
) -> String {
    let material = format!("{space_id}\0{automation_key}\0{provider_event_ref}");
    format!("automation-event:{}", sha256_hex(material.as_bytes()))
}

fn shadow_observation_idempotency_key(
    provider: &str,
    space_id: Uuid,
    mapping_id: Uuid,
    event_id: &str,
    scope: &str,
) -> String {
    let material = format!("{provider}\0{space_id}\0{mapping_id}\0{event_id}\0{scope}");
    format!("space-event-shadow:{}", sha256_hex(material.as_bytes()))
}

fn sha256_marker(value: &str) -> String {
    format!("sha256:{}", sha256_hex(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use base64ct::Encoding;
    use serde_json::json;

    use super::*;

    #[derive(Deserialize)]
    struct MappingFixture {
        selector: Predicate,
        extractor: ExtractorSpec,
    }

    fn group_add_extractor() -> ExtractorSpec {
        serde_json::from_value(json!({
            "event_type": "qiwe.group_member_added",
            "event_id": {
                "pointer": "/msgUniqueIdentifier",
                "transforms": [{"op": "opaque_id"}]
            },
            "space_chat_id": {
                "pointer": "/fromRoomId",
                "transforms": [{"op": "opaque_id"}]
            },
            "subject_user_ids": {
                "pointer": "/msgData/changedMemberList",
                "transforms": [
                    {"op": "base64_utf8"},
                    {"op": "split", "delimiter": ";", "max_parts": 64},
                    {"op": "opaque_id"},
                    {"op": "dedupe"}
                ]
            },
            "occurred_at": {
                "pointer": "/timestamp",
                "transforms": [{"op": "unix_timestamp"}]
            }
        }))
        .unwrap()
    }

    fn v2_selector() -> Predicate {
        serde_json::from_value(json!({
            "op": "all",
            "rules": [
                {"op": "equals", "pointer": "/newMsgType", "value": "GROUP_MEMBER_ADD"},
                {"op": "in", "pointer": "/cmd", "values": [15000, 15500]}
            ]
        }))
        .unwrap()
    }

    fn unified_group_add_selector_value() -> Value {
        json!({
            "op": "any",
            "rules": [
                {
                    "op": "all",
                    "rules": [
                        {
                            "op": "equals",
                            "pointer": "/newMsgType",
                            "value": "GROUP_MEMBER_ADD"
                        },
                        {
                            "op": "in",
                            "pointer": "/cmd",
                            "values": [15000, 15500]
                        }
                    ]
                },
                {
                    "op": "all",
                    "rules": [
                        {"op": "equals", "pointer": "/msgType", "value": 1002},
                        {"op": "exists", "pointer": "/newMsgType", "value": false},
                        {
                            "op": "in",
                            "pointer": "/cmd",
                            "values": [15000, 15500]
                        }
                    ]
                }
            ]
        })
    }

    #[test]
    fn v2_group_add_extracts_all_members_as_opaque_strings() {
        let encoded = Base64::encode_string(b"1234567890123456;member-b;member-b");
        let item = json!({
            "cmd": 15500,
            "newMsgType": "GROUP_MEMBER_ADD",
            "msgUniqueIdentifier": 1234567890123456_u64,
            "fromRoomId": "room-1",
            "senderId": "operator-not-new-member",
            "timestamp": 1786669200,
            "msgData": {"changedMemberList": encoded}
        });
        assert!(evaluate_predicate(&v2_selector(), &item, 0).unwrap());
        let event = extract_event(&group_add_extractor(), &item).unwrap();
        assert_eq!(event.event_id, "1234567890123456");
        assert_eq!(event.subject_user_ids, vec!["1234567890123456", "member-b"]);
        assert!(!event
            .subject_user_ids
            .contains(&"operator-not-new-member".to_string()));
    }

    #[test]
    fn bad_base64_fails_without_sender_fallback() {
        let item = json!({
            "msgUniqueIdentifier": "evt-bad",
            "fromRoomId": "room-1",
            "senderId": "operator-not-new-member",
            "timestamp": 1786669200,
            "msgData": {"changedMemberList": "%%%not-base64%%%"}
        });
        let error = extract_event(&group_add_extractor(), &item).unwrap_err();
        assert!(error.to_string().contains("base64"));
    }

    #[test]
    fn data_expansion_is_bounded_and_handles_string_arrays() {
        let payload = json!({
            "data": "[{\"fromRoomId\":\"room-1\"},{\"fromRoomId\":\"room-1\"}]"
        });
        assert_eq!(expand_data_items(&payload).unwrap().len(), 2);
        let duplicate = json!({
            "data": "[{\"fromRoomId\":\"room-1\",\"fromRoomId\":\"room-2\"}]"
        });
        let error = expand_data_items(&duplicate).expect_err("duplicate keys must fail closed");
        assert!(format!("{error:#}").contains("duplicate"));
        let oversized = json!({
            "data": (0..65).map(|index| json!({"index": index})).collect::<Vec<_>>()
        });
        assert!(expand_data_items(&oversized).is_err());
    }

    #[test]
    fn member_remove_does_not_match_group_add_selector() {
        let item = json!({
            "cmd": 15500,
            "newMsgType": "GROUP_MEMBER_REMOVE"
        });
        assert!(!evaluate_predicate(&v2_selector(), &item, 0).unwrap());
    }

    #[test]
    fn unknown_extractor_fields_are_rejected() {
        let value = json!({
            "event_type": "qiwe.group_member_added",
            "event_id": {"pointer": "/event"},
            "space_chat_id": {"pointer": "/room"},
            "subject_user_ids": {"pointer": "/users"},
            "occurred_at": {"pointer": "/time"},
            "target_group_id": "forbidden"
        });
        assert!(serde_json::from_value::<ExtractorSpec>(value).is_err());
    }

    #[test]
    fn json_pointer_is_literal_not_dynamic_code() {
        let predicate: Predicate = serde_json::from_value(json!({
            "op": "equals",
            "pointer": "/msgData/a~1b",
            "value": "matched"
        }))
        .unwrap();
        let item = json!({"msgData": {"a/b": "matched"}});
        assert!(evaluate_predicate(&predicate, &item, 0).unwrap());
    }

    #[test]
    fn checked_in_v1_and_v2_mapping_fixtures_are_valid() {
        for source in [
            include_str!("../../../fixtures/qiwe/event-mappings/group-member-add-v1.json"),
            include_str!("../../../fixtures/qiwe/event-mappings/group-member-add-v2.json"),
        ] {
            let fixture: MappingFixture = serde_json::from_str(source).unwrap();
            validate_predicate(&fixture.selector, 0, &mut 0).unwrap();
            validate_extractor(&fixture.extractor).unwrap();
        }
    }

    #[test]
    fn public_definition_api_replays_checked_in_v2_fixture() {
        let mapping: Value = serde_json::from_str(include_str!(
            "../../../fixtures/qiwe/event-mappings/group-member-add-v2.json"
        ))
        .unwrap();
        let payload: Value = serde_json::from_str(include_str!(
            "../../../fixtures/qiwe/system/group-member-add-v2.json"
        ))
        .unwrap();

        validate_definition(&mapping["selector"], &mapping["extractor"]).unwrap();
        let events =
            replay_definition(&mapping["selector"], &mapping["extractor"], &payload).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "qiwe.group_member_added");
        assert_eq!(events[0].space_chat_id, "fixture-room-a");
    }

    #[test]
    fn selector_and_transform_unknown_fields_are_rejected() {
        let selector = json!({
            "op": "equals",
            "pointer": "/msgType",
            "value": 1002,
            "target_group_id": "forbidden"
        });
        let extractor = serde_json::from_str::<Value>(include_str!(
            "../../../fixtures/qiwe/event-mappings/group-member-add-v1.json"
        ))
        .unwrap()["extractor"]
            .clone();
        assert!(validate_definition(&selector, &extractor).is_err());

        let mapping: Value = serde_json::from_str(include_str!(
            "../../../fixtures/qiwe/event-mappings/group-member-add-v1.json"
        ))
        .unwrap();
        let mut bad_extractor = mapping["extractor"].clone();
        bad_extractor["event_id"]["transforms"] = json!([{
            "op": "opaque_id",
            "url": "https://example.test"
        }]);
        assert!(validate_definition(&mapping["selector"], &bad_extractor).is_err());
    }

    #[test]
    fn malformed_json_pointer_escape_is_rejected() {
        assert!(validate_pointer("/msgData/bad~2escape").is_err());
        assert!(validate_pointer("/msgData/dangling~").is_err());
    }

    #[test]
    fn v1_mapping_never_claims_an_event_that_declares_a_v2_type() {
        let fixture: MappingFixture = serde_json::from_str(include_str!(
            "../../../fixtures/qiwe/event-mappings/group-member-add-v1.json"
        ))
        .unwrap();
        let item = json!({
            "cmd": 15000,
            "msgType": 1002,
            "newMsgType": "GROUP_MEMBER_ADD"
        });

        assert!(!evaluate_predicate(&fixture.selector, &item, 0).unwrap());
    }

    #[test]
    fn registered_replay_proves_unified_v1_v2_group_add_mapping() {
        let extractor = serde_json::to_value(group_add_extractor()).unwrap();
        let evidence =
            replay_registered_fixtures("qiwe", &unified_group_add_selector_value(), &extractor)
                .unwrap();

        assert_eq!(evidence["fixture_replay_passed"], true);
        assert_eq!(evidence["registered_mapping_count"], 1);
        assert_eq!(evidence["fixture_count"], 1);
        assert_eq!(evidence["positive_fixture_count"], 2);
        assert_eq!(evidence["negative_fixture_count"], 2);
        assert_eq!(evidence["real_event_verified"], false);
    }

    #[test]
    fn registered_mapping_identity_is_bound_to_embedded_source_bytes() {
        let source = include_str!(
            "../../../fixtures/qiwe/event-mappings/group-member-add/v1-v2.mapping.json"
        );
        let expected = format!("{:x}", Sha256::digest(source.as_bytes()));

        assert_eq!(
            registered_mapping_source_sha256("qiwe", "group_member_add").unwrap(),
            Some(expected)
        );
        assert_eq!(
            registered_mapping_source_sha256("qiwe", "not_registered").unwrap(),
            None
        );
        assert!(registered_mapping_source_sha256("qiwe", "../escape").is_err());
    }

    #[test]
    fn registered_replay_rejects_a_mapping_that_covers_only_v2() {
        let selector = serde_json::to_value(v2_selector()).unwrap();
        let extractor = serde_json::to_value(group_add_extractor()).unwrap();
        let error = replay_registered_fixtures("qiwe", &selector, &extractor)
            .expect_err("v1 coverage is required");

        assert!(error
            .to_string()
            .contains("no registered trusted fixture suite"));
    }

    #[test]
    fn complete_json_bundle_registers_a_new_event_type_without_rust_dispatch_code() {
        const MAPPING_PATH: &str = "fixtures/qiwe/event-mappings/registry-probe/v1.mapping.json";
        const FIXTURE_PATH: &str = "fixtures/qiwe/system/registry-probe/v1.fixture.json";
        const MAPPING: &str = r#"{
            "schema_version": 1,
            "provider": "qiwe",
            "definition_key": "registry_probe_v1",
            "selector": {"op": "equals", "pointer": "/kind", "value": "PROBE"},
            "extractor": {
                "event_type": "qiwe.registry_probe",
                "event_id": {"pointer": "/eventId", "transforms": [{"op": "opaque_id"}]},
                "space_chat_id": {"pointer": "/fromRoomId", "transforms": [{"op": "opaque_id"}]},
                "subject_user_ids": {"pointer": "/memberIds", "transforms": [{"op": "opaque_id"}]},
                "occurred_at": {"pointer": "/timestamp", "transforms": [{"op": "unix_timestamp"}]}
            },
            "official_sources": ["https://doc.qiweapi.com/doc-9079960"]
        }"#;
        const POSITIVE_ONLY_FIXTURE: &str = r#"{
            "fixture_metadata": {
                "sanitized": true,
                "synthetic": true,
                "mapping_ref": "fixtures/qiwe/event-mappings/registry-probe/v1.mapping.json"
            },
            "event": {"data": [{
                "kind": "PROBE",
                "eventId": "probe-event",
                "fromRoomId": "probe-room",
                "memberIds": ["probe-member"],
                "timestamp": 1786669200
            }]}
        }"#;
        const FIXTURE: &str = r#"{
            "fixture_metadata": {
                "sanitized": true,
                "synthetic": true,
                "mapping_ref": "fixtures/qiwe/event-mappings/registry-probe/v1.mapping.json"
            },
            "event": {"data": [{
                "kind": "PROBE",
                "eventId": "probe-event",
                "fromRoomId": "probe-room",
                "memberIds": ["probe-member"],
                "timestamp": 1786669200
            }, {
                "kind": "NOT_A_PROBE"
            }]}
        }"#;
        const EXPECTATION: &str = r#"{
            "expectation_metadata": {
                "sanitized": true,
                "synthetic": true,
                "mapping_ref": "fixtures/qiwe/event-mappings/registry-probe/v1.mapping.json",
                "fixture_ref": "fixtures/qiwe/system/registry-probe/v1.fixture.json"
            },
            "events": [{
                "event_type": "qiwe.registry_probe",
                "event_id": "probe-event",
                "space_id": "probe-room",
                "subject_user_ids": ["probe-member"],
                "occurred_at": "2026-08-14T01:00:00Z"
            }]
        }"#;
        let mapping: Value = serde_json::from_str(MAPPING).unwrap();
        let (_, mut cross_space_extractor) =
            parse_definition(mapping["selector"].clone(), mapping["extractor"].clone()).unwrap();
        cross_space_extractor.space_chat_id.pointer = "/targetRoomId".to_string();
        assert!(
            validate_registered_space_extractor("qiwe", &cross_space_extractor, MAPPING_PATH,)
                .expect_err("registered Space routing must use the provider room field")
                .to_string()
                .contains("/fromRoomId")
        );
        let positive_only_error = replay_registered_fixture_sources(
            "qiwe",
            &mapping["selector"],
            &mapping["extractor"],
            &[EmbeddedJsonSource {
                path: MAPPING_PATH,
                contents: MAPPING,
            }],
            &[EmbeddedJsonSource {
                path: FIXTURE_PATH,
                contents: POSITIVE_ONLY_FIXTURE,
            }],
            &[EmbeddedJsonSource {
                path: "fixtures/qiwe/event-mappings/registry-probe/v1.expected.json",
                contents: EXPECTATION,
            }],
        )
        .expect_err("a positive-only fixture suite must fail closed");
        assert!(positive_only_error
            .to_string()
            .contains("at least one negative input record"));

        let evidence = replay_registered_fixture_sources(
            "qiwe",
            &mapping["selector"],
            &mapping["extractor"],
            &[EmbeddedJsonSource {
                path: MAPPING_PATH,
                contents: MAPPING,
            }],
            &[EmbeddedJsonSource {
                path: FIXTURE_PATH,
                contents: FIXTURE,
            }],
            &[EmbeddedJsonSource {
                path: "fixtures/qiwe/event-mappings/registry-probe/v1.expected.json",
                contents: EXPECTATION,
            }],
        )
        .unwrap();

        assert_eq!(evidence["event_type"], "qiwe.registry_probe");
        assert_eq!(evidence["fixture_replay_passed"], true);

        const WRONG_EXPECTATION: &str = r#"{
            "expectation_metadata": {
                "sanitized": true,
                "synthetic": true,
                "mapping_ref": "fixtures/qiwe/event-mappings/registry-probe/v1.mapping.json",
                "fixture_ref": "fixtures/qiwe/system/registry-probe/v1.fixture.json"
            },
            "events": [{
                "event_type": "qiwe.registry_probe",
                "event_id": "wrong-event",
                "space_id": "probe-room",
                "subject_user_ids": ["probe-member"],
                "occurred_at": "2026-08-14T01:00:00Z"
            }]
        }"#;
        let error = replay_registered_fixture_sources(
            "qiwe",
            &mapping["selector"],
            &mapping["extractor"],
            &[EmbeddedJsonSource {
                path: MAPPING_PATH,
                contents: MAPPING,
            }],
            &[EmbeddedJsonSource {
                path: FIXTURE_PATH,
                contents: FIXTURE,
            }],
            &[EmbeddedJsonSource {
                path: "fixtures/qiwe/event-mappings/registry-probe/v1.expected.json",
                contents: WRONG_EXPECTATION,
            }],
        )
        .expect_err("an expected-output mismatch must fail closed");
        assert!(error.to_string().contains("output mismatch"));
    }

    #[test]
    fn incomplete_registered_bundle_fails_closed() {
        const MAPPING: &str = r#"{
            "schema_version": 1,
            "provider": "qiwe",
            "definition_key": "missing_fixture",
            "selector": {"op": "equals", "pointer": "/kind", "value": "PROBE"},
            "extractor": {
                "event_type": "qiwe.registry_probe",
                "event_id": {"pointer": "/eventId", "transforms": [{"op": "opaque_id"}]},
                "space_chat_id": {"pointer": "/fromRoomId", "transforms": [{"op": "opaque_id"}]},
                "subject_user_ids": {"pointer": "/memberIds", "transforms": [{"op": "opaque_id"}]},
                "occurred_at": {"pointer": "/timestamp", "transforms": [{"op": "unix_timestamp"}]}
            },
            "official_sources": ["https://doc.qiweapi.com/doc-9079960"]
        }"#;
        let mapping: Value = serde_json::from_str(MAPPING).unwrap();
        let error = replay_registered_fixture_sources(
            "qiwe",
            &mapping["selector"],
            &mapping["extractor"],
            &[EmbeddedJsonSource {
                path: "fixtures/qiwe/event-mappings/missing/v1.mapping.json",
                contents: MAPPING,
            }],
            &[],
            &[],
        )
        .expect_err("an incomplete registry must not authorize a mapping");

        assert!(error.to_string().contains("has no fixture bundle"));
    }

    #[test]
    fn checked_in_registry_replays_every_fixture_bundle() {
        let suites = load_registered_fixture_suites(
            EMBEDDED_EVENT_MAPPING_SOURCES,
            EMBEDDED_EVENT_FIXTURE_SOURCES,
            EMBEDDED_EVENT_EXPECTATION_SOURCES,
        )
        .unwrap();
        assert!(!suites.is_empty());

        for suite in suites {
            for fixture in suite.fixtures {
                let actual = replay_mapping(
                    &suite.mapping.selector,
                    &suite.mapping.extractor,
                    &fixture.event,
                )
                .unwrap();
                assert_eq!(actual, fixture.expected_events, "{}", fixture.fixture_path);
            }
        }
    }

    #[test]
    fn restricted_primitive_composes_only_fixed_kernel_operations() {
        let primitive = RestrictedPrimitive {
            operations: vec![
                RestrictedPrimitiveOperation::Base64Utf8,
                RestrictedPrimitiveOperation::JsonParse,
                RestrictedPrimitiveOperation::JsonPointer {
                    pointer: "/members".to_string(),
                },
                RestrictedPrimitiveOperation::ArrayFlatten,
            ],
        };
        let encoded =
            Base64::encode_string(br#"{"members":[["member-a"],["member-b","member-a"]]}"#);
        let value = apply_restricted_primitive(Value::String(encoded), &primitive).unwrap();
        let value = apply_transform(value, &Transform::OpaqueId).unwrap();
        let value = apply_transform(value, &Transform::Dedupe).unwrap();

        assert_eq!(value, json!(["member-a", "member-b"]));
    }

    #[test]
    fn restricted_json_parse_rejects_ambiguous_or_unbounded_documents() {
        let duplicate = Value::String(r#"{"members":["first"],"members":["second"]}"#.into());
        let duplicate_error = parse_strict_bounded_json(duplicate).unwrap_err();
        assert!(format!("{duplicate_error:#}").contains("duplicate"));

        let deep = Value::String(format!("{}null{}", "[".repeat(18), "]".repeat(18)));
        assert!(parse_strict_bounded_json(deep).is_err());

        let many_nodes = Value::String(format!("[{}]", vec!["null"; 1_025].join(",")));
        assert!(parse_strict_bounded_json(many_nodes).is_err());

        let oversized = Value::String(format!("\"{}\"", "a".repeat(16 * 1_024 + 1)));
        assert!(parse_strict_bounded_json(oversized).is_err());
    }

    #[test]
    fn restricted_primitive_registry_rejects_escape_and_recursive_operations() {
        const VALID: &str = r#"{
            "schema_version": 1,
            "provider": "qiwe",
            "definition_key": "base64_json_members_v1",
            "operations": [
                {"op": "base64_utf8"},
                {"op": "json_parse"},
                {"op": "json_pointer", "pointer": "/members"}
            ],
            "official_sources": ["https://doc.qiweapi.com/doc-7331304"]
        }"#;
        let valid_path =
            "fixtures/qiwe/event-mappings/_primitives/base64-json-members/v1.primitive.json";
        let registry = load_registered_restricted_primitives(&[EmbeddedJsonSource {
            path: valid_path,
            contents: VALID,
        }])
        .unwrap();
        assert!(registry.contains_key(valid_path));

        let escaped = load_registered_restricted_primitives(&[EmbeddedJsonSource {
            path: "fixtures/qiwe/event-mappings/_primitives/../escape.primitive.json",
            contents: VALID,
        }])
        .unwrap_err();
        assert!(escaped.to_string().contains("path is invalid"));

        const RECURSIVE: &str = r#"{
            "schema_version": 1,
            "provider": "qiwe",
            "definition_key": "recursive_v1",
            "operations": [{
                "op": "restricted_primitive",
                "primitive_ref": "fixtures/qiwe/event-mappings/_primitives/recursive/v1.primitive.json"
            }],
            "official_sources": ["https://doc.qiweapi.com/doc-7331304"]
        }"#;
        assert!(load_registered_restricted_primitives(&[EmbeddedJsonSource {
            path: "fixtures/qiwe/event-mappings/_primitives/recursive/v1.primitive.json",
            contents: RECURSIVE,
        }])
        .is_err());
    }

    #[test]
    fn mapping_rejects_unregistered_restricted_primitive_reference() {
        let selector = unified_group_add_selector_value();
        let mut extractor = serde_json::to_value(group_add_extractor()).unwrap();
        extractor["subject_user_ids"]["transforms"] = json!([
            {
                "op": "restricted_primitive",
                "primitive_ref": "fixtures/qiwe/event-mappings/_primitives/missing/v1.primitive.json"
            },
            {"op": "opaque_id"},
            {"op": "dedupe"}
        ]);

        assert!(validate_definition(&selector, &extractor).is_err());
    }

    #[test]
    fn event_idempotency_survives_definition_version_changes() {
        let space_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let first =
            event_automation_idempotency_key(space_id, "welcome_new_members", "qiwe:event-123");
        let after_version_change =
            event_automation_idempotency_key(space_id, "welcome_new_members", "qiwe:event-123");
        let other_space = event_automation_idempotency_key(
            Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            "welcome_new_members",
            "qiwe:event-123",
        );

        assert_eq!(first, after_version_change);
        assert_ne!(first, other_space);
        assert_eq!(first.len(), "automation-event:".len() + 64);
    }

    #[test]
    fn shadow_observation_idempotency_is_provider_and_space_scoped() {
        let mapping_id = Uuid::new_v4();
        let first_space = Uuid::new_v4();
        let second_space = Uuid::new_v4();
        let first = shadow_observation_idempotency_key(
            "qiwe",
            first_space,
            mapping_id,
            "provider-event-1",
            "mapping_shadow",
        );

        assert_ne!(
            first,
            shadow_observation_idempotency_key(
                "qiwe",
                second_space,
                mapping_id,
                "provider-event-1",
                "mapping_shadow",
            )
        );
        assert_ne!(
            first,
            shadow_observation_idempotency_key(
                "another-provider",
                first_space,
                mapping_id,
                "provider-event-1",
                "mapping_shadow",
            )
        );
    }

    #[tokio::test]
    async fn unauthenticated_raw_event_never_queries_event_mappings() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:9/unused")
            .unwrap();
        let raw_event = RawQiweEvent {
            event_id: "unauthenticated-event".to_string(),
            received_at: Utc::now(),
            source: "qiwe".to_string(),
            ingress_auth_verified: false,
            payload: json!({"data": {"fromRoomId": "room-1"}}),
        };

        process_persisted_raw_event(&pool, Uuid::new_v4(), &raw_event)
            .await
            .expect("unauthenticated event stops before database access");
    }
}
