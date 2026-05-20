---
title: Charts Kitchen Sink
slug: charts-kitchen-sink
---

<!-- One plate-chart per Charts.css family. happycampr is a two-
chromatic palette (graham + moss), so each demo stays <= 2 series.
Edit the data freely, then: cantalog-cli generate <carousel_id>. -->

# Slide 1 :: plate-chart

folio: I / V
headline: |
  Signups keep
  {{accent:climbing}}.
chart:
  family: column
  axis:
    x: Quarter
    y: Signups
    max: 1200
  series:
    - name: 2025
      color: primary
      data: { Q1: 420, Q2: 610, Q3: 780, Q4: 1120 }
    - name: 2026
      color: secondary
      data: { Q1: 540, Q2: 700, Q3: 910, Q4: 1180 }
caption: |
  Two years, same four quarters.


# Slide 2 :: plate-chart

folio: II / V
headline: |
  Where signups
  {{accent:come from}}.
chart:
  family: bar
  axis:
    x: Channel
    y: Signups
    max: 900
  series:
    - name: Signups
      color: primary
      data: { Referral: 860, Search: 540, Social: 410, Direct: 230, Email: 120 }
caption: |
  Ranked, highest first.


# Slide 3 :: plate-chart

folio: III / V
headline: |
  Retention is
  {{accent:holding}}.
chart:
  family: line
  axis:
    x: Week
    y: Active %
    max: 100
  series:
    - name: Cohort A
      color: primary
      data: { W1: 100, W2: 82, W3: 71, W4: 65, W5: 62 }
    - name: Cohort B
      color: secondary
      data: { W1: 100, W2: 88, W3: 80, W4: 76, W5: 74 }
caption: |
  Percent still active by week.


# Slide 4 :: plate-chart

folio: IV / V
headline: |
  Revenue,
  {{accent:cumulative}}.
chart:
  family: area
  axis:
    x: Month
    y: $K
    max: 480
  series:
    - name: 2026
      color: primary
      data: { Jan: 40, Feb: 95, Mar: 165, Apr: 250, May: 350, Jun: 470 }
caption: |
  Running total through June.


# Slide 5 :: plate-chart

folio: V / V
headline: |
  Traffic
  {{accent:mix}}.
chart:
  family: pie
  series:
    - name: Share
      color: primary
      data: { Referral: 46, Search: 28, Social: 18, Direct: 8 }
caption: |
  Share of sessions, last 30 days.

