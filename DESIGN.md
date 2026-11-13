---
name: Tinto
description: A light theme drawn as a transit line diagram, for following coding agents without babysitting them.
colors:
  ink: "#241a17"
  on-ink: "#fbfaf7"
  plan-paper: "#f5f5f1"
  surface: "#ffffff"
  sunken: "#fafaf7"
  strip: "#ecebe5"
  hairline: "#dcdad3"
  hairline-strong: "#c4c0b7"
  text: "#3a2f2b"
  text-soft: "#4f4541"
  text-muted: "#6b625e"
  text-subtle: "#736a65"
  line-agentes: "#1b5e8c"
  agentes-text: "#174f77"
  line-entrega: "#1f7a5a"
  entrega-text: "#1a6a4e"
  entrega-pale: "#a6d8c3"
  amber-yours: "#9a5c00"
  amber-text: "#7d4a00"
  red-broke: "#b3261e"
  red-text: "#a3221b"
  red-pale: "#e8a19b"
  bar-raised: "#3a2f2b"
  bar-hairline: "#5a4e48"
  bar-text-muted: "#cfc6c0"
typography:
  title:
    fontFamily: "Overpass, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 700
  headline:
    fontFamily: "Overpass, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
  body:
    fontFamily: "Overpass, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
  body-strong:
    fontFamily: "Overpass, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 600
  small:
    fontFamily: "Overpass, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
  line-name:
    fontFamily: "Overpass, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    letterSpacing: "0"
  label:
    fontFamily: "Overpass, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 600
    letterSpacing: "0.05em"
  hint:
    fontFamily: "Overpass, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: "16px"
  mono:
    fontFamily: "Overpass Mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
    fontSize: "12px"
    fontWeight: 400
rounded:
  none: "0"
  mark: "50%"
spacing:
  hair: "2px"
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "12px"
  xl: "16px"
  section: "14px"
components:
  window-bar:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.plan-paper}"
  dock-tab-active:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
  button:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "4px 10px"
  button-primary:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.on-ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "4px 10px"
  button-quiet:
    backgroundColor: "transparent"
    textColor: "{colors.text-muted}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "4px 10px"
  button-quiet-hover:
    textColor: "{colors.text}"
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    typography: "{typography.small}"
    rounded: "{rounded.none}"
    padding: "4px 8px"
  task-row:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    rounded: "{rounded.none}"
    padding: "7px 12px 8px"
  link-chip:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    typography: "{typography.body-strong}"
    rounded: "{rounded.none}"
    height: "26px"
    padding: "0 7px"
  queue-row:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    padding: "6px 12px 7px"
  station:
    backgroundColor: "{colors.line-agentes}"
    rounded: "{rounded.mark}"
    size: "14px"
  station-pending:
    backgroundColor: "{colors.plan-paper}"
    rounded: "{rounded.mark}"
    size: "14px"
  station-broke:
    backgroundColor: "{colors.red-broke}"
    rounded: "{rounded.none}"
    size: "12px"
  station-hint:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.plan-paper}"
    typography: "{typography.hint}"
    rounded: "{rounded.none}"
    padding: "2px 7px"
---

# Design System: Tinto

## Overview

**Creative North Star: "Línea Tinto"**

Each task is a line and its stages are stations, so where it is and where it stopped is read at a glance. The theme is a transit plan printed on paper: a warm off-white ground, white surfaces, thin warm hairlines, and one coffee-black ink that writes the text, fills the primary button and makes the window bar a solid band. Two flat line colours carry the diagram, blue for Agentes and green for Entrega, and nothing else on the screen is allowed to be decorative colour.

It is a working tool beside an editor, in daylight, read by someone coming back to see what moved. Rows are dense, rules are thin, secondary controls have no box, and state is said by the form of a mark before it is said by its colour. The coffee idea of the name lives in the ink and the warm neutrals only; there is no coffee-shop decoration.

This file records the Tinto theme, the system this build created, as it ships in the Delivery panel. Four older themes (dark, purple, plum, light) remain in the app and keep their own look; they read the same token names (`--bg`, `--fg`, `--accent`, `--warn`, `--danger` and the rest) with their own values, so a rule written against tokens holds in all five. Only Tinto defines the line colours, the ink window bar and the bundled typefaces.

**Key Characteristics:**
- A transit line diagram as the signature: stages are stations on a solid rail.
- One ink for text, primary action, selection edge, focus ring and window bar.
- One colour, one job: blue informs and links, green runs and is done, amber is the user's, red broke.
- Square corners everywhere; the only round marks are stations and small status dots.
- Dense rows, hairlines instead of boxes, borderless secondary controls.
- Spanish interface, set in Overpass and Overpass Mono shipped with the app.

