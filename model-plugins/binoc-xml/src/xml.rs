//! Parse rule for XML documents.
//!
//! Reads the source bytes, walks them into a single [`serde_json::Value`] tree
//! with a deterministic, lossless-by-convention mapping, and emits the standard
//! `structured_document_v1` artifact tagged `format: "xml"` (mirroring the
//! binformats / JSON / YAML / TOML parsers). The generic structured-document
//! writer then handles diffing, summaries, and tags without knowing the origin
//! format.
//!
//! ## XML -> JSON convention
//!
//! We hand-roll the walk on top of `quick-xml` rather than using
//! `quickxml_to_serde`, because that crate is unmaintained (built on
//! `quick-xml` 0.17 via `minidom`), strips namespace prefixes, and guesses
//! scalar types — all of which corrupt a diff. Our mapping is:
//!
//! - The document is wrapped under its root element's (prefixed) name:
//!   `<rows>…</rows>` -> `{ "rows": … }`. A renamed root is therefore a visible
//!   change.
//! - Elements become object keys keyed by their **full prefixed name**
//!   (`gmd:MD_Metadata`, not `MD_Metadata`) so distinct namespaces never
//!   collapse together. `xmlns:*` declarations ride along as ordinary
//!   attributes.
//! - Attributes become `@`-prefixed keys (`@id`, `@xlink:href`).
//! - Text content becomes a `#text` key on an element that also has
//!   attributes/children, or the element's plain string value when it is a pure
//!   leaf (`<name>Alice</name>` -> `"Alice"`).
//! - Repeated identically-named sibling elements collect into an array, in
//!   document order. A name seen once is a single value; seen twice or more, an
//!   array.
//! - **Every scalar stays a string.** We never coerce `"01"`, `"30"`, or
//!   `"true"` to a JSON number/bool — type guessing is lossy and the source XML
//!   carries no type. Consumers that want typing can apply it downstream.
//!
//! Determinism: object key order follows first-appearance of each child name;
//! array order follows document order; attributes precede children. The same
//! bytes always produce byte-identical JSON, which is what binoc diffs.

use binoc_sdk::*;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::Reader;
use serde_json::{Map, Value};

/// Object-key prefix for XML attributes.
const ATTR_PREFIX: &str = "@";
/// Object key holding an element's text when it also has attributes/children.
const TEXT_KEY: &str = "#text";

/// Parses XML documents dispatched by **extension**
/// (`.xml`, `.rdf`, `.kml`, `.gml`, `.atom`, `.rss`).
#[derive(Default)]
pub struct XmlParseRule;

/// Parses XML documents dispatched by **media type**
/// (`text/xml`, `application/xml`, `application/rdf+xml`, `application/atom+xml`).
///
/// A separate rule from [`XmlParseRule`] because [`NodeMatch`] ANDs its
/// `extensions` and `media_types` fields — a single rule declaring both would
/// only fire when *both* match. Splitting extension and media-type dispatch into
/// sibling rules mirrors the stdlib JSON parsers (`JsonParse` / `JsonMediaParse`).
#[derive(Default)]
pub struct XmlMediaParseRule;

impl ParseRule for XmlParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-xml.parse.xml".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![
                    ".xml".into(),
                    ".rdf".into(),
                    ".kml".into(),
                    ".gml".into(),
                    ".atom".into(),
                    ".rss".into(),
                ],
                ..NodeMatch::default()
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        parse_xml_item(item, data)
    }
}

impl ParseRule for XmlMediaParseRule {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc-xml.parse.xml_media".into(),
            input: NodeMatch {
                is_dir: Some(false),
                media_types: vec![
                    "text/xml".into(),
                    "application/xml".into(),
                    "application/rdf+xml".into(),
                    "application/atom+xml".into(),
                ],
                ..NodeMatch::default()
            },
            output: structured_document_v1(),
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        parse_xml_item(item, data)
    }
}

/// Read `item`, transcode it to a value tree, and emit the
/// `structured_document_v1` artifact tagged `format: "xml"`. Shared by the
/// extension- and media-type-dispatched rules.
fn parse_xml_item(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
    let bytes = data.read_bytes(item)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|err| BinocError::Other(format!("XML is not valid UTF-8: {err}")))?;
    let value = xml_to_json(text)?;

    let source = serde_json::json!({ "byte_len": bytes.len() });
    serde_json::to_vec(&StructuredDocument {
        value,
        format: "xml".into(),
        source,
    })
    .map(ParseOutput::from)
    .map_err(|err| BinocError::Other(format!("serialize structured document artifact: {err}")))
}

