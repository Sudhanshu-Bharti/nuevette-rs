# Nuevette

A desktop app that turns any topic into a learning path you can actually work through.

Type a topic like "Rust async" or "SQL window functions". Nuevette finds the official docs, drafts an outline for you to check, then builds a map of topics and steps. You mark steps done as you go, and it keeps track of your week.

Built in Rust with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) (the UI framework behind the Zed editor) and the [Ely](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components) component library.

## What it does

- **New path.** Give it a topic, plus an optional goal, what you already know, your level and your pace. It reads the official docs, shows you an outline to rename, reorder or trim, then writes the full path. Topics show up on the map as they're written. If you already have a path for that topic, it offers that one first.
- **Built from the real docs.** When the docs site has a table of contents (most do), the steps come straight from it. The AI only groups them into topics and writes the details, so it can't make up steps. Every step links to the docs page it came from.
- **Map.** Each topic is a row, and its steps run left to right. Click a step to see what it covers, the key concepts, prerequisites, links and where it came from. Press Space to mark it done and move to the next one.
- **Core, optional or alternative.** Steps are marked like roadmap.sh does. Optional and alternative ones have a dashed edge, so a big map stays easy to read.
- **Edit the map.** Rename, add, move or delete topics and steps from the side panel. Your done marks stay with the right steps, and deletes can be undone.
- **Share a path.** Save any path as a `.nuevette.json` file and send it to someone. They add it with Import on the Paths screen. Your own progress isn't included.
- **Today.** Your next step, every path and how far along it is, and what you finished recently.
- **Paths.** Every path you have, filtered by in progress, not started or done.
- **Search (Ctrl+K).** Jump to any action, path or step.
- **Light and dark mode.** Or follow your Windows setting.
- **Settings.** Your name, theme, default level and pace, and export or clear your data.

Everything is saved on your own computer. The app only goes online to make a path: it searches for the docs (SerpAPI), reads them, asks Gemini to write the path, and checks that the resource links still work.

## Running it

You need:

- Windows (it's only been built and tested there so far)
- [Rust](https://rustup.rs). The repo pins the stable `x86_64-pc-windows-gnu` toolchain, so you'll also need MinGW (gcc) on your PATH.
- A free Gemini API key from [Google AI Studio](https://aistudio.google.com/apikey)

Then:

```bash
git clone https://github.com/Sudhanshu-Bharti/nuevette-rs.git
cd nuevette-rs
cp .env.example .env
```

Open `.env` and add your Gemini key. Then:

```bash
cargo run
```

The first build takes a while because GPUI is large, and dependencies are built with full optimizations even in debug mode so the app stays smooth. Builds after that are much faster.

## Keys and settings

All of these go in `.env` (it's git-ignored, so your keys stay local):

| Variable | Needed? | What it's for |
| --- | --- | --- |
| `GEMINI_API_KEY` | Yes | Writes the outlines and paths |
| `SERPAPI_API_KEY` | No | Finds the official docs for topics the app doesn't already know. Without it, paths are written from Gemini's own knowledge. |
| `GEMINI_MODEL` | No | Defaults to `gemini-3.1-flash-lite` |
| `GEMINI_FALLBACK_MODEL` | No | A second model to try when the first is busy or out of quota. `none` turns it off. |

Gemini's free tier has daily limits. If you hit them, the app tells you, and you can wait or set a fallback model.

## Shortcuts

| Keys | Does |
| --- | --- |
| Ctrl+K | Search |
| Ctrl+N | New path |
| Ctrl+Q | Quit |
| Arrow keys | Move between steps on the map |
| Space | Mark the selected step done and go to the next |
| Ctrl+= / Ctrl+- | Zoom in / out |
| Ctrl+0 | Fit the whole path |
| Esc | Close the panel |

## Where your data lives

Paths are saved to `%APPDATA%\nuevette\paths.json` and settings to `settings.json` next to it. Set `NUEVETTE_DATA_DIR` to use a different folder, which is handy for testing without touching your real data.

You can export everything as JSON from Settings.

## Project layout

```
src/
  main.rs          starts the app and opens the window
  app.rs           the main window: nav, screens, toasts
  model.rs         paths, topics and steps
  store.rs         saving and loading paths
  settings.rs      your preferences
  stats.rs         weekly numbers and streaks
  theme.rs         light and dark colors
  services/        docs search, Gemini requests, link checks
  ui/
    today.rs       the Today screen
    paths.rs       the Paths screen
    composer/      New path: the form, outline review, building
    mindmap/       the map, cards, inspector and camera
    palette.rs     search (Ctrl+K)
    settings_view.rs
    glass.rs       shared cards, pills and other building blocks
assets/            the background glow images, sample paths, the app icon and the pixel mascot
```

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

If you have the app open while building, use another target folder so the build doesn't fight over the locked exe:

```bash
cargo build --target-dir target/verify
```

## Status

Early and in active development. Things that are known to be missing:

- Only Gemini is supported for now. Bring-your-own-key and other providers (including local models) are planned.
- Only tested on Windows.
- Lessons and a tutor chat for each step aren't built yet.

## License

No license has been chosen yet, so all rights are reserved for now.
