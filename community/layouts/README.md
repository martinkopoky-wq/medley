# Community layouts

Every panel in Medley can be dragged, split, stacked and tabbed. When you land on an arrangement you love, share it. This folder is the gallery: each file is one layout, written in a small JSON format that Medley reads, checks and applies.

The five presets that ship with the app live here too:

| File | Name | Idea |
| --- | --- | --- |
| [`classic.json`](classic.json) | Classic | Library on the left, your music in the middle, what's playing and what's next on the right. |
| [`focus.json`](focus.json) | Focus | Big artwork front and center. Everything else waits in a slim column. Dark theme. |
| [`studio.json`](studio.json) | AI Studio | Describe a vibe on the left, watch the mix appear in the middle. Purple accent. |
| [`dj-booth.json`](dj-booth.json) | DJ Booth | Search and the queue side by side, so lining up the next track is one drag away. Orange accent. |
| [`compact.json`](compact.json) | Compact | One column of tabs for small windows and tiling window managers. |

## Using a layout

- **Share yours:** open the **Appearance** panel and choose **Share layout**. Medley copies the current arrangement to the clipboard as JSON.
- **Try someone else's:** copy a file's contents from this folder and paste it into **Appearance → Import**, or save the `.json` file into Medley's `layouts/` data folder (see the main [README](../../README.md#where-your-data-lives)).
- **Let the AI draft one:** describe what you want in **AI Studio** ("big artwork on the left, queue under the track list") and Medley builds a layout in this same format.

## The format

A layout file is a single JSON object:

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `format` | number | no | Format version. Currently `1`. Files with a newer version are rejected rather than guessed at. |
| `name` | string | yes | Up to 48 characters. |
| `author` | string | no | Up to 48 characters. Your GitHub handle is a good choice. |
| `description` | string | no | One sentence, up to 200 characters. |
| `accent` | string | no | `blue`, `purple`, `pink`, `red`, `orange`, `yellow`, `green` or `graphite`. Applied with the layout. |
| `theme` | string | no | `system`, `light` or `dark`. Applied with the layout. |
| `root` | node | yes | The arrangement, described below. |

### Nodes

`root` is a binary tree. Each node is either a **split** or a **leaf**.

A **split** divides its area in two:

```json
{ "split": "horizontal", "fraction": 0.3, "first": { … }, "second": { … } }
```

- `split`: `"horizontal"` puts `first` on the left and `second` on the right. `"vertical"` puts `first` on top and `second` below.
- `fraction`: the share of the space given to `first`, between `0.1` and `0.9`.

A **leaf** holds one or more panels, shown as tabs:

```json
{ "panels": ["collection", "search", "settings"], "active": 0 }
```

- `panels`: panel ids, in tab order.
- `active`: index of the tab that starts selected. Optional, defaults to `0`.

### Panel ids

| Id | Panel | What it is |
| --- | --- | --- |
| `library` | Library | Sidebar with mixes, liked songs and playlists from every service |
| `search` | Search | Search field and results across Spotify, SoundCloud and Apple Music |
| `collection` | Browse | The open playlist, mix or album as a track list (the main content area) |
| `now_playing` | Now Playing | Large artwork, transport controls and AI liner notes |
| `queue` | Up Next | Upcoming songs, shuffle/repeat and "Keep the Vibe" |
| `ai_studio` | AI Studio | Turn a vibe into a mix, or describe a layout |
| `accounts` | Accounts | Connect and disconnect music services |
| `appearance` | Appearance | Theme, accent color and the layout gallery |
| `settings` | Settings | AI backend and other preferences |

### How Medley cleans up a layout

Layouts come from strangers, clipboards and language models, so Medley sanitizes every file before applying it. Nothing here makes a file fail; it just gets tidied:

- A panel that appears more than once keeps only its first appearance.
- Leaves left without panels are removed, and a split with one empty side collapses into the other side.
- `fraction` is clamped to `0.1`–`0.9`; a missing or invalid number becomes `0.5`.
- `active` is clamped to the leaf's last tab.
- `collection` (Browse) is always present. If a layout leaves it out, Medley adds it on the right, taking 65% of the width.
- `name`, `author` and `description` are trimmed and cut to their length limits; an empty name becomes "Untitled".

Panels a layout doesn't mention simply stay closed. You can reopen any of them from the toolbar.

## A complete example

This is [`classic.json`](classic.json): a 20% library sidebar, then the main area split 70/30, with Now Playing stacked above Up Next on the right.

```json
{
  "format": 1,
  "name": "Classic",
  "author": "Medley",
  "description": "Library on the left, your music in the middle, what's playing and what's next on the right.",
  "root": {
    "split": "horizontal", "fraction": 0.2,
    "first": { "panels": ["library"] },
    "second": {
      "split": "horizontal", "fraction": 0.7,
      "first": { "panels": ["collection", "search", "ai_studio", "accounts", "appearance", "settings"] },
      "second": {
        "split": "vertical", "fraction": 0.56,
        "first": { "panels": ["now_playing"] },
        "second": { "panels": ["queue"] }
      }
    }
  }
}
```

```
┌──────────┬──────────────────────────────┬─────────────┐
│          │                              │ Now Playing │
│ Library  │  Browse · Search · AI Studio │             │
│          │  · Accounts · Appearance …   ├─────────────┤
│          │                              │  Up Next    │
└──────────┴──────────────────────────────┴─────────────┘
    20%              56% (70% of 80%)        24%
```

## Submitting a layout

1. Arrange Medley the way you like it, then **Appearance → Share layout**.
2. Paste the JSON into a new file here named after your layout in kebab-case, for example `late-night-radio.json`.
3. Fill in `name`, `author` and a one-sentence `description`. Add `accent` or `theme` only if the layout is designed around them.
4. Open a pull request with a screenshot of the layout in use.

Please keep layouts focused: one idea per file, every panel used at most once, and names that describe the arrangement rather than the author. Maintainers may promote popular layouts to built-in presets, which are bundled through `builtin_layouts()` in [`crates/medley-core/src/layout.rs`](../../crates/medley-core/src/layout.rs).
