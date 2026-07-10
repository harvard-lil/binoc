#![cfg(all(
    feature = "bundled",
    feature = "sqlite",
    feature = "excel",
    feature = "parquet",
    feature = "avro",
    feature = "dbf",
    feature = "xml",
    feature = "shapefile",
    feature = "binformats",
    feature = "stat-binary",
    feature = "row-reorder"
))]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use binoc_sdk::CoreRule;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Registry {
    plugins: Vec<Plugin>,
}

#[derive(Debug, Deserialize)]
struct Plugin {
    id: String,
    packages: Packages,
    rule_packs: Vec<RulePack>,
}

#[derive(Debug, Deserialize)]
struct Packages {
    #[serde(rename = "crate")]
    rust_crate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RulePack {
    name: String,
    dispatch: Dispatch,
    rules: Vec<String>,
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
struct Dispatch {
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    media_types: Vec<String>,
    #[serde(default)]
    member_extensions: Vec<String>,
    #[serde(default)]
    artifact_formats: Vec<String>,
    scope: String,
}

#[derive(Debug, PartialEq, Eq)]
struct RegisteredDescriptor {
    family: String,
    dispatch: Dispatch,
}

fn registry_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("plugin_registry.json")
}

#[test]
fn rust_plugin_registry_entries_match_registered_descriptors() {
    let registry: Registry =
        serde_json::from_slice(&std::fs::read(registry_path()).expect("read plugin_registry.json"))
            .expect("parse plugin_registry.json");

    let mut config = binoc_stdlib::correspondence::default_engine_config();
    assert_stdlib_entry(&registry, &config);

    let expected = registry
        .plugins
        .into_iter()
        .filter(|plugin| plugin.id != "binoc-stdlib" && plugin.packages.rust_crate.is_some())
        .flat_map(|plugin| plugin.rule_packs)
        .map(|rule_pack| {
            assert_eq!(
                rule_pack.rules.len(),
                1,
                "{} must describe one registered descriptor family",
                rule_pack.name
            );
            (
                rule_pack.name,
                RegisteredDescriptor {
                    family: rule_pack.rules[0].clone(),
                    dispatch: rule_pack.dispatch,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    let stdlib_rules = config
        .rules
        .iter()
        .map(CoreRule::name)
        .collect::<BTreeSet<_>>();
    let stdlib_writers = config
        .writers
        .iter()
        .map(|writer| writer.descriptor().name)
        .collect::<BTreeSet<_>>();

    binoc_cli::register_bundled_correspondence_rules(&mut config);

    let mut actual = BTreeMap::new();
    for rule in &config.rules {
        if stdlib_rules.contains(&rule.name()) {
            continue;
        }
        let CoreRule::Parse(rule) = rule else {
            panic!("bundled rule {} is not a parse rule", rule.name());
        };
        let descriptor = rule.descriptor();
        let members = rule.extra_members();
        let grouped = !members.is_empty();
        let registered = RegisteredDescriptor {
            family: if grouped { "group-parse" } else { "parse" }.into(),
            dispatch: Dispatch {
                extensions: descriptor.input.extensions,
                media_types: descriptor.input.media_types,
                member_extensions: members
                    .into_iter()
                    .flat_map(|member| member.matcher.extensions)
                    .collect(),
                artifact_formats: Vec::new(),
                scope: if grouped { "file-groups" } else { "files" }.into(),
            },
        };
        assert!(
            actual.insert(descriptor.name.clone(), registered).is_none(),
            "duplicate registered descriptor {}",
            descriptor.name
        );
    }

    for writer in &config.writers {
        let descriptor = writer.descriptor();
        if stdlib_writers.contains(&descriptor.name) {
            continue;
        }
        let registered = RegisteredDescriptor {
            family: "writer".into(),
            dispatch: Dispatch {
                extensions: descriptor.input.extensions,
                media_types: descriptor.input.media_types,
                member_extensions: Vec::new(),
                artifact_formats: descriptor
                    .formats
                    .into_iter()
                    .map(|format| format.to_string())
                    .collect(),
                scope: "artifacts".into(),
            },
        };
        assert!(
            actual.insert(descriptor.name.clone(), registered).is_none(),
            "duplicate registered descriptor {}",
            descriptor.name
        );
    }

    assert_eq!(actual, expected);
}

fn assert_stdlib_entry(registry: &Registry, config: &binoc_sdk::CorrespondenceEngineConfig) {
    let plugin = registry
        .plugins
        .iter()
        .find(|plugin| plugin.id == "binoc-stdlib")
        .expect("binoc-stdlib registry entry");
    assert_eq!(plugin.rule_packs.len(), 1);
    let entry = &plugin.rule_packs[0];
    assert_eq!(entry.name, "binoc-stdlib");
    assert_eq!(
        entry.dispatch,
        Dispatch {
            scope: "all".into(),
            ..Dispatch::default()
        }
    );

    let mut actual_families = config
        .rules
        .iter()
        .map(|rule| match rule {
            CoreRule::Expand(_) => "expand",
            CoreRule::Parse(_) => "parse",
            CoreRule::Pair(_) => "pair",
        })
        .collect::<BTreeSet<_>>();
    if !config.writers.is_empty() {
        actual_families.insert("writer");
    }
    if !config.compaction.is_empty() {
        actual_families.insert("compaction");
    }
    if !config.annotators.is_empty() {
        actual_families.insert("annotator");
    }
    if !binoc_stdlib::default_registry().renderer_names().is_empty() {
        actual_families.insert("render");
    }
    assert_eq!(
        actual_families,
        entry.rules.iter().map(String::as_str).collect()
    );
}
