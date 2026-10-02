# Usage Lens interface design

The primary screen was designed with the built-in image-generation tool before implementation. The accepted, reviewed full-screen concept is retained outside this repository with local QA artifacts; no generated screenshot or synthetic account payload is shipped as a public application asset.

## Composition and color lock

A 208px white left navigation rail, a 54px utility header and a softly cool #f7f9fb working canvas. Content has a 40px desktop gutter and a maximum width of 1440px. Heading, explanatory line, conditional demo notice, three-metric band, a two-column chart/quota region, recent-evidence table and a provenance footer appear in that order. The app is a functional local dashboard, not a marketing page. Native system fonts avoid network assets. Main heading 32px/1.2, section heading 20px, body 14px/1.5, labels 12px; tabular numbers where appropriate.

Tokens: white surfaces, #172033 text, #68778d secondary text, #dce3ec borders, #4268e8 accent, #eef3ff selected navigation, #fff7e8 amber evidence notice. Border radius 10px, 4/8/12/16/24/32/40 spacing scale, 140ms transitions. Dark theme uses #101620 canvas, #171f2c surfaces, #edf2fb text and #9eacc2 secondary text. Both have visible high-contrast keyboard focus. Reduced-motion preference disables transitions. Icons are small code-native 20px outline symbols; the lens mark is a CSS optical disk.

## Components and responsive rules

Application shell, API/state utilities, overview, token chart, quota-window panel, reusable evidence table, activity filters, skill evidence view, settings, and an accessible event-detail drawer. No external fonts, images, analytics, model calls, or third-party runtime requests. At <=760px the rail becomes a compact header and horizontally scrollable navigation, summary metrics stack, the chart and quota column stack, and drawer fills available width. At 320px, labels and controls wrap; table scrolling stays inside its container.

## Evidence rules

Only backend-returned records are shown. Demo is shown only for a source whose backend mode is `demo`, with the permanent label “演示数据 / Demo data”. The numerical values in the design concept are explicitly synthetic visual-reference data and are never UI fallbacks. Unknown and invalid values are not zero. Large integer strings are formatted without converting through floating-point numbers. Lifetime totals and local event counts have distinct source scopes and are never added together. The concept's example input/output counts were removed before acceptance because the contract does not supply that attribution.

Daily bucket calendar labels are shown as supplied, without invented timezone conversion. The display range is anchored to the latest reported calendar bucket, not an implied current local day. Separate quota windows show only reported used percentages; no remaining-token inference. History is recorded snapshots only. Source collection time and source-as-of time are distinct. Partial history, omitted cloud activity, stale snapshots and refresh failures remain visible.

Requested, loaded and invoked skills are separate explicit evidence classes. Absence of direct evidence is unknown, never proof of zero use. Event content is an escaped-text local-only detail view, never HTML, and never routed to plugin query methods. Settings changes use the local backend. Retention/deletion requires an explicit typed confirmation with cancellable dialog. Authentication secrets are not collection targets; redaction is best-effort rather than a safety guarantee.

## Required verification

Light/dark and English/Chinese, 320/390/1440px, exact zero/unknown/large counts, empty and unavailable sources, partial and stale observations, network failure/retry, source switching with interrupted requests, date and event filters, detail open/close and Escape/focus restoration, all settings controls, cancelled and confirmed deletion, and retention. Native tests exercise shared UI models and rendered markup; browser bootstrap and WebAssembly DOM interactions require the separate browser run. Browser QA compares rendered output to the accepted concept using screenshots, with all QA images kept outside the repository.

## Reviewed semantic refinements

The concept was accepted by the lead after visual inspection. “Never an account-wide total” was replaced by “Source scope is shown”: the usage endpoint does return account-scope summary values but cross-surface coverage remains unverified. A source selector is a necessary addition to keep account namespaces separate; it never merges sources. The daily-bucket unknown-timezone caveat does not apply to known collection UTC instants or quota-reset epoch values. Date labels and counts always come from the selected backend source. Event-detail requests are cancelled and discarded when the source changes or the drawer closes, and each new request supersedes the previous one.

The optional collapsed “Source details & recorded breakdowns” region extends the required provenance workflow below the primary composition. It exposes source-reported peak/streak/longest-turn metrics, plan/type, metadata-only model/tool event counts, collection vs source-as-of times, adapter identity and capability gaps. Quota epoch reset values are labeled Unix seconds. A reported value above 100% remains exact in text, with an explicit note that the visual progress bar caps at 100%. Calendar gaps inside the chart range receive unknown-pattern columns, never zero-filled values. UTC arithmetic operates only on calendar labels; it does not assign a timezone to those source labels.

Imported sources have an additional factual response-token region in Overview. Its six source counters, model breakdowns and paginated response evidence are explicitly separate from official account summaries. Cached input/reasoning output remain labeled subsets; source-reported totals are not recomputed. A zero imported-record count means no records were imported, not zero historic usage. No cost or quota inference is performed. Skill evidence text explicitly warns that forked or overlapping histories do not describe unique executions.

## Rendered verification status (2026-10-02)

Local rendered QA has not passed. The available browser could not open the loopback dashboard. The explicitly authorized project Playwright workflow then stopped before any page loaded because its Chromium executable was missing. Installing the official browser with Playwright 1.63.0 into a separate writable cache failed: both Chromium and Chromium Headless Shell v1243 (153.0.8010.12) downloads from `cdn.playwright.dev` returned HTTP 200 with `Content-Type: text/html`, 195 bytes, containing “Site Unavailable / Unable to access this site.” Consequently archive extraction reported “End of central directory record signature not found.” No executable or rendered screenshot was produced, and no alternate download host or existing browser profile was used.

This is an environment/download prerequisite failure, not an observed application defect. Non-browser checks and the independently configured CI browser workflow are separate evidence. Visual fidelity, actual layout at 1440/390/320px, browser console health and browser interaction checks must be confirmed from a successful browser run before being reported as verified. The accepted concept and synthetic-only Playwright screenshot assertions are ready for that comparison.
