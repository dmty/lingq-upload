# Design principles

LingQ Importer is a macOS app that happens to render in a web view. Every screen should be mistakable for AppKit. When a choice is unclear, open Finder, Notes, or Music and copy what they do.

Status: reference. Tokens live in `src/app.css`; components in `src/lib/components/`.

## 1. Use the platform's controls, at the platform's size

- Regular control height is 24–26px in the web view (AppKit: 22px). Large buttons are for empty states only. Never a full-width button.
- Buttons have intrinsic width and a constant label. "Upload" stays "Upload" whether enabled or not. The reason it is disabled goes in a hint beside it.
- One-of-N choices with short labels: segmented control. With long labels or more than four options: popup button. Never a row of cards or a grid of large tiles.
- Boolean: checkbox. Mutually exclusive settings with details: radio group, details indented under the selected item.
- Default button is accent-filled. Disabled is greyed text on a neutral background, never a grey fill that could be read as a secondary button.
- Text fields, search fields, popups: 24–28px tall, `--radius-sm`.

## 2. Sidebar is for navigating content

- Sidebar rows: 13px, 26–28px tall, an SF Symbol leading each row, translucent material behind (vibrancy on the window, transparent sidebar background).
- Selection is an accent pill on the row; there is no hover state.
- Settings are not content. They open from the app menu (⌘,) in their own window or sheet, not from the sidebar.
- The content pane is edge-to-edge beside the sidebar, separated by a 1px `--color-border`. No inset card, no rounded floating panel.

## 3. Actions live in the toolbar, menus, and keyboard

- Primary actions once, in the toolbar, with tooltips. The item shows a pressed state while its screen is open.
- Row-level actions live in a context menu (right-click) and keyboard shortcuts. Not in persistent text links on every row.
- Hover may reveal a single ellipsis or handle at the row end. It never fills the row.
- Blue text means "opens a web page". Actions are buttons or menu items, never blue text.
- Destructive actions ask once (sheet) or offer undo. The label says what happens: "Move to Trash" if there is a trash, "Delete" if there is not.

## 4. Selection, not hover

- Lists highlight the selected row with an accent fill and `--color-on-fill` text. Hover changes nothing except the cursor and any revealed controls.
- Keyboard focus is a separate ring (`--color-accent-ring`), never reused to mean "selected".

## 5. Typography

- UI text: system font. 13px body in chrome (sidebar, tables, forms, toolbar), 11px secondary and column headers, 14px only for reading-heavy panes. Window title 13–15px semibold.
- Sentence case everywhere. No all-caps labels, no tracked eyebrows, no zero-padded numbers.
- Numbers in tables and stats use `font-variant-numeric: tabular-nums`, right-aligned.
- Serif (Literata) is reserved for content the user reads: chapter text preview. Never for chrome.
- Monospace only for values the user might copy exactly (a model id, a key suffix), at 11px muted.

## 6. Show exceptions, not the norm

- A status that is true for most rows is not shown. "Done" on every finished book, "High" on every confident match, "100%" on every read lesson: remove. Show "Needs review", "Low match", and partial progress only.
- One visual encoding per status. A badge or a colored bar or a dot, not two of them.
- Hide columns that are entirely zero or entirely identical.

## 7. One meaning per visual device

| Device | Means | Nowhere else |
|---|---|---|
| Dashed border | Drop target | Not empty states, not collapsed groups |
| Accent fill | Selected row, default button | Not decoration |
| Blue text | Web hyperlink | Not actions |
| Orange dot | Needs the user's attention | Not "in progress" |
| Green | Success confirmation, transient | Not steady-state "done" |

## 8. Density and rhythm

- List rows 28–56px. A cover thumbnail in a list is at most 40×56.
- Group headers inside a list are section rows, not separate cards. If cards are unavoidable, `--radius-md` and an 8px gap.
- Hairline separators (`--color-border`) between rows; no shadows inside the window.
- Forms: label above or right-aligned with a colon, 8px label-to-field, 16px between fields, 24px between groups.

## 9. Words

- Name things the way the user thinks: "Set aside", not "Park". "Low match", not "HIGH/LOW". "Continue matching", not "Resolve".
- Language names, not codes. IDs (course numbers, ASINs, filenames) are hidden or in tooltips; the course page may show the LingQ course id under "Open in LingQ".
- Explain every number that is not self-evident: "76,300 known words", not "(76,300)".
- Helper text earns its place by changing what the user does. Delete lines like "Where this app keeps your keys and preferences."
- Errors say what happened and what to do next, in one or two sentences, no apology.
- Placeholders describe the default behavior: "Taken from the book if left blank", "Auto-fills from audio filename".

## 10. Formats

- Duration under an hour: `m:ss`. One hour or more: `h:mm:ss`. Totals in a header may use `10h 51m`.
- Counts with a unit: "13 lessons", "5 books". Thousands separated per locale.
- Relative time for freshness ("updated just now"), absolute time in tooltips.
- Titles from LingQ are normalized for display: no spaces between CJK characters, lesson number separated from title.

## 11. Keyboard

| Shortcut | Action |
|---|---|
| ⌘N | Add project |
| ⌘⇧U | Quick upload |
| ⌘, | Settings |
| ⌘F | Focus search |
| ⌘[ / ⌘] | Back / forward |
| ⌘O or ↩ | Open selected item |
| ⌫ | Move selected item to Trash (asks first) |
| ↑ ↓ | Move selection |
| ⌥↑ / ⌥↓ | Reorder selected chapter |
| Space | Play / pause in the inspector |
| Esc | Dismiss sheet, clear search |

Every shortcut also appears as a menu item with the shortcut shown.

## 12. Appearance

- Follow the system: light/dark, accent color, reduced motion, reduced transparency. Already wired through `--color-accent` from NSColor; keep it that way.
- Motion only in response to the user's action, 150–200ms, `--ease-snappy`. No page-load entrances, no hover transitions on every card.
- Sheets and alerts for confirmations use the native dialog plugin where possible, so they look like every other Mac dialog.

## 13. The one deliberate thing

Chapter text in Literata, set generously, in the inspector. That is where the app is allowed to have a voice. Everything around it is quiet AppKit.
