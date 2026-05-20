# Meta-feedback Log

The implementation and manager agents append entries here when the design
spec is ambiguous, contradictory, or silent on something relevant. Use this
log to refine the spec or the agent prompts in future iterations — do not
edit existing entries.

Format:

```
## {ISO timestamp} — {implementation|manager} — {slide-id}

What was unclear, which spec sections conflicted, what the agent chose.
```

---

## 2026-04-24T00:00:00Z — manager — 00-0c402695-1379-4910-911f-ee311fd03635

Two template-level conflicts between `cantaloupe.design-contracts.json` and
`carousel-manifest.md` surfaced on this slide — both inherited from the
canonical page layout rather than caused by the implementation.

1. **Eyebrow text-to-rule clearance.** Manifest §3 fixes tag/folio at y=34
   and the top rule at y=56 (gap 22 px). At 10 px font, the descender
   formula `24 + (10×0.22) = 26.2 px` is never satisfiable with only 22 px
   of room. Same issue mirrored at the bottom: rule y=1294 / colophon
   y=1316 gap of 22 px. The contract's descender formula is plainly aimed
   at body/headline sizes, not tracked 10 px eyebrows — ruled acceptable.

2. **Shape bottom on 8 px grid vs canonical shape height.** Contracts
   §grid requires both shape top and bottom to be multiples of 8. Manifest
   §8 fixes `bar-right-small` at 6 × 260 px. 260 is not a multiple of 8,
   so either top or bottom must drift off the grid (here top=544 ✓, but
   bottom=804 ✗). Ruled acceptable — the canonical size dominates.

Suggested spec fixes: (a) add an exception to grid §baselineSnapPx for
10 px header/footer eyebrows; (b) either set bar-right-small height to
256 or 264, or relax the grid contract to only require shape TOP on the
8 px grid.

---

## 2026-04-25T00:00:00Z — manager — 03-b81764eb-569f-4c22-8bcb-4119c40f20ca

Contracts say `caption.position: "below-body-only"`, but this slide is a
hybrid plate: intro body → numbered list → caption. The plate-list slide
type spec doesn't include a body or caption, and the plate spec assumes
body→caption with no list between. Contracts are silent on hybrid
plate+list+caption layouts.

Ruled acceptable: the caption still functions as a closing "quieter
register" below all primary content. Group-to-group spacing (list→caption
= 80 px) exceeds the 48 px minimum. I read "below-body-only" as forbidding
caption-above-body, not forbidding intervening list elements.

Suggested spec fix: clarify whether `caption.position: below-body-only`
means "below body specifically, never beneath a list" or "in the lower
half of the content area, after primary content blocks." Add a hybrid
plate-list-with-body slide type or explicitly disallow this combination.


## 2026-04-25T00:00:00Z — manager — 06-53baaa6d-7e99-42b8-afa6-1a693fa8345e

Three contract gaps surfaced on this plate (pricing slide):

1. **Typographic mono data displays.** The slide renders price data ($5 / $25) in JetBrains Mono as a tabular list, not as a code block. The contracts define `code` (2px primary left rule, 24px indent) and inline code, but say nothing about non-code mono data displays. Ruled acceptable because the element does not claim to be a code block and the implementation distinguishes the two roles in CSS. Gap: contracts should either (a) bless mono data displays as a separate role with its own rules, or (b) require the code-block left-rule treatment for any block JetBrains Mono.

2. **Italic asides at non-caption sizes.** The slide includes a `~ 67% more expensive than Sonnet.` line in italic ink-dim at 24px, positioned between the pricing display and the body — not below the body. The `caption` contract is 14px, ≤520px, italic, ink-dim, below-body-only. This element shares the italic+ink-dim styling but is larger and positioned mid-content. Contracts don't address italic asides that aren't strictly captions. Ruled acceptable because no explicit rule is violated; the role reads as a "summary aside," not a caption.

3. **Italic + primary for prose.** The closing `What's your experience?` is italic primary at 24px, used as a soft CTA. Body emphasis is specified as italic (allowed), and headline accents are primary (allowed), but the contracts don't explicitly address combining italic with primary on a non-headline, non-body, non-caption element. Ruled acceptable; flagging because the role "italic CTA in primary" is undefined.

