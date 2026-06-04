---
title: Manifests for Agents
subtitle: A Short Essay
author: Josh
attribution: happycampr
slug: manifests-for-agents
plate_count: 5
---

# Slide 1 :: cover

tag: A Short Essay
folio: I / V
colophon_left: Josh · happycampr
colophon_right: Five Plates
headline: |
  Manifests
  for {{accent:agents}}.
shapes:
  - kind: circle-center
    color: primary
    diameter: 360
    y: 330
  - kind: dot-accent
    color: warning
    diameter: 64
    anchor: upper-right-of-circle

# Slide 2 :: plate

tag: Plate II · The Problem
folio: II / V
headline: |
  Prose is a
  {{accent:lossy}} format.
body: |
  An essay read by a human is a conversation. An essay read by an agent
  is a parse. When the target audience is both, the document needs two
  registers: one for the eye, one for the parser.
caption: |
  Ambiguity is the cost of elegance when the reader is a machine.
shape:
  kind: bar-left
  color: warning
  top: 200
  width: 180
  height: 360

# Slide 3 :: plate-code

tag: Plate III · The Shape
folio: III / V
headline: |
  A manifest is
  {{accent:a contract}}.
body: |
  The numbers are not decoration. They are the interface an agent
  reads when a sentence would be too slow.
code: |
  {
    "canvas":  { "width": 1080, "height": 1350 },
    "surface": "#F5F1E8",
    "title":   { "size": 104, "lineHeight": 1.05 }
  }

# Slide 4 :: plate-list

tag: Plate IV · The Registers
folio: IV / V
headline: |
  Three layers
  of {{accent:signal}}.
items:
  - name: PROSE
    desc: The essay a human reads. Where ideas earn their weight.
  - name: MANIFEST
    desc: The numbers an agent reads. Where the ideas become rules.
  - name: CONTRACTS
    desc: The guardrails that refuse to let either one drift.

# Slide 5 :: takeaway

tag: Plate V · The Takeaway
folio: V / V
surface: inverse
colophon_right: Fin.
headline: |
  Write for
  {{accent:humans}}.
  Ship for
  {{accent:agents}}.
body: |
  One document, two readers, the same truth. That is a manifest.

---

## Reference — plate-chart (Charts.css)

Standalone reference for the `plate-chart` type — **not** part of the
five-slide essay above. See manifest §12a for the full rules. The chart is
a Charts.css `<table>`; the framework owns the geometry, the spec owns the
brand.

### Authoring markdown (what the author writes in `source.md`)

```markdown
# Slide N :: plate-chart

folio: VI / VI
headline: |
  Manifests cut
  {{accent:rework}}.
chart:
  family: column          # column | bar | line | area | pie
  axis:
    x: Slide type
    y: Iterations to accept
    max: 4                 # optional; else a clean max >= largest value
  series:
    - name: Prose brief
      color: secondary     # series palette, in order (two distinct chromatics)
      data: { Cover: 3, Plate: 4, List: 4 }
    - name: Manifest brief
      color: primary
      data: { Cover: 1, Plate: 2, List: 1 }
caption: |
  Fewer round-trips when the brief is a contract, not an essay.
```

### Expected HTML output (vertical, 1080 × 1350)

`--size = value / axis.max` (max = 4 → 1→0.25, 2→0.50, 3→0.75, 4→1.00).
Two series → two **distinct** chromatic tokens: series 1 `secondary` (moss), series 2 `primary` (graham). happycampr has only two chromatic colours, so a 2-series chart must use these two — not `primary`+`warning`, which both resolve to graham (see manifest §12a / `ambiguous.md` §13.2).
Every Charts.css default is overridden to brand tokens. **Build the series
key from plain brand HTML — do not use the Charts.css `legend` component;
it is hard to brand reliably.** This HTML was rendered through the real
pipeline (`scripts/render-slide.ts`) and visually verified.