/// An in-progress element while walking. Children are kept grouped by name in
/// first-seen order so the final object preserves a stable key order, and each
/// group preserves document order for the repeated-element -> array case.
#[derive(Default)]
struct Node {
    /// Attributes in document order, keys already `@`-prefixed.
    attrs: Vec<(String, String)>,
    /// Child elements: `(prefixed_name, [child, …])` in first-seen key order.
    children: Vec<(String, Vec<Node>)>,
    /// Accumulated text / CDATA content.
    text: String,
}

/// The full prefixed element name (e.g. `gmd:MD_Metadata`) as a string.
fn element_name(start: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(start.name().as_ref()).into_owned()
}

/// Collect an element's attributes into `node`, keyed `@name`, value unescaped.
fn collect_attrs(start: &BytesStart<'_>, node: &mut Node) -> BinocResult<()> {
    for attr in start.attributes() {
        let attr = attr.map_err(|err| BinocError::Other(format!("parse XML attribute: {err}")))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        let raw = std::str::from_utf8(attr.value.as_ref())
            .map_err(|err| BinocError::Other(format!("XML attribute is not valid UTF-8: {err}")))?;
        let value = quick_xml::escape::unescape(raw)
            .map_err(|err| BinocError::Other(format!("unescape XML attribute: {err}")))?
            .into_owned();
        node.attrs.push((format!("{ATTR_PREFIX}{key}"), value));
    }
    Ok(())
}

/// Resolve a `&entity;` / `&#NN;` reference event to its text.
///
/// Handles numeric character references and the five predefined XML entities.
/// Any other named entity (a DTD-defined general entity) is unresolvable
/// deterministically here, so we surface it as an error rather than guess.
fn resolve_reference(reference: &BytesRef<'_>) -> BinocResult<String> {
    if let Some(ch) = reference
        .resolve_char_ref()
        .map_err(|err| BinocError::Other(format!("resolve XML character reference: {err}")))?
    {
        return Ok(ch.to_string());
    }
    let name = reference
        .decode()
        .map_err(|err| BinocError::Other(format!("decode XML entity reference: {err}")))?;
    match name.as_ref() {
        "amp" => Ok("&".into()),
        "lt" => Ok("<".into()),
        "gt" => Ok(">".into()),
        "apos" => Ok("'".into()),
        "quot" => Ok("\"".into()),
        other => Err(BinocError::Other(format!(
            "unresolvable XML entity reference: &{other};"
        ))),
    }
}

/// Append `child` under `name`, grouping repeated names in document order.
fn push_child(parent: &mut Node, name: String, child: Node) {
    if let Some(entry) = parent.children.iter_mut().find(|(k, _)| *k == name) {
        entry.1.push(child);
    } else {
        parent.children.push((name, vec![child]));
    }
}

/// Walk XML `text` into a [`serde_json::Value`] per the module convention.
fn xml_to_json(text: &str) -> BinocResult<Value> {
    let mut reader = Reader::from_str(text);
    let config = reader.config_mut();
    // Treat `<a/>` like `<a></a>` so empty elements get a `Start`/`End` pair.
    // We deliberately do NOT trim here: trimming each text chunk would corrupt
    // text that straddles an entity boundary (`a &amp; b` arrives as three
    // events). Insignificant inter-element whitespace is instead dropped when a
    // node has child elements (see `node_to_json`).
    config.expand_empty_elements = true;

    // Open-element stack; the document root is captured on its End event.
    let mut stack: Vec<(String, Node)> = Vec::new();
    let mut root: Option<(String, Node)> = None;

    loop {
        let event = reader
            .read_event()
            .map_err(|err| BinocError::Other(format!("parse XML: {err}")))?;
        match event {
            Event::Start(start) => {
                let name = element_name(&start);
                let mut node = Node::default();
                collect_attrs(&start, &mut node)?;
                stack.push((name, node));
            }
            Event::End(_) => {
                let (name, node) = stack
                    .pop()
                    .ok_or_else(|| BinocError::Other("unbalanced XML end tag".into()))?;
                match stack.last_mut() {
                    Some((_, parent)) => push_child(parent, name, node),
                    None => root = Some((name, node)),
                }
            }
            Event::Text(text) => {
                let chunk = text
                    .xml_content()
                    .map_err(|err| BinocError::Other(format!("decode XML text: {err}")))?;
                if let Some((_, node)) = stack.last_mut() {
                    node.text.push_str(&chunk);
                }
            }
            Event::GeneralRef(reference) => {
                let chunk = resolve_reference(&reference)?;
                if let Some((_, node)) = stack.last_mut() {
                    node.text.push_str(&chunk);
                }
            }
            Event::CData(cdata) => {
                let chunk = cdata
                    .decode()
                    .map_err(|err| BinocError::Other(format!("decode XML CDATA: {err}")))?;
                if let Some((_, node)) = stack.last_mut() {
                    node.text.push_str(&chunk);
                }
            }
            Event::Eof => break,
            // Declarations, comments, processing instructions, and DOCTYPE carry
            // no document data we diff; skip them.
            _ => {}
        }
    }

    let (root_name, root_node) =
        root.ok_or_else(|| BinocError::Other("XML document has no root element".into()))?;
    let mut wrapper = Map::new();
    wrapper.insert(root_name, node_to_json(root_node));
    Ok(Value::Object(wrapper))
}

