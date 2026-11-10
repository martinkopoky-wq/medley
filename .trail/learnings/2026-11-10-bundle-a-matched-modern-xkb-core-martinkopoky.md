---
title: Bundle a matched modern XKB core and X11 pair
author: Martinkopoky-wq
date: 2026-11-10
branch: main
id: l-3cd90145
scope: repo
status: active
replaces: l-e91431f4
src: agent
touches:
  - packaging/linux/appimage.sh
  - ".github/workflows/release.yml"
  - crates/medley-audio/src/spotify.rs
---

## Decided
Build libxkbcommon 1.11.0 core and X11 from the same pinned source commit on Ubuntu 22.04 and bundle both. They share private symbols, so a host core and a bundled extension from different builds must not be mixed. Validate the actual CI-built AppImage on the newer desktop before making it the recommended release.

Spotify developer Web API credentials and playback grants still need separate identities and caches. Validate login5 before reporting playback authorization. Successful login does not prove audio keys can be obtained; upstream issue 1649 remains the documented account limitation.

## Tried and ruled out
The Ubuntu 22.04 keyboard pair was internally consistent but its old core rejected newer Compose keysyms. Excluding only the core appeared to work with a local Debian package, yet the CI artifact mixed the host core with the older X11 extension and crashed. The previous host-core recommendation is superseded by the matched modern pair.

## Watch out
A green smoke test on the old build host is insufficient for new desktop compatibility. A track Unavailable event is not proof of a country restriction. Clear failed tracks, ignore stale events, and retain the honest Spotify limitation in the app and website.

In the code:
- packaging/linux/build-xkb.sh builds the pinned matched pair into target/appimage without installing system libraries.
- packaging/linux/appimage.sh deploys both libraries with a controlled library search path.
- crates/medley-audio/src/spotify.rs keeps separate playback approval and load-failure handling.
