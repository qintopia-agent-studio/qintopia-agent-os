# Rust Quality Prebuilt Tool Installation

Date: 2026-07-23

## Observed Evidence

The Rust quality job repeatedly spent several minutes in `cargo install` for
`cargo-nextest` and `cargo-llvm-cov`. A prior run failed while downloading the crates.io
`indexmap` entry with `curl [16] Error in the HTTP2 framing layer`. Because the install
step failed before producing coverage files, the unconditional artifact upload then
reported that `coverage` contained no files.

## Root Cause

The workflow compiled both fixed-version CI tools from crates.io on every high-risk run.
Cargo retry and disabled HTTP/2 multiplexing reduced some transient failures but did not
remove the crates.io index and source-download dependency.

The official cargo-nextest and cargo-llvm-cov documentation recommends
`taiki-e/install-action` for prebuilt GitHub Actions installation. The action verifies
checksums by default and supports disabling its Cargo fallback.

## Resolution

- Install fixed `nextest@0.9.138` and `cargo-llvm-cov@0.8.7` prebuilt binaries through
  `taiki-e/install-action@v2`.
- Set `fallback: none` so a missing prebuilt tool cannot silently return to
  `cargo install`.
- Create a bounded installation-strategy artifact before downloading tools and retain
  the installed version output after success.
- Keep the full coverage, all-feature nextest, and clippy gates unchanged.
- Update the CI contract checker to reject source installation or missing prebuilt
  version, checksum, fallback, and evidence boundaries.

## Validation

- `node tools/ci/check-ci-contracts.mjs`
- `npm run tools:ci:check`
- `npm run format:check`
- `npm run lint:md`
- `git diff --check`

## Remaining Boundary

The prebuilt download still requires GitHub network access, but it no longer depends on
the crates.io sparse index or compiling CI tools from source. Test, coverage, clippy,
PostgreSQL, Release Please validation, and production deployment gates remain unchanged.
