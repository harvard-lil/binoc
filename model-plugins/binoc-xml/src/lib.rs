//! XML correspondence rule pack for Binoc.
//!
//! Parses XML documents (`.xml`, `.rdf`, `.kml`, `.gml`, `.atom`, `.rss`) into
//! the generic `structured_document_v1` artifact, tagged `format: "xml"`, so the
//! stdlib structured-document writer handles diffing, summaries, and tags
//! without knowing the source format — and so future XML-specific edit-list
//! rewrite rules can match on the parser + artifact + tag.
//!
//! See [`xml`] for the deterministic XML → JSON convention.

mod xml;

use std::sync::Arc;

use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub use xml::{XmlMediaParseRule, XmlParseRule};

/// Register this pack's parse rules into an engine config.
pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    let rules: Vec<CoreRule> = vec![
        CoreRule::Parse(Arc::new(XmlParseRule)),
        CoreRule::Parse(Arc::new(XmlMediaParseRule)),
    ];
    for rule in rules.into_iter().rev() {
        config.rules.insert(0, rule);
    }
}
