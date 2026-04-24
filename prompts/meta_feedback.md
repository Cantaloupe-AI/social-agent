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

