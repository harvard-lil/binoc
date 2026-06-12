# Architectural Decisions

ADRs (Architecture Decision Records) capture the rationale behind binoc's design — including alternatives that were considered and rejected. They are the canonical long-form record of the project's reasoning.

Newer entries appear first. Each entry shows its date and current status. Create a new ADR with `just adr <title>`. See the [Documentation platform ADR](2026-04-17-documentation_platform_and_info_design.md) for how this index is produced and how ADRs fit into the docs site.

| Date | Title | Status |
|---|---|---|
| 2026-06-12 | [Invariant and Lint Tiers: Harness, Mechanical, Agent](2026-06-12-invariant_and_lint_tiers.md) | Implemented |
| 2026-06-11 | [Inline Pure-Reorder Judgment; Retire Tag-Handoff Layering](2026-06-11-inline_pure_reorder_judgment.md) | Implemented |
| 2026-06-11 | [Declared Write-Sets on TransformerDescriptor](2026-06-11-declared_write_sets_on_transformer_descriptor.md) | Implemented |
| 2026-06-03 | [Transformer-Initiated Recompare as a Correspondence Contract](2026-06-03-transformer_initiated_recompare.md) | Implemented |
| 2026-06-03 | [Structured Summary Segments](2026-06-03-structured-summary-segments.md) | Implemented |
| 2026-06-03 | [Progressive Renderer Annotations](2026-06-03-progressive_renderer_annotations.md) | Implemented |
| 2026-06-03 | [Error Diagnostics Are Reportable Findings](2026-06-03-error_diagnostics_are_reportable_findings.md) | Implemented |
| 2026-06-02 | [Markdown Renderer Groups Replace Significance-Map Grouping](2026-06-02-renderer_groups.md) | Implemented |
| 2026-06-01 | [Unified Dataset Config and Identity Policy](2026-06-01-unified_dataset_config_and_identity.md) | Accepted |
| 2026-06-01 | [Tabular Collection Artifact Model](2026-06-01-tabular_collection_artifact_model.md) | Accepted |
| 2026-06-01 | [Single-stream gzip as an expanding comparator](2026-06-01-single_stream_gzip_as_expanding_comparator.md) | Implemented |
| 2026-06-01 | [Optional First-Party Plugins and `binoc[all]`](2026-06-01-optional_first_party_plugins.md) | Accepted |
| 2026-06-01 | [Example Verbosity and Plugin-Supplied Details](2026-06-01-example_verbosity.md) | Decided |
| 2026-06-01 | [Diagnostics Channel for Non-Fatal Warnings and Suggestions](2026-06-01-diagnostics_channel.md) | Implemented |
| 2026-05-13 | [Rename-and-modify detection: fuzzy correlation + transformer-initiated re-dispatch](rename_modify_detection.md) | Implemented |
| 2026-04-17 | [Documentation Platform and Information Design](2026-04-17-documentation_platform_and_info_design.md) | Proposed |
| 2026-04-16 | [Transient Fields Are Wire-Visible; Output Stripping Is a Boundary Concern](2026-04-16-transient_fields_on_wire.md) | Implemented |
| 2026-04-16 | [Transformer dispatch: bottom-up by default, Root for tree-wide walkers](2026-04-16-transformer_scope_yagni.md) | Implemented |
| 2026-04-16 | [Test vector materialization: plugin trait, not a runtime plugin point](2026-04-16-test_vector_materialization.md) | Implemented |
| 2026-04-16 | [Opportunistic ItemRef Metadata, Transformer-Hydrated for Correlation](2026-04-16-opportunistic_itemref_metadata.md) | Implemented |
| 2026-04-10 | [Security posture and how to audit Binoc (core and plugins)](2026-04-10-security_posture_and_auditing.md) | Accepted |
| 2026-04-10 | [Rust MSRV and dependency update policy](2026-04-10-rust_msrv_and_dependency_update_policy.md) | Implemented |
| 2026-04-10 | [Independent release tags and published version policy](2026-04-10-independent_release_tags_and_published_version_policy.md) | Implemented |
| 2026-04-08 | [Release Surface And Automated Publishing](2026-04-08-release_surface_and_automated_publishing.md) | Implemented |
| 2026-03-20 | [Transformer Dispatch Refinement](2026-03-20-transformer_dispatch_refinement.md) | Implemented |
| 2026-03-20 | [Transformer Composition and Artifact Flow](2026-03-20-transformer_composition_and_artifact_flow.md) | Implemented |
| 2026-03-19 | [Published artifacts for cross-plugin composition](2026-03-19-published_artifacts_for_cross_plugin_composition.md) | Implemented |
| 2026-03-18 | [Terminology](2026-03-18-terminology.md) | Accepted |
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
| 2026-03-05 | [Full Comparison Tree and Content Hash Propagation](2026-03-05-full_comparison_tree_and_content_hashes.md) | Implemented |
| 2026-03-05 | [Cross-Phase Data Cache in CompareContext](2026-03-05-cross_phase_data_cache.md) | Superseded by [Published artifacts for cross-plugin composition](2026-03-19-published_artifacts_for_cross_plugin_composition.md) |
