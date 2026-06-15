---
audience: core developers thinking about how to grow binoc's plugin ecosystem before the SDK is stable
---

# Nurturing a plugin ecosystem under an unstable SDK

!!! note "Research note, not normative documentation"
    This is a background survey of how other platforms managed plugin
    ecosystems while their APIs were still moving, plus the implications
    those precedents suggest for binoc. It explains trade-offs; it does not
    make decisions. When a decision is made, it belongs in an
    [ADR](../../adr/README.md), which can cite this survey. Web research
    verified against repos, registries, and announcements, June 2026.

*The question is not "how do plugin ecosystems work" but: **when the
platform API is unstable, who absorbs the cost of breaking changes, and
what infrastructure makes that absorption cheap?** Each precedent below is
interrogated with the same template: where do plugins live (in-tree /
mono-org / federated)? Who fixes a plugin when the SDK breaks it? What
infrastructure detects breakage before release? And what changed at
stabilization — what was the exit path from the early arrangement?*

*The headline finding: the precedents sort into a small number of
mechanisms — co-evolution in one tree, ecosystem-wide regression testing,
migration tooling shipped with the break, stability tiers inside the API,
and registry policy as leverage — and the mechanisms compose. The thesis
worth testing: AI code generation makes the "author of the breaking change
fixes every consumer" model, previously viable only inside Google-scale
monorepos, available to small teams coordinating an open ecosystem.*

## Where binoc stands today

All real plugins live in-tree under `model-plugins/` (binoc-sqlite,
binoc-stat-binary, binoc-html, binoc-row-reorder), in the same workspace as
the SDK they depend on. `third_party_plugins.json` is an in-tree registry
describing each plugin's dispatch claims, packages, and entry points —
currently it lists only in-tree plugins, but its schema doesn't assume
that. Plugins, including hypothetical out-of-tree ones, can run the shared
test-vector harness via `binoc_stdlib::test_vectors` without duplicating
harness code. That combination — a registry plus a uniform conformance
harness — turns out to be exactly the raw material several precedents
below had to build deliberately.

## Co-evolution in one tree

**Linux kernel.** The most explicit statement of the in-tree bargain is
the kernel's own
[stable-api-nonsense.rst](https://docs.kernel.org/next/process/stable-api-nonsense.html)
(Greg Kroah-Hartman): there is *no* stable internal API, by design, and
the compensation is that when an interface changes, "all instances of
where this interface is used within the kernel are fixed up at the same
time." The author of the breaking change pays; in-tree drivers ride along
for free; out-of-tree modules face what the document calls a nightmare of
per-version, per-distro tracking, with no sympathy. The document's
standing advice is simply: get your driver into the tree. Two decades of
evidence support both halves — the model works, and its costs are real:
the quality bar and review process gate entry, and projects that can't or
won't clear it (NVIDIA's driver, ZFS) route around the tree permanently.

