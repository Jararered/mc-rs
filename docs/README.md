# Documentation

One fact has one home. Other files link to it.

| File | What it is |
| --- | --- |
| [AGENTS.md](../AGENTS.md) | Rules for every change. |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Plugins, the world session, dimensions, streaming. |
| [BLOCK_TICKS.md](BLOCK_TICKS.md) | How to give a block update behavior. |
| [PERFORMANCE.md](PERFORMANCE.md) | Measurements behind the rendering rules. |
| [todo/GENERAL.md](todo/GENERAL.md) | Beta behavior the game does not have yet. |
| [todo/MULTIPLAYER.md](todo/MULTIPLAYER.md) | What multiplayer still needs. |
| [ROADMAP.md](../ROADMAP.md) | The order to build what those lists contain. |
| [CHANGELOG.md](../CHANGELOG.md) | Player-visible changes to test in game. |

A public module's `//!` says what it is, which Beta type it transcribes, and the one constraint a
caller needs. It does not repeat the guide.

## Writing

- Wrap prose at 100 columns.
- `#` is the title. `##` is a section. `###` only when a section has real subsections.
- A status line ("not sent yet", "still single-player") lives in `todo/` or `ROADMAP.md`. A how-to
  says how the code works now.
- Link a file only when it is in the tree.
- Put a Beta class name in backticks when the sentence is about transcribing it.
