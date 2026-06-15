//! Excel correspondence rule pack for Binoc.
//!
//! Parses spreadsheet workbooks (`.xlsx`/`.xls`/`.xlsm`/`.xlsb`/`.ods`) into
//! Binoc's format-neutral tabular model. A workbook is a namespace of named
//! sheets: the workbook node is a plain container (no artifact) and every
//! non-empty sheet becomes a child node (`book.xlsx/>Sheet1`) carrying a
//! `tabular_v1` artifact — including single-sheet workbooks, since sheets have
//! intrinsic names.

mod excel;

use std::sync::Arc;

use binoc_sdk::{CoreRule, CorrespondenceEngineConfig};

pub use excel::ExcelParse;

pub fn register_correspondence_rules(config: &mut CorrespondenceEngineConfig) {
    config
        .rules
        .insert(0, CoreRule::Parse(Arc::new(ExcelParse)));
}

#[cfg(feature = "test-support")]
pub mod test_support;
