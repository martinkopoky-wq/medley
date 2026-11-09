---
title: Respect Spotify cooldowns and distinguish API limits from login failures
author: Martinkopoky-wq
date: 2026-11-09
branch: main
cost: { minutes: 23, tokens: 118712 }
id: l-db02210b
scope: repo
status: active
src: agent
touches:
  - ".github/workflows/release.yml"
  - README.md
  - packaging/linux/appimage.sh
  - ".gemini/settings.json"
  - ".github/workflows/ci.yml"
  - AGENTS.md
  - CONTRIBUTING.md
  - Cargo.lock
  - Cargo.toml
  - GEMINI.md
---

## Decided
Never shorten an API's Retry-After deadline. Retry automatically once only when the advertised delay is at most ten seconds; otherwise return the error with the full wait time so the UI stays responsive. A 429 is not evidence of a bad password. For Spotify, explain the shared built-in client ID and how to switch to a personal developer app and reconnect. Distinguish QUOTA_EXCEEDED from a short rate limit; reconnecting does not replenish an app quota.

## Tried and ruled out
The original HTTP helper used min(10) on Retry-After, so a one-hour cooldown caused a new request after ten seconds. Waiting the full hour inside Connect would instead leave an opaque pending action. Neither behavior helps the user.

## Watch out
Changing the configured client ID does not reassign existing OAuth tokens: reconnect is required. Keep personal client IDs in user settings, not public release defaults. Linux release binaries must be compiled on Ubuntu 22.04 for the documented glibc 2.35 baseline; the local Debian build requires glibc 2.39, and AppImage packaging does not lower that requirement.

In the code:
- crates/medley-providers/src/http.rs: send, retry_after, error_message, and loopback regression tests.
- crates/medley/src/panels/accounts.rs: Spotify setup and developer dashboard link.
- .github/workflows/release.yml: ubuntu-22.04 and offline AppImage GUI smoke test.