## Colors

Warm paper neutrals tinted from a single coffee-black ink, two flat line colours, and two signals held in reserve.

### Primary
- **Coffee Ink** (`ink`): body headings and strong text, the primary button, the selected row's left edge, the focus ring, the checked checkbox, the active dock tab's top line, the station hint, and the whole window bar. Text on it is **Paper on Ink** (`on-ink`).

### Secondary
- **Agentes Blue** (`line-agentes`): the Agentes line and its stations, every link in the panel, information marks (the Jira link's icon, the QA-in-progress badge and row tint), the text caret, and the text selection as a 22% wash. **Agentes Blue Text** (`agentes-text`) is its darker step for running text.

### Tertiary
- **Entrega Green** (`line-entrega`): the Entrega line and its stations, what is running and what is done (the live dot, the used capacity slots, "Entregada"). **Entrega Green Text** (`entrega-text`) for text; **Entrega Pale** (`entrega-pale`) for the border of a verified mark.
- **Amber, Yours** (`amber-yours`): only what waits on the user. The "Te necesita" heading mark, the thick ring of a station that is the user's, the bar of a pending decision or step, the count of QA problems awaiting an answer. **Amber Text** (`amber-text`) for text on paper.
- **Red, Broke** (`red-broke`): only what broke. The square stop station, the title and thin edge of a broken stage, an error line, a destructive menu item. **Red Text** (`red-text`) for text; **Red Pale** (`red-pale`) for soft edges.

### Neutral
- **Plan Paper** (`plan-paper`): the ground of every panel, and the fill of hollow stations.
- **White Surface** (`surface`): inputs, default buttons, menus, cards that open on top, the active dock tab.
- **Sunken Paper** (`sunken`) and **Strip** (`strip`): recessed areas and the quiet dock strip behind tabs.
- **Hairline** (`hairline`) and **Strong Hairline** (`hairline-strong`): 1px dividers and control borders; the stronger one separates dock groups.
- **Text** (`text`), **Soft** (`text-soft`), **Muted** (`text-muted`), **Subtle** (`text-subtle`): the reading ramp. Muted is the default for secondary lines, pending stations and quiet controls; every step keeps 4.5:1 on the paper and on white.
- **Rail Grey**: the undone rail is Muted at 50% over the paper, not a token of its own.
- **Bar Raised** (`bar-raised`), **Bar Hairline** (`bar-hairline`), **Bar Muted Text** (`bar-text-muted`): the ink-side values used only inside the window bar.
- Pointer feedback is the ink at low alpha: 5% hover, 8% strong hover, 12% pressed or selected.

### Named Rules
**The Amber Is Yours Rule.** Amber marks only what waits on the user. A warning nobody has to answer, a blocked task waiting on something else, a note: all neutral.

**The Red Broke Rule.** Red marks only what broke. Not emphasis, not a count, not a destructive-looking decoration.

**The One Colour, One Job Rule.** Blue is the Agentes line, information and links. Green is the Entrega line, running and done. Ink is action, selection and focus. A colour never takes a second job, which is why the selected row's edge is ink and not green.

## Typography

**Display Font:** none; the largest text is a 15px task title.
**Body Font:** Overpass (with Segoe UI Variable, Segoe UI, system-ui)
**Label/Mono Font:** Overpass Mono (with ui-monospace, SFMono-Regular, Menlo, Consolas)

**Character:** A signage face and its monospace sibling: open, even, built to be read small and fast. Both ship inside the app (`src/assets/fonts`, SIL OFL), weights 400 to 700 for the sans and 400 to 600 for the mono, so the theme never waits on a network. Only Tinto names them, through `--font-ui` and `--mono`; the other themes fall back to the platform stack.

### Hierarchy
- **Title** (700, 15px): the open task's title, two lines at most; the part after the key is regular weight and muted.
- **Headline** (600, 14px): the panel's name in its header.
- **Body** (400, 13px): everything by default. **Body Strong** (600, 13px) for a row's key, a station's name, a link chip.
- **Small** (400, 12px): second lines of rows, station details, metadata, gists of folded sections, the search field.
- **Line Name** (600, 12px, sentence case, no tracking, in the line's own colour): "Agentes" and "Entrega" above their rails.
- **Label** (600, 11px, 0.05em, uppercase, muted): the heading of a list or of a folded section ("En curso", "Pull request", "Historial"). A heading of real content below it, never a tag above a title.
- **Hint** (400, 11px on a 16px line): the ink tooltip above a station.
- **Mono** (400, 12px): branches, commits, paths, code and logs.

### Named Rules
**The Tabular Time Rule.** Elapsed times, costs and counts use tabular figures and sit at the right edge of their row, so a ticking number never shifts the line.

**The Middle Dot Rule.** The interface separates facts on one line with a middle dot. Overpass sets it too tight, so that one character is excluded from the bundled face and falls back to the system font; keep it that way.

## Layout

A desktop panel inside a docked window, sized by container queries, not by the viewport. The Delivery panel is a header row, a batch row, then two columns: a task list between 230px and 290px wide with a 1px hairline on its right, and the open task taking the rest with 12px 16px 24px of padding. Sections of the task stack on a 14px gap.

Rhythm is tight and built from a short set: 2, 4, 6, 8, 12 and 16px. Rows are 7px 12px 8px; header and banner rows 6 to 8px by 12px; controls 4px 10px. Separation is a hairline or space, not a box. Text that does not fit is cut to one line with an ellipsis in a row and clamped to two lines for a title or a station's detail.

The two lines of the stage diagram sit side by side only while every station keeps room for its words (about 200px per agent stage, 160px per delivery step); narrower, Entrega goes under Agentes, and a line whose stations still do not fit wraps into a second row that starts at the edge.

### Named Rules
**The One Surface Per Fact Rule.** A fact is shown in one place. If the line already says it, the row does not repeat it, the header does not restate it and no badge echoes it. Only the selected row shows secondary data; long sections stay folded under a heading with their gist on one line.

## Elevation & Depth

Flat. Depth is tonal: paper ground, white for what can be typed in or pressed, a slightly darker strip behind the tabs. Nothing at rest casts a shadow.

### Shadow Vocabulary
- **Warm float** (`box-shadow: 0 10px 28px rgba(36, 26, 23, 0.16), 0 1px 3px rgba(36, 26, 23, 0.12)`): menus, the theme picker and modals that open on top. Tinted from the ink, never black.
- **Scrim** (`rgba(36, 26, 23, 0.32)`): the ink wash behind a modal.
- **Station halo** (`box-shadow: 0 0 0 4px` of the station's own colour at 24%): the response of a station under the pointer or with focus. A state, not a resting depth.

### Named Rules
**The Flat At Rest Rule.** A surface lifts only when it opens on top of the page. Rows, bars, banners and stations never carry a resting shadow.

## Shapes

Square. Every control and container has a corner radius of 0: buttons, inputs, selects, checkboxes, tabs, menus, modals, banners, badges, tooltips, scrollbar thumbs. The theme enforces it with one rule that zeroes the radius of everything except a short list of genuinely round marks.

Round is reserved for marks that are dots by nature: the stations of the stage line and of the small line in a task row, and small status dots (live, coordinator, repo tab). Within the diagram, a square is itself a signal: the red stop and the skipped tick are square precisely because the stations around them are round.

Lines are 1px hairlines; the selected row has a 2px ink left edge; the rail is 4px; the focus ring is a 2px ink outline (paper inside the window bar). Scrollbars are thin, square and have no arrow buttons.

### Named Rules
**The Square Corner Rule.** No rounded corners on controls or containers, at any size. The stations of the stage line and small status dots are the only round marks.

## Components

### Buttons
Plain, small and square; the label does the work.
- **Shape:** square (0 radius), 4px 10px, 1px hairline border, optional 14px stroked icon with a 6px gap.
- **Default:** white surface, body text. Hover is the 8% ink wash; a button whose menu is open stays pressed.
- **Primary:** ink fill, paper text, no visible border. One per view: the next thing the task needs, or "Nueva tarea" when nothing does. Hover brightens it slightly.
- **Quiet:** no border, no fill, muted text; on hover a 5% ink wash and body text. For secondary actions beside a primary or a default ("Archivadas…", "Añadir bloqueo").
- **Focus:** 2px ink outline, 1px offset. **Disabled:** 50% opacity.

### Inputs / Fields
- **Style:** white surface, 1px hairline, square, 4px 8px, 12 to 13px text; placeholder in muted text at full opacity.
- **Hover / Focus:** selects and checkboxes darken their border to muted on hover; focus is the ink outline; a checked checkbox is solid ink. The caret is Agentes blue.

### Navigation
- **Window bar:** a solid ink band with paper text; its own controls use the ink-side values, and the menus it opens return to paper.
- **Dock tabs:** the active tab is white with an ink line on top; the strip and the other tabs stay quiet over a hairline.
- **Task rows:** borderless, transparent rows. Line one is the key in strong text with the small line at the right; line two the muted title on one line; line three, when there is something to say, a single status line with its time at the right in tabular figures. The selected row gets the 12% ink wash and a 2px ink left edge. Packages nest under their issue with a 40px indent and a fold.
- **Links:** Agentes blue, underlined only under the pointer. The task's outbound links (Jira, PR, QA record) are borderless 26px chips in strong text whose icon takes the colour of what they need.

### The "Te necesita" Queue
What needs the user, above the task list. A label heading whose flag and count are amber; plain rows split by a hairline, each with the task's key in strong text, one line of what it needs (red only if it broke), and the one control that answers it. A row that has just arrived tints amber once and settles.

### Folded Sections
A long part of the task (Alcance, Decisiones, Historial, Pull request) sits under a label heading with a drawn chevron that turns down when open, and its gist on one muted line. Open, its content sits in a hairline box.

### Asks and Stops
- **Yours:** a 1px amber border with an 8% amber fill, the text of what is asked and its buttons on one line.
- **Broke:** a thin red edge with no fill, red on the icon and the title only, then the blockers as plain lines and the actions, primary first.
- **Note:** a hairline box, neutral. Nothing to do and nothing broken gets no colour.

### The Stage Line (signature)
Each group of stages is a line with its name above it in its colour; each stage is a station centred over its name (strong), up to two lines of detail (small, muted) and who runs it.

- **The rail** is a continuous 4px solid line from station to station. It is the line's colour only between two passed stations (skipped ones between them do not cut it) and rail grey everywhere else. Never dashed, never dotted, never a gradient.
- **The station's form says the state** (14px, 2.5px ring):
  - filled disc in the line's colour: passed;
  - ring with a centre dot: running;
  - hollow grey ring: pending, never done yet;
  - hollow ring in the line's colour: done before, repeats this round;
  - thick amber ring (3.5px): yours;
  - red square (12px): broke;
  - ink warning triangle (16px) over a patch of paper: reached with a problem nobody has to answer here;
  - small grey square tick (6px): skipped.
- **Something running that is no stage** (a setup, the app's checks) is a 9px running ring on the stretch after the stage it follows.
- **Under the pointer or with focus,** a stage that opens its history lights its station (it grows to 1.2 and takes the halo), its name takes the line's colour with a 2px underline, its detail darkens to ink, and a square ink hint above the station says what it opens. No grey box appears behind the cell.
- **Motion:** state colours change over 200ms; the hover response takes 120ms; when a task merges, the line draws itself to the Merge station over 300ms. With reduced motion the draw becomes a fade.
- **The small line** in a task row repeats the same forms at 6px with a 2px gap and 4px between the two lines, so the list can be scanned for the one square or amber ring.

### Named Rules
**The Solid Rail Rule.** The rail is always solid and continuous. It is coloured only between two passed stations; everything else is grey.

**The Lit Station Rule.** A stage under the pointer lights its station and its words. It never gets a grey box.

**The State By Form Rule.** Every state has its own shape, so the diagram still reads without colour.

## Do's and Don'ts

### Do:
- **Do** keep every control and container square (0 radius), including badges, tooltips, menus and modals.
- **Do** use amber only for what waits on the user and red only for what broke; everything else that is merely noteworthy stays neutral with an icon.
- **Do** show a fact on one surface and fold or remove whatever restates it.
- **Do** draw the rail solid, 4px, coloured only between two passed stations.
- **Do** answer the pointer on a stage by lighting its station and its words.
- **Do** give each state a form of its own before giving it a colour.
- **Do** use ink for the primary action, the selection edge and the focus ring, and keep one primary button per view.
- **Do** separate with hairlines and space, and leave secondary controls without a box.
- **Do** tint shadows and scrims from the ink, and only on surfaces that open on top.
- **Do** write the interface in Spanish and set times, costs and counts in tabular figures.

### Don't:
- **Don't** round a corner. The stations of the stage line and small status dots are the only round marks.
- **Don't** use amber for a warning nobody has to answer, or for a task that is blocked by something else.
- **Don't** use red for emphasis or for anything that did not break.
- **Don't** dash, dot or fade the rail, or colour a stretch that is not between two passed stations.
- **Don't** put a grey box behind a stage under the pointer.
- **Don't** repeat in a badge, a header or a second row what the line already says.
- **Don't** use green for selection, focus or links; green is the Entrega line, running and done.
- **Don't** add coffee-shop decoration: no beans, cups, steam, textures or brown gradients.
- **Don't** rest a shadow under a row, a bar or a banner, and never use a black shadow.
