//! binformats correspondence rule pack for Binoc.
//!
//! Parses a family of serialization / value-tree formats (CBOR, MessagePack,
//! BSON, plist, Ion) into the generic `structured_document_v1` artifact, so the
//! stdlib structured-document writer handles diffing, summaries, and tags
//! without knowing the source format.

mod binformats;

use std::sync::Arc;

use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub use binformats::{
    BsonParseRule, CborParseRule, IonParseRule, MsgpackParseRule, PlistParseRule,
};

/// Register this pack's parse rules into an engine config.
pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    let rules: Vec<CoreRule> = vec![
        CoreRule::Parse(Arc::new(CborParseRule)),
        CoreRule::Parse(Arc::new(MsgpackParseRule)),
        CoreRule::Parse(Arc::new(BsonParseRule)),
        CoreRule::Parse(Arc::new(PlistParseRule)),
        CoreRule::Parse(Arc::new(IonParseRule)),
    ];
    for rule in rules.into_iter().rev() {
        config.rules.insert(0, rule);
    }
}

#[cfg(feature = "test-support")]
pub mod test_support;