```html
<!doctype html>
<html>
<head>
<meta charset="utf-8" />
<link rel="stylesheet"
      href="https://fonts.googleapis.com/css2?family=Inter:wght@400;600;700&display=swap" />
<link rel="stylesheet"
      href="https://cdn.jsdelivr.net/npm/charts.css@1.2.0/dist/charts.min.css" />
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body {
    width: 1080px; height: 1350px; overflow: hidden;
    background: #F5F1E8; position: relative;
    font-family: "Inter", sans-serif; color: #2B1810;
  }
  .logo  { position: absolute; top: 24px; left: 96px; height: 24px; width: 118px; }
  .folio { position: absolute; top: 34px; right: 96px;
           font-size: 10px; font-weight: 600; letter-spacing: .38em;
           text-transform: uppercase; }
  .rule-top    { position: absolute; left: 96px; top: 56px;   width: 888px; height: 1px; background: #2B1810; }
  .rule-bottom { position: absolute; left: 96px; top: 1294px; width: 888px; height: 1px; background: #2B1810; }
  .colophon { position: absolute; top: 1316px; font-size: 10px; font-weight: 600;
              letter-spacing: .38em; text-transform: uppercase; }
  .colophon.left  { left: 96px; }
  .colophon.right { right: 96px; }

  .headline { position: absolute; left: 96px; top: 160px; width: 820px;
              font-size: 84px; line-height: 1.05; font-weight: 700;
              letter-spacing: -0.035em; }
  .headline .accent { color: #946334; }

  /* Chart box: inside contentArea (x96–984, y96–1254), >=48px below the
     headline, every edge on the 8px grid. Charts.css needs an explicit
     table height — height:100% does not propagate reliably. */
  .chartbox { position: absolute; left: 96px; top: 480px; width: 888px; height: 600px; }

  /* --- Brand overrides for Charts.css ---
     Charts.css colors dataset bars from the --color-1..N dataset palette.
     Override those (ID beats the framework's .charts-css class). The series
     key below is plain brand HTML, not the Charts.css legend component. */
  #chart {
    --color-1: #5A6B4C;             /* series 1 — Prose brief — secondary (moss) */
    --color-2: #946334;             /* series 2 — Manifest brief — primary (graham) */
    height: 600px;
    color: #2B1810;                 /* axis line + ticks inherit currentColor */
    font-family: "Inter", sans-serif;
  }
  #chart caption { display: none; } /* the slide headline is the title */
  #chart .data { font-size: 14px; color: #2B1810; }
  #chart th[scope="col"] { font-size: 14px; }
  #chart th[scope="row"] { font-size: 14px; font-weight: 600; color: #2B1810; }

  /* Series key — plain brand HTML, not the Charts.css legend component. */
  .serieskey { position: absolute; left: 96px; top: 1112px;
               display: flex; gap: 32px; font-size: 14px; color: #7C6F66; }
  .serieskey .item { display: flex; align-items: center; gap: 8px; }
  .serieskey .sw { width: 14px; height: 14px; display: inline-block; }

  .caption { position: absolute; left: 96px; top: 1160px; width: 520px;
             font-size: 14px; font-style: italic; color: #7C6F66; }
</style>
</head>
<body>
  <img class="logo" src="happycampr-logo.svg" alt="happycampr" />
  <div class="folio">VI / VI</div>
  <div class="rule-top"></div>

  <h1 class="headline">Manifests cut<br /><span class="accent">rework</span>.</h1>

  <div class="chartbox">
    <table id="chart"
           class="charts-css column multiple show-labels show-primary-axis data-spacing-20">
      <caption>Iterations to accept</caption>
      <thead>
        <tr>
          <th scope="col">Slide type</th>
          <th scope="col">Prose brief</th>
          <th scope="col">Manifest brief</th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <th scope="row">Cover</th>
          <td style="--size: 0.75"><span class="data">3</span></td>
          <td style="--size: 0.25"><span class="data">1</span></td>
        </tr>
        <tr>
          <th scope="row">Plate</th>
          <td style="--size: 1.00"><span class="data">4</span></td>
          <td style="--size: 0.50"><span class="data">2</span></td>
        </tr>
        <tr>
          <th scope="row">List</th>
          <td style="--size: 1.00"><span class="data">4</span></td>
          <td style="--size: 0.25"><span class="data">1</span></td>
        </tr>
      </tbody>
    </table>
  </div>

  <div class="serieskey">
    <span class="item"><span class="sw" style="background:#5A6B4C"></span>Prose brief</span>
    <span class="item"><span class="sw" style="background:#946334"></span>Manifest brief</span>
  </div>

  <p class="caption">Fewer round-trips when the brief is a contract, not an essay.</p>

  <div class="rule-bottom"></div>
  <div class="colophon left">Manifests for Agents</div>
  <div class="colophon right">Josh · happycampr</div>
</body>
</html>
```

Notes: the chart box (`top: 480px`, `height: 600px` → bottom 1080, ≤ 1254)
sits inside the content area and clears the headline by ≥ 48 px; every edge
is a multiple of 8. Charts.css needs an explicit table height
(`#chart { height: 600px }`) — `height: 100%` does not propagate. Series
colors are the ordered palette via the `--color-1..N` dataset variables
(`warning`, `primary`); the plot-area chromatic exception (§7 / contracts
`#chart.plotAreaChromaticException`) is why two chromatic series are
allowed. Axis line and all labels are `ink`/`ink-dim` Inter ≥ 14 px. No
Charts.css 3D, animation, tooltip, or hover-only data.
