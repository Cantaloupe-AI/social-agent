# happycampr design tokens — ambiguous / unported / compromised values

§1–§12 are ported from `mimir/design/ambiguous.md` (the brand-bible → DTCG port).
§13 is repo-specific: it records every place the four-colour happycampr palette
could not fill a role the **carousel scaffold** required, and the
decision taken. The scaffold (spacing, per-orientation layout, canvas-physics
type scale + line-height/tracking floors) was deliberately preserved — those are
LinkedIn-canvas constraints the contracts enforce, not brand identity.

---

## 1. Responsive font sizes (`clamp()`)

`typography.brand.{display,h1,h2,h3,body-lg}` originals were `clamp()` ranges
(display `clamp(72px,11vw,168px)`, h1 `clamp(56px,7vw,96px)`, h2
`clamp(40px,4.5vw,64px)`, h3 `clamp(26px,2.4vw,34px)`, body-lg
`clamp(20px,1.6vw,26px)`). DTCG `dimension` only accepts px/rem — no
`clamp()`/`vw`. The clamp minimum is the fixed `fontSize`; the max is in each
token's `$description`.

## 2. `em`-based letter-spacing

All `typography.brand.*` originals used `em` letter-spacing
(display −0.055em, h1 −0.05em, h2 −0.045em, h3 −0.035em, body-lg −0.02em,
body −0.012em, small −0.01em, eyebrow 0.16em). `em` scales with font size; a
px value does not. The px values stored are `em × minimum fontSize` and are
accurate only at that size.

## 3. `text-transform: uppercase`

`typography.brand.eyebrow` (and the colophon/folio/tag eyebrow uses in the
carousel) render uppercase. DTCG has no `textTransform`. Convention: any token
whose `$description` says "uppercase" must have `text-transform: uppercase`
applied by the consumer.

## 4. `ch` max-width values

Body measure caps (`body`/`body-lg` 70ch, plus brand-page line caps 18/22/28/42ch)
are font-relative; not DTCG `dimension`. Enforce in component/layout CSS. The
carousel uses fixed px column widths instead (`layout.column.*`).

## 5. `%` border-radius (circle chips)

`border-radius: 50%` circular chips can't be a DTCG dimension. Treat 50% as a
component convention. `border-radius.pill` (999px) covers all other rounding.

## 6. Opacity as a modifier

Element-level opacity (eyebrow 0.7, marshmallow watermark 0.10, section number
0.5, …) is not a DTCG token type. Where the intent is muted text, use
`color.ui.burnt-60`; whole-element opacity is a component concern.

## 7. `font-feature-settings`

`font-feature-settings: 'ss01','cv11','tnum' off` is not in the DTCG typography
composite. Apply as a global/per-component CSS rule.

## 8. Logo construction rules (non-numeric)

Clear space ≥ 0.5× cap height all sides · marshmallow mark tilted 8°
counter-clockwise (fixed, never adjusted) · at 72px lockup: 4px body stroke /
3.2px smile stroke. Not DTCG-typed — preserved here and honoured by the shipped
asset. `sizing.logo.min-width` (120px) is the one numeric constraint that ports.

## 9. Viewport/percentage spacing

Page horizontal padding `clamp(40px,8vw,140px)`, logo-card padding
`clamp(40px,6vw,80px)`, vp-card padding `clamp(32px,4vw,56px)`, cover title-wrap
top `8vh` — no `vw/vh/clamp()` in DTCG. Use the min as a fixed fallback.

## 10. WCAG contrast (design constraints, not tokens)

| Pairing | Ratio | Body 4.5:1 | Large/UI 3:1 |
|---|---|---|---|
| Burnt on Marshmallow | 15.0:1 | AAA | AAA |
| Marshmallow on Burnt | 15.0:1 | AAA | AAA |
| Marshmallow on Moss | 5.1:1 | AA | AAA |
| Marshmallow on Graham | 5.4:1 | AA | AAA |
| Moss on Marshmallow | 5.1:1 | AA | AAA |
| Graham on Marshmallow | 4.6:1 | AA | AAA |
| Burnt on Graham | 3.3:1 | Large/UI only | AA |
| Burnt on Moss | 2.9:1 | Fail | Borderline |
| Graham on Moss | 1.8:1 | Fail | Fail |

**Hard rule:** never Burnt-on-Moss or Graham-on-Moss for text at any size.

## 11. Spec vs semantic line-height

The brand bible's specimen line-heights differ from semantic ones (h2 1.08 vs
1.15, h3 1.2 vs 1.3, display 0.92 vs 1.0, h1 1.0 vs 1.05). `typography.brand.*`
uses the **semantic** values as canonical.

## 12. `font-variant-numeric: tabular-nums`

