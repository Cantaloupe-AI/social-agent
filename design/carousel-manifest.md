# happycampr Carousel Manifest

The authoring spec for happycampr LinkedIn carousels. Markdown in, PDF out.

This document is the source of truth for anyone — human or agent — generating a happycampr carousel. Its companion files are `carousel.manifest.json` (machine-readable values) and `happycampr.design-contracts.json` (collision-prevention rules). Four-colour-palette compromises are logged in `ambiguous.md` §13.

---

## 1. Canvas

LinkedIn carousels render in two orientations. **Each carousel is a single
orientation** — every page in the same PDF must share an aspect ratio. The
driver injects an `ACTIVE ORIENTATION` line into the agent's system prompt;
follow that block exactly and ignore the other.

### 1.a Vertical (default)

| Property | Value | Note |
|---|---|---|
| Width | 1080 px | |
| Height | 1350 px | |
| Ratio | 4:5 | Portrait, traditional carousel |
| Page count | 3 to 10 | Linkedin's render limit |
| Export format | PDF | One page per slide |

### 1.b Landscape

| Property | Value | Note |
|---|---|---|
| Width | 1920 px | |
| Height | 1080 px | |
| Ratio | 16:9 | Presentation-standard |
| Page count | 3 to 10 | Same limit |
| Export format | PDF | One page per slide |

The orientation is metadata on the carousel row, set when the user creates
the carousel and editable from the carousel editor. The renderer reads it
to size the puppeteer viewport and PDF page; the agents read it from the
`ACTIVE ORIENTATION` header to pick the matching block of every rule below.

---

## 2. Surface

The carousel uses the marshmallow surface — happycampr never uses pure white. This is intentional: carousels are long-form content, and marshmallow replaces white everywhere in the brand.

| Token | Hex | Use |
|---|---|---|
| `surface` | `#F5F1E8` | Every slide background except takeaway |
| `surface-inverse` | `#2B1810` | Takeaway slide only (optional) |

---

## 2.5 Branding (current assumption: happycampr-only)

Until theming lands, every carousel renders with **happycampr brand
identity**. The header logo (see §4) is the full happycampr lockup;
the type system, color palette, and rules in this manifest are all
happycampr-specific.

### Future work: theming

A future version will let users pick from multiple themes — alternate
happycampr palettes, agency client brands, or a neutral/blank theme for
non-branded posts. When that lands:

- A `theme` column will exist on the carousel row (alongside `orientation`).
- The renderer + agents will read it and pick the matching `branding.{theme}`
  block from this manifest (parallel to how orientation works today).
- The `branding.headerLogo` spec, color palette, and any other brand-coupled
  rules become per-theme.

For v0.x we are the only users, so theming is **not** a blocker. The agent
should treat "happycampr-branded" as a hard assumption and always emit the
happycampr lockup. When theming arrives we'll re-derive — see
`carousel.manifest.json#branding.themingFutureWork`.

---

## 3. Grid

Based on Tailwind's 0.25 rem scale (4 px base). All vertical positions snap to the 8 px baseline grid.

| Token | px | rem | Tailwind | Purpose |
|---|---|---|---|---|
| `space-2` | 8 | 0.5 | `p-2` | Base grid unit |
| `space-4` | 16 | 1.0 | `p-4` | Text-to-text gap |
| `space-6` | 24 | 1.5 | `p-6` | **Text-to-rule safe zone** |
| `space-8` | 32 | 2.0 | `p-8` | **Shape-to-text safe zone** |
| `space-10` | 40 | 2.5 | `p-10` | **Shape-to-rule safe zone** |
| `space-12` | 48 | 3.0 | `p-12` | **Group-to-group safe zone** |
| `space-14` | 56 | 3.5 | `p-14` | Rule offset from page edge |
| `space-20` | 80 | 5.0 | `p-20` | Page top and bottom margin |
| `space-24` | 96 | 6.0 | `p-24` | Page horizontal margin |

### Page layout — vertical (1080 × 1350)

```
┌─────────────────────────────────────────────────────┐  y=0
│         [tag]              [folio]                  │  y=34  (tag/folio baseline)
│  ──────────────────────────────────────────────     │  y=56  (top rule)
│                                                     │
│                                                     │
│                   content area                      │
│                                                     │
│                                                     │
│  ──────────────────────────────────────────────     │  y=1294 (bottom rule)
│         [colophon left]     [colophon right]        │  y=1316 (colophon baseline)
│                                                     │
└─────────────────────────────────────────────────────┘  y=1350
           ↑ 96 px ↑                        ↑ 96 px ↑
```

