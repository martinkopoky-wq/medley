---
title: "Keep vergen pinned to 9.0.x or librespot-core 0.8 won't build"
author: Martinkopoky-wq
date: 2026-11-09
cost: { minutes: 1, tokens: 0 }
id: l-ba363bbb
scope: repo
status: active
src: agent
---

## Watch out
`crates/medley-audio/Cargo.toml` pulls librespot-core 0.8, whose build script uses vergen-gitcl 1.0. That only works with vergen 9.0.x (vergen-lib 0.1). A broad `cargo update` pulls vergen 9.1.0, which switches to vergen-lib 9.x, and the build then fails with "the trait bound `vergen::feature::build::Build: vergen_lib::entries::Add` is not satisfied" inside librespot-core's build.rs.

## Decided
Pin it in `Cargo.lock` instead of patching librespot. Re-run the pin after any broad `cargo update` until librespot ships a fix.

In the code:
- `cargo update -p vergen --precise 9.0.6`
- `Cargo.lock` must keep vergen 9.0.6 and a single vergen-lib 0.1.x