/// Convert a finished [`Node`] into JSON: a pure leaf becomes its (trimmed) text
/// string; anything with attributes/children becomes an object.
///
/// Text is trimmed of surrounding whitespace. For a node that has children, an
/// all-whitespace `#text` (insignificant inter-element formatting) is dropped
/// entirely; only meaningful mixed-content text survives.
fn node_to_json(node: Node) -> Value {
    let text = node.text.trim();

    if node.attrs.is_empty() && node.children.is_empty() {
        return Value::String(text.to_string());
    }

    let mut map = Map::new();
    for (key, value) in node.attrs {
        map.insert(key, Value::String(value));
    }
    for (name, mut group) in node.children {
        let value = if group.len() == 1 {
            node_to_json(group.pop().expect("len == 1"))
        } else {
            Value::Array(group.into_iter().map(node_to_json).collect())
        };
        map.insert(name, value);
    }
    if !text.is_empty() {
        map.insert(TEXT_KEY.to_string(), Value::String(text.to_string()));
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn leaf_element_becomes_string() {
        let value = xml_to_json("<name>Alice</name>").unwrap();
        assert_eq!(value, json!({ "name": "Alice" }));
    }

    #[test]
    fn attributes_are_at_prefixed_and_text_is_separate() {
        let value = xml_to_json(r#"<a href="x">label</a>"#).unwrap();
        assert_eq!(value, json!({ "a": { "@href": "x", "#text": "label" } }));
    }

    #[test]
    fn repeated_siblings_collect_into_array_in_document_order() {
        let value = xml_to_json("<rows><row>a</row><row>b</row><row>c</row></rows>").unwrap();
        assert_eq!(value, json!({ "rows": { "row": ["a", "b", "c"] } }));
    }

    #[test]
    fn single_sibling_is_not_arrayed() {
        let value = xml_to_json("<rows><row>a</row></rows>").unwrap();
        assert_eq!(value, json!({ "rows": { "row": "a" } }));
    }

    #[test]
    fn namespace_prefixes_are_preserved() {
        let xml = r#"<gmd:MD_Metadata xmlns:gmd="urn:gmd" xmlns:gco="urn:gco"><gmd:language><gco:CharacterString>eng</gco:CharacterString></gmd:language></gmd:MD_Metadata>"#;
        let value = xml_to_json(xml).unwrap();
        assert_eq!(
            value,
            json!({
                "gmd:MD_Metadata": {
                    "@xmlns:gmd": "urn:gmd",
                    "@xmlns:gco": "urn:gco",
                    "gmd:language": { "gco:CharacterString": "eng" }
                }
            })
        );
    }

    #[test]
    fn numeric_looking_text_stays_a_string() {
        // "01" must not become the number 1, or a leading-zero id is corrupted.
        let value = xml_to_json(r#"<row id="01"><n>30</n></row>"#).unwrap();
        assert_eq!(value, json!({ "row": { "@id": "01", "n": "30" } }));
    }

    #[test]
    fn entities_and_cdata_decode() {
        let value = xml_to_json("<v>a &amp; b</v>").unwrap();
        assert_eq!(value, json!({ "v": "a & b" }));
        let value = xml_to_json("<v><![CDATA[<raw>]]></v>").unwrap();
        assert_eq!(value, json!({ "v": "<raw>" }));
    }

    #[test]
    fn empty_element_is_an_empty_string_leaf() {
        let value = xml_to_json("<a><b/></a>").unwrap();
        assert_eq!(value, json!({ "a": { "b": "" } }));
    }
}