How I decided to rule: each ambiguous element passes the explicit hard-rule checks (color count, font family, grid snap, safe zones, line-height floors, headline rules). Accepting on the plain reading of the contracts, with this note so the spec can be tightened if these patterns recur across the deck.


## 2026-04-28T00:00:00Z — manager — 01-1bf8d55a-08d3-4321-a4d8-a1e710b6cf19

Caption max-column conflict between contracts and manifest in landscape.

`cantaloupe.design-contracts.json` §caption sets `maxColumnPx: 520`
unconditionally (no orientation split). `carousel-manifest.md` §9 says:
"Flush left, max column width: vertical = 520 px; landscape = 600 px".
The system prompt instructs the manager to use the landscape variants of
per-orientation rules, but the contracts file is silent on landscape for
caption.

This slide ships caption width = 600 px (landscape value from manifest).
Ruled acceptable because (a) the manifest is explicit about the
landscape variant, (b) 600 px is still well inside the 1664 px content
area, and (c) other orientation-coupled values (body, code, title) all
have explicit landscape entries in contracts — caption appears to be an
oversight rather than a deliberate same-width-everywhere rule.

Suggested spec fix: add `caption.maxColumnPx: { vertical: 520, landscape: 600 }`
to `cantaloupe.design-contracts.json` so the contracts and manifest
agree.

## 2026-05-18T00:00:00Z — implementation — 02-d15b1fc9-a42f-478a-9da2-91f6cd295c09

**Hard rule #10 vs. manifest/contracts logo filename and width conflict.**

Hard rule #10 (in the implementation agent system prompt) specifies:
- filename: `cantaloupe-logo.svg`
- width: `132px`
- alt: `"Cantaloupe"`

`carousel.manifest.json#branding.headerLogo` and `cantaloupe.design-contracts.json#branding.headerLogo` both specify:
- filename: `happycampr-logo.svg`
- widthPx: `118` (native 54:11 aspect at 24px height → 24 × 1080/220 ≈ 118px)
- alt: `"happycampr"`

The driver actually copied the file as `happycampr-logo.svg` (confirmed via directory listing). The canonical `carousel_example.md` also uses `happycampr-logo.svg` with width 118px and was "visually verified through the real pipeline."

**Chose:** `happycampr-logo.svg`, `width: 118px`, `alt="happycampr"` — aligning with the machine-readable spec, the design contracts, and the verified example. Hard rule #10 appears to be a copy-paste artifact from the pre-rebrand Cantaloupe template that was not updated when the brand switched to happycampr.

Suggested fix: update hard rule #10 in the implementation agent system prompt to match the happycampr values (`happycampr-logo.svg`, `118px`, `alt="happycampr"`).

## 2026-05-19T00:00:00Z — implementation — 04-fda23f48-7805-4dd1-9bfa-9c8b56159592

**pie-chart slice colors vs. strict `seriesPalette` order.**

`carousel.manifest.json#chart.seriesPalette` lists `["primary","warning","secondary","complement"]` which resolves to `["#946334","#946334","#5A6B4C","#2B1810"]`. Slots 1 and 2 are identical because the happycampr brand has no amber and `warning` maps to `primary` (graham).

For a bar/column/line chart, a duplicate colour on adjacent series just means two series look the same — mitigated by labels. For a **pie chart**, adjacent same-coloured slices visually merge into one arc, destroying the segmentation entirely (Referral 46% + Search 28% would appear as one 74% wedge).

**Chose:** use four visually distinct brand tones in order — `#946334` (primary), `#5A6B4C` (secondary), `#2B1810` (complement/ink), `#7C6F66` (ink-dim) — skipping the `warning` duplicate. All data is redundantly labelled in the series key, so slice-colour is not the sole encoding.

Suggested spec fix: (a) add a pie-chart exception to the `seriesPalette` order that skips exact duplicates, or (b) offer an alternate 4-tone pie palette using ink-dim as the fourth slot so pie charts can be fully colour-distinct without violating brand constraints.