Applied to numeric tables/grades/folios. Not a DTCG type — apply as a CSS rule
in components showing tabular numbers.

---

## 13. happycampr palette → carousel scaffold mapping (repo-specific)

The carousel system was built for a role-based palette (6+ chromatic
roles + code colours + a 4-colour chart series palette) plus a JetBrains-Mono
code face. happycampr is **four flat colours, Inter-only**. Per the agreed
approach ("derive + keep scaffold"), the scaffold is unchanged and the palette
is mapped onto the roles. Every collapse is below. `happycampr.design-contracts.json`,
`carousel.manifest.json`, and `carousel-manifest.md` hold the
collision-prevention rules; their colour/logo values now resolve here.

### 13.1 Colour role mapping

| Scaffold role | Prior value | happycampr now | Hex | Note |
|---|---|---|---|---|
| `surface` | bone | marshmallow | `#F5F1E8` | Replaces white everywhere. |
| `surface-inverse` | near-black forest | burnt | `#2B1810` | Takeaway slide. |
| `ink` | forest-tinted black | burnt | `#2B1810` | Text + rules. |
| `ink-dim` | `#525B55` solid | burnt @0.60 over marshmallow, **flattened to a solid** | `#7C6F66` | Carousel PDFs always sit on marshmallow, so the alpha is pre-flattened to give the renderer a real colour. |
| `primary` | forest green | graham | `#946334` | Brand anchor / headline accent / shape & code-key fills. |
| `secondary` | mint | moss | `#5A6B4C` | Decorative fills, success. |
| `warning` | amber `#FBA100` | **graham** | `#946334` | happycampr has **no amber**. Shape-fill warning and `warning-text` both collapse to graham; graham-on-marshmallow is AA body / AAA large (4.6:1) so it is safe for text too. |
| `complement` | mulberry `#854272` | **burnt/ink** | `#2B1810` | happycampr has **no mulberry**. The complement role (code numbers, rare emphasis) collapses to ink emphasis. |
| `code-string` | deep amber `#8D5405` | moss | `#5A6B4C` | Greens read well for strings against burnt/ink keys. |

`maxChromaticPerSlide: 2` is unchanged, but note happycampr has only **two
truly chromatic colours** (graham, moss) — burnt/marshmallow/ink-dim are
non-chromatic. Honour the §10 rule: never Burnt-on-Moss or Graham-on-Moss for
text.

### 13.2 Chart series palette (known limitation)

`chart.seriesPalette` keeps its four ordered slots
`[primary, warning, secondary, complement]`, which now resolve to
`[graham, graham, moss, burnt]`. Slots 1 and 2 are **the same colour** (graham).
A multi-series chart with >2 series therefore cannot be fully colour-distinct
on-brand. Guidance: prefer ≤2 series; for 3–4 series fall back to `plate-list`
or distinguish series by pattern/label rather than forcing a 4th hue. This is a
deliberate brand-imposed limitation, not a renderer bug.

### 13.3 Mono typeface (functional exception)

happycampr is Inter-only. Code blocks and `01 / 02 / 03` list folios still use
**JetBrains Mono** (`font.family.mono`, `fonts.allowed` in contracts). This is a
carousel-functional exception, explicitly *not* a brand font. If the brand later
forbids any non-Inter face, code blocks must switch to an Inter tabular
treatment and this note is the trigger to revisit.

### 13.4 Heading weight 800 → 700

The carousel scaffold set `title-sm`/`title`/`display` at weight **800**.
happycampr caps at **700** (bold). All three drop to 700 across
`design-tokens.json`, `carousel.manifest.json`, `carousel-manifest.md`, and the
example. Line-height/tracking floors are unchanged (physics).

### 13.5 Header logo

The legacy full-lockup green logo (native 160:29) is removed.
The header now uses `design/assets/happycampr-logo-full.svg` — the vector
lockup (marshmallow mark + "happycampr" wordmark), native viewBox 1080×220
(**aspect 54:11**). At the fixed 24px header height the rendered width is
24 × 1080/220 ≈ **118px** (was 132). The colour-shift reject rule still applies:
the burnt/marshmallow lockup is canonical — do not recolour, invert, or trace.
Standalone marshmallow mark (graham) ships at full res and at 120×125 for the
sub-120px case.

### 13.6 Source of assets

The marshmallow mark + wordmark were **not present in the Notion workspace**
the integration can see (only legacy 2022 logos). They were sourced
from `~/Downloads` (`happycampr wordmark burnt.svg` → the full lockup;
`happycampr icon mark graham.png` + the 120×125 variant → the standalone mark).
If a different canonical asset exists, replace the files in `design/assets/`
keeping the same filenames.