**Google's monorepo and Large-Scale Changes.** The strongest precedent
for *author-pays at scale* is Google's LSC process
([Software Engineering at Google, ch. 22](https://abseil.io/resources/swe-book/html/ch22.html)):
infrastructure teams making a breaking change are expected to fix the
hundreds of thousands of references to it themselves, because they — not
the downstream teams — have the domain knowledge, and because anything
else is an "unfunded mandate." The Rosie tool shards an ecosystem-wide
change along ownership boundaries into independently submittable pieces,
routes each shard to a reviewer via OWNERS detection, and runs the
centralized test infrastructure over all of it — thousands of changes per
day. Pre-AI, this took Google-scale tooling and headcount. The relevance
to binoc is direct: LSC mechanics — find every consumer, generate the
fix, verify against the consumer's own tests, send for review — are
precisely what LLM-based agents now do cheaply (see
[migration tooling](#migration-tooling-shipped-with-the-break) below).

**Home Assistant.** The largest living example of "keep the plugins
in-tree and evolve them with the platform": 2,000+ integrations in the
[home-assistant/core](https://github.com/home-assistant/core) monorepo,
releases with breaking changes the first Wednesday of every month,
per-integration ownership via CODEOWNERS entries declared in each
integration's manifest
([ADR-0008](https://github.com/home-assistant/architecture/blob/master/adr/0008-code-owners.md)),
and a tiered
[integration quality scale](https://developers.home-assistant.io/docs/core/integration-quality-scale/)
(Bronze/Silver/Gold/Platinum) — since
[November 2024](https://developers.home-assistant.io/blog/2024/11/20/integration-quality-scale/),
Bronze is the minimum bar for any new in-tree integration. The
out-of-tree contrast exists but is anecdotal: HACS custom components do
break across core releases (there is a community
[breaking_changes](https://github.com/custom-components/breaking_changes)
integration just to warn about it), though no published data quantifies
the in-tree/out-of-tree breakage gap. The structural lesson: a monorepo
does not require centralized maintenance — manifest-declared per-plugin
owners inside one tree give shared evolution *and* distributed
responsibility, and a quality ladder makes the entry bar explicit rather
than reviewer-discretionary.

## Ecosystem-wide regression detection

**Rust crater.** [Crater](https://github.com/rust-lang/crater) builds —
and optionally tests — every crate on crates.io (plus a small set of
GitHub repos) against a candidate compiler. Compiler contributors request
runs from PR threads; a check-only run over the whole ecosystem takes 3–4
days. Its cultural effect exceeds its mechanical one: crater turned "is
this bug fix a de facto breaking change?" from an argument into a
measurement, which is what lets Rust make judgment calls about
[Hyrum's-law](https://www.hyrumslaw.com/) breakage with evidence in hand.
The prerequisite is just an enumerable ecosystem and a uniform way to
build and test each member.

**Jenkins: the mono-org pattern.** Jenkins plugins live in *separate
repos under one GitHub org* ([jenkinsci](https://github.com/jenkinsci),
2,000+ plugins) with shared CI, shared release infrastructure, and a
plugin BOM. The
[Plugin Compatibility Tester](https://github.com/jenkinsci/plugin-compat-tester)
runs plugins' own test suites against a candidate Jenkins core — a
crater for plugins. And the
[Plugin Modernizer](https://github.com/jenkins-infra/plugin-modernizer-tool)
(active, OpenRewrite-based) applies migration recipes across the org and
opens automated PRs — mass mechanical maintenance *without* a monorepo.
The mono-org is a genuine middle point on the spectrum: separate
ownership and release cadence, but centralized ability to test and patch
the whole ecosystem.

**Swift source compatibility suite.** Apple's
[swift-source-compat-suite](https://github.com/swiftlang/swift-source-compat-suite)
inverts the registration: projects *volunteer* into the suite by PR, and
in exchange the Swift compiler CI builds them on every candidate change.
The contract is explicit — projects must be open source, build on
supported platforms, and have maintainers who resolve issues within two
weeks, or they're dropped. Registration buys protection; protection
requires responsiveness.

**The failure cases are instructive.** pytest's
[plugincompat](https://github.com/pytest-dev/plugincompat) service (ran
known pytest plugins against new pytest releases) is retired; Gentoo's
tinderbox, which built the entire portage tree against toolchain changes
and filed 4,000–5,800 bugs a year, was
[shut down February 2025](https://blogs.gentoo.org/ago/2025/02/01/tinderbox-shutdown/)
for lack of hardware; CPAN Testers spent months in a degraded state
before a
[2025 rescue effort](https://www.perl.com/article/perl-toolchain-summit-2025-key-results/).
Ecosystem-wide CI is infrastructure with ongoing cost, and it dies when
it depends on one volunteer or one machine. A small ecosystem like
binoc's has the advantage that the equivalent service is a CI job, not a
fleet — but the sustainability lesson still applies to whatever the job
grows into.

## Migration tooling shipped with the break

**Go's gofix.** During Go's pre-1.0 period (public release Nov 2009 →
Go 1 in March 2012), the team shipped
[gofix](https://go.dev/blog/introducing-gofix): each breaking API change
came with a mechanical rewriter, so updating a codebase was one command.
Breaking changes stayed cheap for users precisely *because* the platform
team paid the cost of writing the migration alongside the break. Then the
[Go 1 compatibility promise](https://go.dev/doc/go1compat) ended the era
deliberately, and ecosystem growth followed the stability promise — the
sequence (break freely with migration tooling → promise stability →
ecosystem grows) is the cleanest version of the pre-1.0 playbook on
record.

**Rust editions.** The post-1.0 evolution of the same idea: editions
(2015/2018/2021/2024, the
[2024 edition](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)
shipping in Rust 1.85) allow opt-in surface breakage, `cargo fix
--edition` automates the migration, and crates of different editions
interoperate in one build — so the ecosystem never has to move in
lockstep. The interop point is the part Python lacked.

**Python 2→3, the negative control.** Twelve years (Python 3.0 in
December 2008 → Python 2 EOL January 2020) for a migration that was
individually cheap per library. The failure dynamics are well documented
in core-dev retrospectives (e.g.
[Guido's BDFL retrospective](https://2018.pycascades.com/talks/bdfl-python-3-retrospective/)):
2to3 was a one-way translator, so libraries couldn't straddle both worlds
until the community built [six](https://six.readthedocs.io/) itself;
there was no interop bridge, so migration had to proceed bottom-up
through the dependency DAG; and every node in that DAG needed an *active
maintainer* to move, so abandoned foundational libraries gated everyone
above them. No central actor was positioned to pay the cost on
maintainers' behalf. This is precisely the failure mode that author-pays
plus automated fix generation is designed to avoid — and the reason a
registry that knows about every plugin matters: you can't pay costs for
consumers you can't enumerate.

**Codemod culture, then AI.** The JS ecosystem normalized shipping
migrations with breaks: Facebook's
[jscodeshift](https://github.com/facebook/jscodeshift) underpins official
codemods from [React](https://github.com/reactjs/react-codemod) and
[Next.js](https://nextjs.org/docs/app/guides/upgrading/codemods)
(`npx @next/codemod upgrade` accompanies every major release). What's
changed in 2024–2026 is that the migrations no longer need to be
hand-written AST transforms: Amazon's Q Code Transformation
[upgraded 1,000 internal production Java apps from 8 to 17 in two days](https://aws.amazon.com/blogs/devops/three-ways-amazon-q-developer-agent-for-code-transformation-accelerates-java-upgrades/)
(~10 minutes per app); Google's [Jules](https://jules.google/) (GA
August 2025) takes "upgrade this app to Next.js 15" as a task, runs the
tests in a VM, and opens the PR;
[OpenRewrite-plus-agents](https://www.moderne.ai/blog/open-source-auto-refactoring-meets-ai-agent-to-modernize-fintech-software-at-scale)
platforms run recipe-based migrations across thousands of repos. The
Google-LSC loop — enumerate consumers, generate fixes, verify with the
consumer's own tests, open reviewable PRs — is now a commodity. For a
platform with a dozen or even a hundred registered plugins, generating a
fix PR per plugin per breaking change is an afternoon of agent time, *if*
each plugin has a test suite that can adjudicate the fix.

## Stability tiers and protocol seams

**VS Code proposed APIs.** Unstable extension APIs are marked
[proposed](https://code.visualstudio.com/api/advanced-topics/using-proposed-api):
usable only in development and Insiders builds, opted into per-extension
(`enabledApiProposals`), and — the teeth — extensions using them *cannot
be published to the Marketplace*. APIs graduate to stable after bake
time. Rust's
[nightly feature gates](https://doc.rust-lang.org/nightly/unstable-book/)
are the same pattern. The affordance: instability becomes a *labeled
subset* of the API rather than a property of the whole platform, and the
registry/marketplace is the enforcement point.

**Terraform's extraction story.** Providers originally lived in the
hashicorp/terraform repository; [Terraform 0.10
(June 2017)](https://www.hashicorp.com/blog/upcoming-provider-changes-in-terraform-0-10)
split them into separate repos, fetched on demand, communicating over a
versioned plugin protocol (go-plugin/gRPC, protocol versions 5 and 6).
Notably the *SDK* came out two years later
([terraform-plugin-sdk, October 2019](https://www.hashicorp.com/en/blog/announcing-the-terraform-plugin-sdk),
succeeded by terraform-plugin-framework) — and the SDK then became a
product with its own versioning and compatibility burden. The split
succeeded because the seam (the wire protocol) was versioned before the
repos separated; the lesson usually drawn is that provider development
decoupled from core's release cadence, and the cost is that every
interface the plugins touch is now a public contract.

**Kubernetes: publish-from-monorepo, extract after maturity.** Two
distinct moves. First,
[staging](https://github.com/kubernetes/kubernetes/blob/master/staging/README.md):
`staging/src/k8s.io/*` inside the monorepo is the authoritative source
for what users consume as separate repos (client-go etc.), published
outward by automation — separate installable packages without separate
development repos. Second, extraction: in-tree cloud providers were
removed only behind a matured interface, on a long runway (feature gate
introduced v1.22 in 2021, on by default
[v1.29, December 2023](https://kubernetes.io/blog/2023/12/14/cloud-provider-integration-changes/),
locked v1.31, code removal after); in-tree volume plugins were replaced
by CSI with a translation layer preserving compatibility during the
migration. The ordering is the point: interfaces stabilized *first*,
extraction followed, never the reverse.

## Registry policy as leverage

**WordPress — the platform-absorbs-everything pole.** WordPress holds
backward compatibility as near-absolute policy, so plugins essentially
never break; the directory's
[maintenance policy](https://developer.wordpress.org/plugins/wordpress-org/plugin-developer-faq/)
is correspondingly mild (plugins are closed after ~6 months of no commits
or a year of breakage, sometimes without notice, rather than warned).
The cost lands entirely on the platform: WordPress core carries its
compatibility burden forever. This pole is coherent — but unavailable to
a pre-1.0 project, whose whole purpose in being pre-1.0 is to keep
breaking things.

**JetBrains — the registry runs the checks.** JetBrains Marketplace runs
the open-source
[Plugin Verifier](https://github.com/JetBrains/intellij-plugin-verifier)
automatically against uploads, checking binary compatibility against IDE
builds and flagging use of deprecated and internal APIs
([compatibility docs](https://plugins.jetbrains.com/docs/intellij/verifying-plugin-compatibility.html)).
Centralized verification means plugin authors learn about incompatibility
from the registry, not from users.

**Chrome MV2→MV3 — the deadline gate.** Announced ~2020, repeatedly
delayed, then executed on a published
[timeline](https://developer.chrome.com/docs/extensions/develop/migrate/mv2-deprecation-timeline):
disabled by default in stable October 2024, fully disabled (no
re-enable) July 2025. A forced migration *can* be driven to completion
by a platform with sufficient leverage — at the price of years of
friction and permanent loss of the extensions (and goodwill) that didn't
make the jump. The relevant transfer is less the coercion than the
shape: long runway, published dates, phased enforcement.

**Elm — the blessed-only extreme.** Two separable things. First, an
affordance worth stealing on its own:
[package.elm-lang.org](https://elm-lang.org/packages) *enforces* semver
mechanically — `elm bump` diffs the package's public API against the
prior release and computes the version; publishing a breaking change as
a patch is impossible. (Rust is converging on the same idea:
[cargo-semver-checks](https://crates.io/crates/cargo-semver-checks) is
actively maintained and merging it into `cargo publish` is a
[2026 Rust project goal](https://rust-lang.github.io/rust-project-goals/2026/cargo-semver-checks.html).)
Second, the policy: since Elm 0.19 (2018), "kernel" (native JS) code is
restricted to the `elm` and `elm-explorations` orgs — only blessed
packages may cross the language boundary. The fallout is
well-documented
([one retrospective](https://dev.to/kspeakman/elm-019-broke-us--khn)):
community projects broke with no sanctioned path forward, trust eroded,
and the ecosystem partially routed around the platform (the Lamdera
fork). The last Elm compiler release remains 0.19.1, from October 2019.
The boundary worth marking: *verifying* what plugins do (Elm's semver
machinery, JetBrains' verifier) strengthens an ecosystem; *forbidding*
non-blessed plugins shrinks it to what the core team can personally
attend to.

## What the precedents converge on

1. **Author-pays beats maintainer-pays during instability.** Linux,
   Google, and Home Assistant all converge on it; Python 2→3 is the
   documented cost of the alternative. The platform team breaking the
   API is the actor with the knowledge, the motive, and — now — the
   tooling to fix every consumer.
2. **Author-pays requires an enumerable ecosystem.** You can only fix
   the consumers you can find. Every working version of this has a
   census: the kernel tree, Google's monorepo, `jenkinsci/*`, Swift's
   suite, crates.io. The registry is not a directory; it is the
   instrument that makes ecosystem-wide obligations dischargeable.
3. **A uniform test harness is what makes automated fixes trustworthy.**
   Crater, PCT, and the Swift suite all reduce to "run each member's own
   checks against the candidate platform." Amazon's thousand-app
   migration worked because each app's tests adjudicated each fix.
   Without per-plugin verification, ecosystem-wide patching is
   ecosystem-wide vandalism.
4. **Stability tiers let one platform be stable and unstable at once.**
   VS Code and Rust nightly show instability working fine as a labeled,
   gated subset — with the registry as the natural enforcement point.
5. **Version the seam before splitting the repo; extract after the
   interface matures, not before.** Terraform and Kubernetes both ran
   this order deliberately; Kubernetes' staging mechanism shows
   "separate packages" doesn't have to mean "separate repos."
6. **Ecosystem infrastructure has a body count.** plugincompat,
   tinderbox, and near-miss CPAN Testers all show that the regression
   service itself needs an owner and a budget, or it quietly stops.
7. **Verification builds ecosystems; prohibition shrinks them.** Elm
   marks where the blessed-set logic, followed to its end, arrives.

## Implications for binoc

These are implications and open questions, not decisions; any of them
that ripens into a decision should get an ADR.

**The registry could be a contract, not a directory.**
`third_party_plugins.json` currently describes plugins; the precedents
suggest the leverage comes when an entry *buys* something (inclusion in
ecosystem CI; AI-generated fix PRs when the SDK breaks you; a listing
page) and *requires* something (test vectors that pass; a responsive
maintainer, or pre-granted permission to merge fix PRs — Swift's
two-week responsiveness rule is a working calibration). That structure
naturally yields tiers resembling the kernel's bargain: in-tree plugins
are fixed atomically in the breaking PR itself; registered out-of-tree
plugins get a generated fix PR and a compatibility report; unregistered
plugins get the changelog.

**A binoc-crater is nearly free, and is the prerequisite for
everything else.** The expensive parts of crater — enumerating the
ecosystem and uniformly testing each member — already exist as
`third_party_plugins.json` and the shared vector harness. A CI job that
checks out each registered plugin, builds it against candidate `main`,
and runs its vectors would make every SDK-breaking PR's blast radius a
measurement instead of a guess. The failure cases (tinderbox,
plugincompat) suggest keeping it a boring CI job owned by the project,
not a satellite service.

**Test vectors are the migration oracle.** The thesis that AI-generated
fix PRs make co-evolution tractable holds exactly to the extent that
each plugin carries vectors able to adjudicate a fix: a generated PR is
credible iff the plugin's vectors pass against the new SDK. This
reframes vector coverage requirements for registered plugins from
quality gate to load-bearing infrastructure — and suggests the
write-set-completeness style checks in the
[lint tiers](../../adr/2026-06-12-invariant_and_lint_tiers.md) matter
for third parties even more than for in-tree code.

**Repo design reads as a spectrum with a known safe ordering.**
Monorepo (now) → publish-from-monorepo, Kubernetes-staging style, if
"equal footing" packaging for binoc-sqlite/binoc-stat-binary is wanted
before the SDK stabilizes → mono-org (separate repos, shared org, shared
CI, mass-PR capability, Jenkins-style) → federation. Terraform's lesson
cautions against splitting repos before the plugin ABI/protocol seam is
versioned, and notes that the SDK becomes a separately-versioned product
the moment plugins live elsewhere. Nothing in the precedents suggests
the breakout has to happen early to be legitimate; Kubernetes and
Terraform both extracted *late*, after the interfaces had proven
themselves, and were judged to have ordered it correctly. Home
Assistant's manifest-declared code owners show how third-party-authored
plugins could live in-tree without the core team owning their
maintenance.

**Instability could be labeled rather than ambient.** A VS Code-style
`unstable` marking on SDK surfaces — with the registry as the
enforcement point for what registered plugins may depend on — would let
parts of the SDK freeze early while the rest keeps moving, and gives
plugin authors a vocabulary for risk that "pre-1.0, anything goes"
doesn't. Relatedly, Elm-style mechanical API diffing
(cargo-semver-checks for the Rust surface) could anchor an honest
changelog for the SDK itself.

**Stabilization can have observable exit criteria.** The Go sequence
suggests treating 1.0 not as a date but as a measurement: crater-style
pass rates stable across N consecutive releases; generated fix-PR diffs
trending toward empty; no `unstable`-gated API used by any registered
plugin for M months. Those are signals a small project can actually
collect.

**The counterweight to keep in view.** The in-tree pole has a known
failure mode: the entry bar becomes a review bottleneck and the
ecosystem routes around the blessed set (out-of-tree kernel modules,
HACS, Lamdera). The AI-fix-PR thesis is partly an answer — it lowers
the cost of *staying* blessed, and agents can carry some review burden —
but it is a hypothesis, not a result. Nobody has yet run author-pays
co-evolution for an open plugin ecosystem on AI labor; binoc would be
generating the evidence, and should instrument accordingly (track the
cost per breaking change of regenerating the ecosystem, and watch for
plugins accumulating in the "we merge their fix PRs because no
maintainer answers" state, which is the registry quietly becoming a
monorepo with extra steps).
