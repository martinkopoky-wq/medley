---
title: Use separate Spotify playback approval and the host keyboard data
author: Martinkopoky-wq
date: 2026-11-10
branch: main
id: l-e91431f4
scope: repo
status: active
src: agent
touches:
  - crates/medley-audio/src/spotify.rs
  - crates/medley/src/app.rs
  - packaging/linux/appimage.sh
---

## Decided
Keep the personal Spotify developer app for Web API search and library access. Native playback must obtain a separate PKCE grant for librespot's playback identity, and its reusable credentials must be isolated from the old developer-token cache. Validate login5 before announcing playback authorization. Authentication success still does not prove an account can obtain audio keys.

Use the host libxkbcommon base library with the host Compose data. Bundle the dynamically loaded X11 extension and libxcb-xkb, which may be absent on otherwise usable desktops.

## Tried and ruled out
A live diagnostic reproduced BAD_REQUEST with both cached and fresh developer-app credentials. Reusing those grants with the default playback client instead yielded INVALID_CREDENTIALS. A fresh playback-specific browser approval passed login5. Two user-reported tracks then failed with audio key error 0 1 on an account confirmed as Premium. The upstream maintainers document the Spotify-side account compatibility problem in https://github.com/librespot-org/librespot/issues/1649; do not present repeated authentication or a country change as a fix for it.

Bundling Ubuntu 22.04's base keyboard library caused dead_hamza Compose errors on the newer host. Omitting the base library while keeping the X11 extension preserves the supported Linux build baseline without forcing old keysyms onto current desktop data.

## Watch out
An Unavailable event does not prove a country restriction. Report a neutral load failure, clear the current track, ignore late events from earlier tracks, and stop playback after repeated failures so pause and seek are not sent to a stopped player. The manual playback check is muted and needs browser approval only when no valid playback credentials are cached. Marketing and installation pages must disclose the currently known Spotify account limitation and must not claim a successful live audio test when only authentication passed.

In the code:
- crates/medley-audio/src/spotify.rs: authorize_playback, playback_session, isolated playback cache and unavailable_event.
- crates/medley-audio/examples/playback_check.rs: opt-in manual browser approval and real playback position check.
- packaging/linux/appimage.sh: --exclude-library libxkbcommon.so*.
- website/lib/install-data.ts: central release version and actual native installation commands.