- Content area: x = 96 to 984 (888 px wide), y = 96 to 1254 (1158 px tall)
- Top rule: y = 56, from x = 96 to x = 984
- Bottom rule: y = 1294, from x = 96 to x = 984

### Page layout — landscape (1920 × 1080)

```
┌──────────────────────────────────────────────────────────────────┐  y=0
│   [tag]                                          [folio]         │  y=34
│  ──────────────────────────────────────────────────────────────  │  y=56  (top rule)
│                                                                  │
│                       content area                               │
│                                                                  │
│  ──────────────────────────────────────────────────────────────  │  y=1024 (bottom rule)
│   [colophon left]                            [colophon right]    │  y=1046
│                                                                  │
└──────────────────────────────────────────────────────────────────┘  y=1080
   ↑ 128 px ↑                                          ↑ 128 px ↑
```

- Content area: x = 128 to 1792 (1664 px wide), y = 96 to 984 (888 px tall)
- Top rule: y = 56, from x = 128 to x = 1792
- Bottom rule: y = 1024, from x = 128 to x = 1792

The chrome (tag, folio, rule offsets, colophon) sits at the same y-coords
as vertical because eyebrow type at 10 px is scale-invariant. Only the
horizontal margin grows (96 → 128 px) so the wider canvas doesn't read
as crowded, and the bottom rule mirrors at `1080 − 56 = 1024`.

---

## 4. Header

Every slide has a header band: a logo at top-left, a folio at top-right,
and a 1 px hairline rule below them.

### Logo (top-left, default)

