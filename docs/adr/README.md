# Architectural Decisions

ADRs (Architecture Decision Records) capture the rationale behind binoc's design — including alternatives that were considered and rejected. They are the canonical long-form record of the project's reasoning.

Newer entries appear first. Each entry shows its date and current status. Create a new ADR with `just adr <title>`. See the [Documentation platform ADR](2026-04-17-documentation_platform_and_info_design.md) for how this index is produced and how ADRs fit into the docs site.

| Date | Title | Status |
|---|---|---|
| 2026-06-30 | [Fat-binoc Distribution and the ABI Canary](2026-06-30-fat_binoc_distribution_and_abi_canary.md) | Accepted; implemented |
| 2026-06-29 | [SDK Annotation Package Identity Stays Explicit](2026-06-29-sdk_annotation_package_id.md) | Accepted |
| 2026-06-29 | [Renderers Are Ecosystem-Ignorant; Ugly Output From an Unknown Rule Is an Upstream Bug](2026-06-29-renderer_ecosystem_ignorance.md) | Accepted |
| 2026-06-29 | [Memory and Wall-Clock Contract for Large Dataset Diffs](2026-06-29-memory_and_wall_clock_contract_for_large_dataset_diffs.md) | Proposed |
| 2026-06-29 | [Inference Is Allowed, But Always Disclosed and Overridable; the High-Churn Guardrail Is the Backstop](2026-06-29-inference_disclosure_and_high_churn_guardrail.md) | Accepted |
| 2026-06-29 | [Feedback Report Bundles for Imperfect Results](2026-06-29-feedback_report_bundles.md) | Implemented |
| 2026-06-29 | [Binary Byte-Range Edit Kind](2026-06-29-binary_byte_range_edit_kind.md) | Proposed |
| 2026-06-29 | [A Unified Per-Path Dataset Config Model](2026-06-29-per_path_dataset_config_model.md) | Accepted |
| 2026-06-22 | [The Vintage Audience: a Kept Benchmark for Metadata-Over-Data Reading](2026-06-22-vintage_audience_and_metadata_only_benchmark.md) | Accepted (benchmark landed; features deliberately deferred) |
| 2026-06-15 | [Tiered Artifact Metadata: Column, Table, and a `parser_metadata_v1` Artifact](2026-06-15-tiered_artifact_metadata.md) | Implemented (channels + producers in CFM-80; rendering + significance in CFM-82) |
| 2026-06-15 | [The Engine Overhaul, Told Whole: Single-Tree to Correspondence-First](2026-06-15-engine_overhaul_retrospective.md) | Retrospective |
| 2026-06-15 | [Partition Identities: a JIT, Format-Owned Capability for N↔M Correspondence (CFM-72)](2026-06-15-partition_identities_jit_format_capability.md) | Implemented |
| 2026-06-15 | [Multi-Input Claims: Grouping Sibling Files into One Logical Dataset](2026-06-15-multi_input_file_sets_and_shapefile_fusion.md) | Implemented (CFM-83; supersedes the earlier registry/composite-node framing of this ADR) |
| 2026-06-15 | [Composable Per-Artifact Writers: the Artifact Is the Rendering Unit](2026-06-15-composable_per_artifact_writers.md) | Implemented (CFM-81) |
| 2026-06-14 | [Typed Records: a Greenfield `tabular` Artifact and a Generic `structured_document`](2026-06-14-typed_record_tabular_and_structured_document.md) | Accepted |
| 2026-06-14 | [Parsed Children and Decompose Boundaries (CFM-69)](2026-06-14-parsed_children_and_decompose_boundaries.md) | Accepted |
| 2026-06-14 | [Default Markdown Is A Changelog, Not An IR Dump](2026-06-14-default_markdown_changelog_policy.md) | Implemented |
| 2026-06-13 | [Stable ABI Tier Assessment for CFM-27b](2026-06-13-stable_abi_tier_cfm_27b.md) | Implemented |
| 2026-06-13 | [Derive Parse-Rule Link Gating from Pair Reads](2026-06-13-derived_requires_link.md) | Implemented |
| 2026-06-13 | [CFM-44 Measured Correspondence Performance](2026-06-13-cfm_44_measured_correspondence_performance.md) | Implemented |
| 2026-06-12 | [Tiered Plugin Surface During Pre-1.0: In-Process Proposed Tier, ABI Stable Tier](2026-06-12-tiered_plugin_surface_pre_1_0.md) | Accepted |
| 2026-06-12 | [Invariant and Lint Tiers: Harness, Mechanical, Agent](2026-06-12-invariant_and_lint_tiers.md) | Implemented |
| 2026-06-12 | [Correspondence-First Engine: Two Trees, Links, and Edit-List Compaction](2026-06-12-correspondence_first_engine.md) | Accepted (prototype validated; migration gated by structural projection) |
| 2026-06-12 | [Binoc: the architecture, told as a story](2026-06-12-historical_single_tree_architecture_story.md) | Historical |
| 2026-06-11 | [Inline Pure-Reorder Judgment; Retire Tag-Handoff Layering](2026-06-11-inline_pure_reorder_judgment.md) | Implemented |
| 2026-06-11 | [Declared Write-Sets on TransformerDescriptor](2026-06-11-declared_write_sets_on_transformer_descriptor.md) | Superseded in part by [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md) — `TransformerDescriptor` was removed in the migration; the write-set discipline carried over to rule descriptors and is mechanized in [Invariant and Lint Tiers](2026-06-12-invariant_and_lint_tiers.md) |
| 2026-06-03 | [Transformer-Initiated Recompare as a Correspondence Contract](2026-06-03-transformer_initiated_recompare.md) | Superseded by [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md) |
| 2026-06-03 | [Structured Summary Segments](2026-06-03-structured-summary-segments.md) | Implemented |
| 2026-06-03 | [Progressive Renderer Annotations](2026-06-03-progressive_renderer_annotations.md) | Implemented |
| 2026-06-03 | [Error Diagnostics Are Reportable Findings](2026-06-03-error_diagnostics_are_reportable_findings.md) | Implemented |
| 2026-06-02 | [Markdown Renderer Groups Replace Significance-Map Grouping](2026-06-02-renderer_groups.md) | Implemented |
| 2026-06-01 | [Unified Dataset Config and Identity Policy](2026-06-01-unified_dataset_config_and_identity.md) | Accepted; implementation notes superseded in part by [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md) |
| 2026-06-01 | [Tabular Collection Artifact Model](2026-06-01-tabular_collection_artifact_model.md) | Accepted |
| 2026-06-01 | [Single-stream gzip as an expanding comparator](2026-06-01-single_stream_gzip_as_expanding_comparator.md) | Implemented |
| 2026-06-01 | [Optional First-Party Plugins and `binoc[all]`](2026-06-01-optional_first_party_plugins.md) | Accepted |
| 2026-06-01 | [Example Verbosity and Plugin-Supplied Details](2026-06-01-example_verbosity.md) | Decided |
| 2026-06-01 | [Diagnostics Channel for Non-Fatal Warnings and Suggestions](2026-06-01-diagnostics_channel.md) | Implemented |
| 2026-05-13 | [Rename-and-modify detection: fuzzy correlation + transformer-initiated re-dispatch](rename_modify_detection.md) | Superseded in part by [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md) |
| 2026-04-17 | [Documentation Platform and Information Design](2026-04-17-documentation_platform_and_info_design.md) | Proposed |
| 2026-04-16 | [Transient Fields Are Wire-Visible; Output Stripping Is a Boundary Concern](2026-04-16-transient_fields_on_wire.md) | Implemented |
| 2026-04-16 | [Transformer dispatch: bottom-up by default, Root for tree-wide walkers](2026-04-16-transformer_scope_yagni.md) | Implemented |
| 2026-04-16 | [Test vector materialization: plugin trait, not a runtime plugin point](2026-04-16-test_vector_materialization.md) | Implemented |
| 2026-04-16 | [Opportunistic ItemRef Metadata, Transformer-Hydrated for Correlation](2026-04-16-opportunistic_itemref_metadata.md) | Implemented |
| 2026-04-10 | [Security posture and how to audit Binoc (core and plugins)](2026-04-10-security_posture_and_auditing.md) | Accepted |
| 2026-04-10 | [Rust MSRV and dependency update policy](2026-04-10-rust_msrv_and_dependency_update_policy.md) | Implemented (MSRV raised to 1.95 on 2026-06-29 — see Addendum) |
| 2026-04-10 | [Independent release tags and published version policy](2026-04-10-independent_release_tags_and_published_version_policy.md) | Implemented |
| 2026-04-08 | [Release Surface And Automated Publishing](2026-04-08-release_surface_and_automated_publishing.md) | Implemented |
| 2026-03-20 | [Transformer Dispatch Refinement](2026-03-20-transformer_dispatch_refinement.md) | Implemented |
| 2026-03-20 | [Transformer Composition and Artifact Flow](2026-03-20-transformer_composition_and_artifact_flow.md) | Superseded in part by [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md) |
| 2026-03-19 | [Published artifacts for cross-plugin composition](2026-03-19-published_artifacts_for_cross_plugin_composition.md) | Implemented |
| 2026-03-18 | [Terminology](2026-03-18-terminology.md) | Accepted; updated by the 2026-06-12 correspondence-first migration |
| 2026-03-12 | [Plugin SDK, ABI Safety, and Native Plugin Loading](2026-03-12-plugin_sdk_and_abi.md) | Implemented |
| 2026-03-09 | [Test Vector Root Defaults and Plugin Test Vectors](2026-03-09-test_vector_defaults_and_plugin_vectors.md) | Implemented |
| 2026-03-09 | [Standard Library Boundary Policy](2026-03-09-stdlib_boundary.md) | Accepted |
| 2026-03-09 | [Shared Test-Vector Harness for Plugins](2026-03-09-plugin_test_vector_harness.md) | Implemented |
| 2026-03-09 | [Per-Renderer Config and Significance as a Renderer Concern](2026-03-09-renderer_config.md) | Decided (implemented) |
| 2026-03-09 | [Output Routing and CLI UX](2026-03-09-output_routing_and_cli_ux.md) | Implemented |
| 2026-03-09 | [Deferred Performance Optimizations](2026-03-09-deferred_optimizations.md) | Decided (not pursuing) |
| 2026-03-06 | [`just` as the Canonical Task Runner](2026-03-06-just_as_task_runner.md) | Implemented |
| 2026-03-06 | [Tutorial Regeneration Is a Build Step, Not a Test](2026-03-06-tutorial_regeneration_lifecycle.md) | Implemented |
| 2026-03-06 | [Plugin Discovery and the Rust/Python Boundary](2026-03-06-plugin_discovery.md) | Implemented |
| 2026-03-05 | [Snapshot Testing for Test Vectors](2026-03-05-snapshot_testing_for_test_vectors.md) | Implemented |
| 2026-03-05 | [Provenance Tracking and the Extract Chain](2026-03-05-provenance_and_extract.md) | Implemented |
| 2026-03-05 | [Media Type Detection and Content-Aware Dispatch](2026-03-05-media_type_detection.md) | Implemented (phase 1) |
| 2026-03-05 | [Full Comparison Tree and Content Hash Propagation](2026-03-05-full_comparison_tree_and_content_hashes.md) | Superseded in part by [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md) |
| 2026-03-05 | [Cross-Phase Data Cache in CompareContext](2026-03-05-cross_phase_data_cache.md) | Superseded by [Published artifacts for cross-plugin composition](2026-03-19-published_artifacts_for_cross_plugin_composition.md) and [Correspondence-First Engine](2026-06-12-correspondence_first_engine.md) |
