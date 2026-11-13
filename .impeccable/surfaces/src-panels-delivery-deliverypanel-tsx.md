---
version: 1
slug: "src-panels-delivery-deliverypanel-tsx"
primary_target: "src/panels/delivery/DeliveryPanel.tsx"
related_targets: ["src/App.css"]
---

# Tema Tinto, sobre Delivery

Scope: a new theme, `tinto`, added beside the existing ones (dark, purple, plum, light); the first surface it must hold is the Delivery panel. Visitor mode: Operate.

Audience and job: any developer who works with coding agents, in daylight, with the window beside their editor. They come back to see what moved and to answer only what needs them.

Constraints: Spanish interface; square corners in every control and container (a brand commitment), the stations of the line being the only round marks; code, diffs and terminals must stay easy to read; no coffee-shop decoration; it must still read as a serious tool; the other themes keep working unchanged.

Chosen by the user on 2026-11-13 from three sketches (`.impeccable/mocks/decision/assigned.html` is the one picked).

Fonts: Overpass and Overpass Mono ship with the app (src/assets/fonts, SIL OFL), approved by the user on 2026-11-13.

Decided with the user after he saw the build (2026-11-13): no dashed segments; square corners restated as binding; the history reads by rounds; a stage under the pointer lights its station and its words.

## Direction contract

THESIS: Each task is a line and its stages are stations, so where it is and where it stopped is read at a glance. It refuses the category default: a grey editor clone with one accent colour.

OWN-WORLD: Plan paper `#f5f5f1` ground, white surfaces, hairlines `#dcdad3`; a coffee-black ink `#241a17` for text and for the solid window bar. Two flat line colours, Agentes `#1b5e8c` and Entrega `#1f7a5a`. Amber `#9a5c00` only for what waits on the user, red `#b3261e` only for what broke. Overpass and Overpass Mono. Dense rows, thin rules, borderless secondary controls.

STORY: The user returns, reads the queue, follows a line to the station where a task stopped, answers, and leaves knowing the rest goes on alone.

FIRST VIEWPORT: Ink window bar; Delivery header with one primary button; the batch line; a left column of about 280px with search, the "Te necesita" queue and the tasks, each ending in a small line of its progress; on the right the task: title, what is the user's, "Ahora", then the stages drawn as two connected lines with one station per stage, the actions under them, and the folded sections.

FORM: a transit line diagram, position 5 of 7 on the ordered list; seed:b3be64ac6c4885f2. Signature: the stage strip as a line diagram. The rail is always solid: in its line colour only between two passed stations, grey otherwise. The station's form carries the state: filled disc passed, ring with a dot running, hollow grey ring pending, hollow ring in the line colour repeats this round, amber ring yours, red square stop, a warning triangle for a problem nobody has to answer here, a small tick for a stage that was skipped. The two groups stack as two rows when their cells would be narrow. Raises kept: state said by form and not only colour; one colour, one job; density kept; only the selected row shows secondary data; times in a marginal column of tabular figures.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
