# Testing Guide

This repository has two classes of tests:

1) Default (offline) tests
   - Fast, deterministic, no network access.
   - Run with: `cargo test --workspace`

2) Live (online) tests
   - Use real public feeds (RSS/Atom/JSON) to validate end-to-end fetching and parsing.
   - Disabled by default (`#[ignore]`) and gated by `CAPTURA_TEST_LIVE=1`.
   - Recommended to run serially to reduce load on sources.

Live test commands:

- Pipeline live tests:
  `CAPTURA_TEST_LIVE=1 cargo test -p captura-pipeline -- --ignored --test-threads=1`

- API live tests:
  `CAPTURA_TEST_LIVE=1 cargo test -p captura-api -- --ignored --test-threads=1`

- Scheduler live tests:
  `CAPTURA_TEST_LIVE=1 cargo test -p captura-scheduler -- --ignored --test-threads=1`

Notes:
- Sources include Rust blog, XKCD, BBC, NASA, Solidot (中文), NHK (日本語), jsonfeed.org, Daring Fireball.
- If any source becomes unstable, run live tests manually and adjust/replace sources as needed.
- CI workflow `.github/workflows/live-tests.yml` is provided for manual trigger via GitHub Actions.

