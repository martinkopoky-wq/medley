# Contributing to Medley

Thanks for helping. Bug reports, layouts, fixes and new features are all welcome. For anything large, open an issue first so we can agree on the shape before you write the code.

## Build and test

You need Rust 1.88 or newer. On Linux, install the packages listed under [Install](README.md#install) first.

```sh
cargo run                                   # debug build of the app
cargo test --workspace                      # all unit tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

CI runs rustfmt, clippy with warnings denied, and the tests on Linux, macOS and Windows. A pull request should pass all three locally.

Dependencies are compiled with optimizations even in debug builds (see `[profile.dev.package."*"]` in `Cargo.toml`), so `cargo run` stays responsive enough to judge animations and scrolling.

## Code style

- Formatting is whatever `cargo fmt` produces with the repository's [`rustfmt.toml`](rustfmt.toml) (`max_width = 120`, `use_small_heuristics = "Max"`).
- Errors use `anyhow` with messages a user can act on ("SoundCloud needs your app's client ID and secret (Accounts panel)"), because many of them end up in the UI.
- Keep the UI thread free: network and disk work runs on the Tokio runtime and reports back through a message, then requests a repaint.
- Medley exists to be light. Avoid per-frame allocations in hot UI paths, don't request continuous repaints, and prefer streaming or file-backed buffers over holding large data in memory.
- Never log or display tokens, secrets or API keys. Secrets belong in `credentials.json` via `CredentialStore`, not in `settings.json`.

## Project layout

| Crate | What lives there |
| --- | --- |
| `crates/medley-core` | Models, the `Provider` trait, settings and credentials, OAuth helpers, song matching, the layout format |
| `crates/medley-providers` | One module per music service |
| `crates/medley-audio` | Playback engine (librespot, rodio/symphonia, stream buffering) |
| `crates/medley-ai` | AI features and their Ollama and Claude backends |
| `crates/medley` | The desktop app |

## Adding a music service

Every service implements [`medley_core::Provider`](crates/medley-core/src/provider.rs):

1. Add a variant to `ProviderKind` in `crates/medley-core/src/model.rs`, and extend `ALL`, `name()` and `slug()`.
2. If the service needs configuration, add a settings struct to `Settings` (non-secret values) and fields to `Credentials` (tokens and secrets) in `crates/medley-core/src/store.rs`. Use `#[serde(default)]` so existing settings files keep loading.
3. Create `crates/medley-providers/src/<service>.rs` and implement the trait:
   - `setup_hint` returns what the user still has to configure, or `None`.
   - `connect` runs the browser login. Use `medley_core::oauth::{Pkce, wait_for_code}` for OAuth with PKCE on a `127.0.0.1` redirect, and persist tokens through `CredentialStore::update`.
   - `restore` brings a session back from stored tokens without user interaction, refreshing them if needed.
   - `search`, `liked_tracks`, `playlists`, `playlist_tracks` and `create_playlist` map the service's responses to `Track` and `PlaylistSummary`. Fill in `isrc` whenever the API offers it; it makes cross-service matching exact.
   - `find_by_isrc` is optional but strongly recommended.
   - `stream` returns a `StreamSource`: `Http` for progressive files, `Hls` for HLS playlists. Mark tracks that only have previews with `Playability::Preview`.
4. Use the helpers in `crates/medley-providers/src/http.rs` (`client`, `send`, `json`, `ok`) so rate limiting and error messages behave like the other services.
5. Add unit tests for response parsing with captured, anonymized JSON. Tests must not touch the network.
6. Register the provider in the app and document its setup and limitations in the README.

Only integrate services through their official APIs and within their terms of use. Pull requests that circumvent DRM or access controls won't be accepted.

## Submitting a layout

Layouts are JSON files in [`community/layouts/`](community/layouts/). The format and the submission steps are described in [community/layouts/README.md](community/layouts/README.md). Include a screenshot in the pull request.

## Licensing

Medley is dual licensed under MIT or Apache-2.0. By submitting a contribution, you agree that it is licensed under both, without any additional terms or conditions.

## Demo mode and screenshots

The app can run fully offline with fake data, which is handy for UI work and for the images in `docs/screenshots/`:

```sh
MEDLEY_DEMO=1 cargo run --release                      # fake accounts, playlists, mixes; nothing is saved
MEDLEY_DEMO=1 MEDLEY_THEME=dark MEDLEY_LAYOUT="DJ Booth" \
  MEDLEY_SCREENSHOT=docs/screenshots/dj-booth.png cargo run --release   # save one frame, then quit
```

Other switches: `MEDLEY_THEME=light|dark`, `MEDLEY_ACCENT=<blue|purple|pink|red|orange|yellow|green|graphite>`, `MEDLEY_PANEL=<panel slug>` (for example `ai_studio`). See `crates/medley/src/demo.rs`.