- Asset: the full happycampr lockup (mark + wordmark), shipped in **two
  colourways** so it stays legible on either header surface. Pick the one
  that contrasts with the slide background:
  - `design/assets/happycampr-logo-full.svg` — **burnt** lockup, for
    light/**marshmallow** slides (cover + all default plates).
  - `design/assets/happycampr-logo-full-marshmallow.svg` — **marshmallow**
    lockup, for dark/**burnt** surface-inverse slides (e.g. takeaway).
  Do **not** recolour, CSS-filter, or trace either; do not swap in an
  icon-only variant in the header slot. A logo that blends into its
  background (burnt-on-burnt or marshmallow-on-marshmallow) is a hard fail.
- Per-slide handling: the driver copies **both** variants into each slide's
  working directory as `happycampr-logo.svg` (burnt) and
  `happycampr-logo-inverse.svg` (marshmallow); HTML references the matching
  one by relative path — `<img src="happycampr-logo.svg" …>` on light
  slides, `<img src="happycampr-logo-inverse.svg" …>` on the burnt
  takeaway slide.
- Position: `top: 24 px, left: 96 px` (vertical) / `left: 128 px` (landscape)
- Size: `height: 24 px, width: 118 px` (preserves native 1080 × 220 aspect, 54:11)
- The logo replaces the legacy eyebrow `tag` text on every slide, **including
  the cover**. Cover slides do not get a separate "A Short Essay" eyebrow
  anymore — the logo is identity enough.
- Header-chrome exemption: the logo lives in the chrome zone above the top
  rule and is exempt from the 40 px `shape-to-rule` content safe zone. It
  must clear the rule by at least 8 px (logo bottom = y=48; rule = y=56).

### Folio (top-right)

- Same type spec as tag
- Position: `top: 34 px, right: 96 px`
- Content: Roman numeral `N / TOTAL` (e.g. `III / VII`)

### Top rule

- Full-width hairline between tag/folio and content
- Color: `ink` (`#2B1810`)
- Weight: 1 px
- Position: `y = 56, x = 96 to 984`

---

## 5. Footer

Every slide has a colophon band.

### Bottom rule

- Same spec as top rule
- Position: `y = 1294`

### Colophon (two columns)

- Same type spec as tag (10 px Inter semibold uppercase tracked)
- Position: `top: 1316 px`
- Left column: `Manifests for Agents` (or the essay title)
- Right column: `Josh · happycampr` (or author attribution)
- Cover exception: left shows author, right shows `Seven Plates` (plate count as words) or total count
- Takeaway exception: right shows `Fin.`

---

## 6. Typography

The carousel uses Inter variable for all type. Code blocks and list folio numbers use JetBrains Mono — a carousel-functional exception only; happycampr's brand is Inter-only (see `ambiguous.md` §13.3). Do not introduce any other typefaces.

### Type scale (font-size / line-height / tracking)

| Role | Size (px) | Size (rem) | Line-height | Tracking (em) | Weight | Use |
|---|---|---|---|---|---|---|
| `eyebrow` | 10 | 0.625 | 1.2 | +0.38 | 600 | Tag, folio, colophon |
| `caption` | 14 | 0.875 | 1.5 | 0 | 400 italic | Italic captions under body |
| `body-sm` | 18 | 1.125 | 1.55 | 0 | 400 | Inline code, dense list descriptions |
| `body` | 22 | 1.375 | 1.55 | 0 | 400 | Default running text |
| `body-lg` | 24 | 1.5 | 1.5 | 0 | 400 | Emphasized body |
| `title-sm` | 84 | 5.25 | 1.05 | −0.035 | 700 | Secondary titles when content dense |
| `title` | 104 | 6.5 | 1.05 | −0.035 | 700 | **Default plate headline** |
| `display` | 140 | 8.75 | 0.95 | −0.055 | 700 | Cover display only |

### Line-height contract (critical)

- Multi-line headlines at `title` size MUST use line-height ≥ 1.05
- Multi-line display headlines MUST use line-height ≥ 1.00
- Below these values, ascender of line N+1 collides with descender of line N in Inter
- `display: 0.95` is safe ONLY if the title is at most two lines and the descender-bearing letters (g, j, p, q, y) fall on the last line

### When to pick which title size

- Use `title` (104 px) by default
- Drop to `title-sm` (84 px) if the headline is longer than 4 words or wraps to 4+ lines at 104 px
- Use `display` (140 px) only on the cover, single or two-line

### Line break control

Headlines in markdown use the YAML block scalar (`|`) syntax which preserves newlines. The renderer treats each newline as an explicit `<br>`. The author controls line breaks, not the renderer. Rules:

- Each line of the block scalar becomes one rendered line
- If the rendered text exceeds the column width (820px for titles), it will still wrap — but this should be treated as a bug, not a feature
- Count characters before writing: at `title` size (104px) in Inter 700, a line holds roughly 10–12 characters at the vertical 820 px column, or ~18 characters at the landscape 1500 px hero band. At `title-sm` (84px), ~14 characters / ~22 characters respectively.
- If a line is longer than that budget, break it manually or drop the size tier

---

## 7. Color

From `design-tokens.json` (happycampr four-colour palette). Only use these — do not introduce other colors.

| Semantic | Hex | Where it appears |
|---|---|---|
| `surface` | `#F5F1E8` | Slide background |
| `ink` | `#2B1810` | Body text, rules, default headlines |
| `ink-dim` | `#7C6F66` | Captions, italic asides |
| `primary` | `#946334` | Accent words in headlines, code keys, shape fills |
| `secondary` | `#5A6B4C` | Moss shape fills (decorative only) |
| `warning` | `#946334` | Warm (graham) shape fills — happycampr has no amber |
| `complement` | `#2B1810` | Code numbers, rare emphasis — collapses to ink (no mulberry) |
| `code-string` | `#5A6B4C` | String literals in code |

### Color rules

- happycampr has only **two chromatic colours**: `primary` (graham) and `secondary` (moss). `ink`/`ink-dim`/`surface` are non-chromatic. Maximum two chromatic colours visible on a single slide still applies.
- `warning` maps to graham (`#946334`) — there is no amber. Shape `warning` and `warning-text` are the same colour; graham on marshmallow is WCAG AA body / AAA large so it is safe for text.
- `complement` collapses to ink/burnt (`#2B1810`) — there is no mulberry. Used for code numbers / rare emphasis only; do not use as a general accent.
- **Never** put text in Burnt-on-Moss or Graham-on-Moss (fails contrast — see `ambiguous.md` §10/§13).

---

## 8. Shapes

Decorative shapes serve composition, not decoration. Rules:

- Maximum 1 decorative shape per non-cover slide (cover may have 2: primary + accent)
- Shapes must respect the `safe-zone` contracts (40 px from any rule, 32 px from any text)
- Shapes are drawn using only these six primitives:

| Kind | Shape | Fill | Typical position | Safe-zone check |
|---|---|---|---|---|
| `circle-center` | Circle | `primary` | Cover: horizontally centered, y ≈ 400 | Must clear 40 px from top rule |
| `circle-right` | Circle bleeding off right edge | `secondary` | `top: 50%, right: −280 px`, size 620 px | Bleed is intentional; must clear top rule by 40 px |
| `bar-left` | Vertical bar bleeding off left edge | `warning` | `top: 200, left: 0`, 180 × 360 px | Must be behind text, not touching |
| `bar-right-small` | Small vertical bar | `warning` | `top: 50%, right: 0`, 6 × 260 px | Decorative hairline |
| `triangle-tr` | Small equilateral triangle | `warning` | `top: 136, right: 96`, 140 × 120 px | Must clear top rule by 80 px |
| `square-tr` | Small filled square | `ink` or `warning` | `top: 180, right: 96`, 140 × 140 px | Must clear top rule by 40 px |
| `dot-accent` | Small circle | `warning` | Pair with cover circle, 64 px diameter | Cover only |

### Shape composition principles

1. **Shapes bleed off the edge** when they're large. Never center a large shape inside the content area — it fights the text.
2. **Shapes sit opposite the text weight.** If the headline is bottom-left weighted, the shape goes top-right. If the headline is top, the shape goes bottom.
3. **Small accents pair with larger shapes.** A primary circle + a small warning dot reads as composed; two similarly-sized shapes read as cluttered.
4. **Shapes behind text, not beside text.** A bar on the left can bleed behind a headline if the headline is offset right enough to avoid the bar entering the text column.

### Cover composition (exception)

The cover slide has a fixed composition: `circle-center` in `primary` (360 px diameter) at y ≈ 330, with an optional `dot-accent` in `warning` (64 px) positioned at the upper-right edge of the circle. The title goes below, left-aligned, at `display` size.

---

## 9. Element-level rules

### Headlines

- Flush left always. No centering.
- Accent one or two phrases per headline in `primary` (or rarely `warning`/`complement`)
- Break lines manually in the source; do not rely on auto-wrap
- Must respect line-height contract above

### Body text

- Flush left
- Max column width: **vertical** = 620 px; **landscape** = 720 px (single column)
- Landscape body plates may use the **two-column** primitive: 720 + 96 + 720
  (a 96 px gutter), centered inside the 1664 px content area. Both columns
  share the same baseline grid; never mix one column at 720 with another at
  a different width.
- Title / cover plates remain single-column hero (1500 px in landscape, 820
  px in vertical) — never split a title across columns.
- Inline code is monospace 18 px in `primary` with a 1 px underline in `primary`
- Inline emphasis uses italic, not bold (bold is reserved for the headline)

### Captions

- Italic Inter, 14 px, `ink-dim`
- Flush left, max column width: **vertical** = 520 px; **landscape** = 600 px
- Appear below body, never above
- Functions as a museum plate caption — a second, quieter register

### Code blocks

- Font: JetBrains Mono 17 px, line-height 1.7
- Left edge marked by a 2 px vertical rule in `primary`
- Indented 24 px from the rule
- Max column width: **vertical** = 720 px; **landscape** = 1100 px
- Code stays single-column even in landscape — never split a code block
  across the two-column body grid
- Syntax colors:
  - Keys: `primary` bold
  - Strings: `code-string` (moss green)
  - Numbers: `complement` (ink — no mulberry in the palette)
  - Punctuation: `ink-dim`

### Lists (plate-list slides)

- Numbered `01 / 02 / 03` (not `1. / 2. / 3.`)
- Monospace 12 px number in `ink`, 28px gap to content
- Item name: JetBrains Mono 24 px bold in `primary`
- Item description: Inter 18 px regular in `ink`
- 1 px rule between each item, in `ink`

---

## 10. Safe zones (collision prevention)

These are hard minimums. Violating them produces the "AI slop" look — descenders kissing rules, shapes touching text, accents without breathing room.

| Contract | Minimum | Notes |
|---|---|---|
| `shape-to-rule` | 40 px | Any decorative shape to any hairline rule |
| `text-to-rule` | 24 px | Text descender bottom to any rule. Account for descender depth: `0.22 × font-size` |
| `shape-to-text` | 32 px | Edge-to-edge between a shape and a text block |
| `text-to-text` | 16 px | Between adjacent text blocks of the same group (headline → body) |
| `group-to-group` | 48 px | Between semantically distinct groups (caption → code block) |

### Descender math

For text sitting above a rule:

    clearance_needed = safe_text_to_rule + (font_size × 0.22)

Example — title at 104 px:

    clearance = 24 + (104 × 0.22) = 24 + 22.9 = 47 px from baseline to rule

---

## 11. Authoring markdown

See `carousel_example.md` for a working example. Minimum structure:

```markdown
---
title: My Essay Title
subtitle: A Short Essay
author: Josh
attribution: happycampr
plate_count: 5
---

# Slide 1 :: cover

headline: |
  My Essay,
  {{accent:a title}}.

# Slide 2 :: plate

tag: The Setup
headline: |
  The setup goes
  {{accent:here}}.
body: |
  Body text. Inline `code` works. Italics with _underscores_.
caption: |
  A quieter second register.
shape:
  kind: bar-left
  color: warning
```

---

## 12. Rendering pipeline

1. Parse YAML frontmatter → populates cover, running colophons, plate numbering
2. Split markdown on `# Slide N :: TYPE` headings
3. For each slide, the driver copies `design/assets/happycampr-logo-full.svg`
   into the slide's working directory as `happycampr-logo.svg`, then runs the
   implementation agent. The agent's HTML references the logo by filename only.
4. Apply the layout template matching TYPE
5. Validate against `happycampr.design-contracts.json`:
   - Check all text-to-rule clearances
   - Check all shape-to-rule clearances
   - Check line-height floors for multi-line titles
   - Verify the header logo is present, correctly positioned, and not recolored
   - For `plate-chart`: verify the Charts.css engine, the pinned stylesheet,
     series-palette colors, and the chart box inside the content area (§12a)
   - Fail the build on any violation
6. Render each slide to PDF at the carousel's active orientation
   (1080 × 1350 vertical, or 1920 × 1080 landscape — the renderer reads
   the orientation column on the carousel row)
7. Concatenate PDFs in slide order → final carousel

---

## 12a. Charts

The `plate-chart` slide type renders quantitative content as a chart. The
engine is **Charts.css** — a pure-CSS framework that styles a semantic HTML
`<table>` as a chart. No JavaScript, no canvas, no inline-SVG charts. See
`carousel_example.md` → "Reference — plate-chart" for canonical markup.

### When to reach for a chart

Use `plate-chart` when the point is the *shape of the numbers* — a trend, a
comparison, a breakdown, a before/after, a distribution — and prose would
just narrate a table. If the data does not fit one of the Charts.css
families below, **do not force a chart**: fall back to `plate-list` or a
plain styled table. A wrong chart is worse than no chart.

### Engine + delivery

- Stylesheet: a pinned CDN `<link>`, loaded exactly like Google Fonts (the
  renderer waits for `networkidle0`, so it is fully applied before the
  screenshot):
  `<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/charts.css@1.2.0/dist/charts.min.css">`
  This is the **only** non-font external resource permitted. Pinned to
  `@1.2.0` so renders are reproducible — never the floating `charts.css`
  tag, never any other CDN, never external JS.
- The chart is a `<table class="charts-css …">` with `<thead>` (column
  headers) and `<tbody>` (one `<tr>` per category, one `<td>` per series).
  Data magnitude is the inline `--size` custom property, normalized to 0–1.

### Families (the whole menu)

| Family | Class | Use |
|---|---|---|
| Column | `charts-css column` | Vertical bars; categories over time / few groups |
| Bar | `charts-css bar` | Horizontal bars; ranked lists, long category names |
| Line | `charts-css line` | Trend across an ordered axis |
| Area | `charts-css area` | Trend with magnitude / volume emphasis |
| Pie | `charts-css pie` | Parts of a whole, ≤ 5 slices |

Multi-series adds the `multiple` class and one `<td>` per series per row.
Line/area cells use `--start` + `--size` (start = running baseline, size =
this segment). Normalization: `--size = value / axisMax`. The author may
set `axis.max`; otherwise pick a clean max ≥ the largest value and render
matching ticks. Charts.css families are the whole menu — scatter, slope,
waterfall, bubble, etc. are out of scope; fall back rather than fake them.

### Plot box (reuse the content area — do not invent coordinates)

The chart `<table>` is a content block. Its bounding box must sit inside
the active orientation's `grid.contentArea`
(`carousel.manifest.json#orientations.{vertical|landscape}.grid.contentArea`).
That content area is already inset 40 px from the top and bottom rules, so
`shape-to-rule` is satisfied automatically — no extra math.

- No headline: chart box = the full content area.
- With a headline: headline sits at the top of the content area at
  `title-sm`; the chart box starts ≥ 48 px (`group-to-group`) below the
  headline's last line and ends at or above `contentArea.yEnd`.
- Every chart-box edge y-coordinate snaps to the 8 px grid.

### Brand theming — override every Charts.css default

Charts.css ships its own palette, spacing, and borders. None of them are
on-brand. Set series color via the `--color-1` … `--color-N` dataset
variables on the chart (ID selector beats the framework's `.charts-css`
class); these also keep bars consistent. Charts.css needs an **explicit
table height** (`height: 600px`) — `height: 100%` does not propagate.
Override all of:

| Element | Rule |
|---|---|
| Series fill | `--color-1` … `--color-N` dataset variables, from the series palette below, in order |
| Axis + grid lines | `ink` (`#2B1810`) or `ink-dim` (`#7C6F66`), ≤ 1 px; prefer few or no gridlines |
| Axis tick + category labels | Inter, `caption` (14 px) or `body-sm` (18 px) token, `ink`/`ink-dim`. **14 px hard floor** |
| Data value labels | Same as tick labels; prefer direct labels over a key |
| Series key (if needed) | Plain brand HTML (swatch + label) — **not** the Charts.css `legend` component (hard to brand reliably) |
| Chart `<caption>` | Suppress (`caption { display: none }`) — the slide headline is the title — or restyle to the `caption` token |
| Spacing | `--spacing` / `data-spacing-*` resolved to a multiple of 8 px |
| Table font | Inter (Charts.css inherits; set it explicitly) |

#### Series palette (ordered)

Charts.css multi-series needs more than two hues, so the chart **plot area
is the one documented exception** to the two-chromatic-colors rule (§7).
Everything else on the slide — and all axis/label text — still obeys §7 and
stays `ink`/`ink-dim`.

| Order | Token | Hex |
|---|---|---|
| 1 | `primary` | `#946334` |
| 2 | `warning` | `#946334` |
| 3 | `secondary` | `#5A6B4C` |
| 4 | `complement` | `#2B1810` |

**Known happycampr limitation:** the four-colour palette has only two
chromatic colours, so slots 1 and 2 are the *same* colour (graham) and slot
4 is ink. A chart with >2 series cannot be fully colour-distinct on-brand —
prefer ≤ 2 series, or distinguish extra series by pattern/direct label rather
than forcing a hue. See `ambiguous.md` §13.2. Use only as many as there are
series. ≤ 6 series per chart; beyond that, split the slide or summarize.

### Forbidden (static PDF)

The output is a single screenshot. Anything time- or pointer-dependent is
meaningless or captures mid-frame. Do **not** use Charts.css 3D effects,
motion / animation, tooltips, or hover-only data (`show-data-on-hover`).
All data must be statically visible.

---

## 13. Export

One PDF file. Each slide is one page. No page numbers in the PDF itself (the folio tokens handle that visually). Filename convention: `{slug}-v{N}.pdf`.

---

## Quick reference: slide type templates

### Cover
- Large `circle-center` shape, optional `dot-accent`
- `display` title, left-aligned, at bottom of slide
- Tag: `A Short Essay` or custom
- Colophon right: `Seven Plates` or total

### Plate
- Optional shape (one of six kinds)
- `title` headline, 1–4 lines
- `body` text, 1–3 short paragraphs
- Optional `caption` italic below body

### Plate-code
- Same as plate, but body is replaced or followed by a code block
- Code block has the left-rule treatment

### Plate-list
- `title` headline
- Three numbered items in gallery-plaque style
- 1 px rules between items

### Plate-chart
- Charts.css `<table>` chart (column / bar / line / area / pie)
- Optional `title-sm` headline above the chart
- Series colored from the ordered brand palette; axis/labels in ink
- See §12a and `carousel_example.md` → "Reference — plate-chart"

### Takeaway
- May invert to dark surface
- `title` headline with emphasis on two phrases
- Short body
- Colophon right: `Fin.`
