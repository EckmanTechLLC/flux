# ADR-007 Phase 3A: Bug Fix and Production Testing

**Date:** 2026-02-21
**Session Type:** Foundation (bug fix + testing)
**Status:** Complete

---

## Task Summary

Identified and fixed bugs in the Phase 3A generic connector implementation through live testing with real APIs. All three auth types validated in production.

---

## Changes Made

### Files Modified
- `connector-manager/src/runners/generic.rs` — Fixed Bento poll interval: replaced invalid `interval` field with `rate_limit_resources` block (Bento's correct mechanism)
- `ui/public/index.html` — Fixed `formatValue()` to handle nested objects (`JSON.stringify`) and arrays of objects

### Bugs Fixed

1. **Bento config lint error — invalid `interval` field**
   - Initial fix guessed `interval: {n}s` on `http_client` input (wrong)
   - Bento rejected it: `field interval not recognised`
   - Correct fix: `rate_limit_resources` with `local` backend and `count: 1 / interval: {n}s`
   - `poll_interval_secs` from config is now correctly rendered into Bento YAML

2. **UI showing `[object Object]` for nested values**
   - `formatValue()` fell through to `String(v)` for objects → `[object Object]`
   - Fixed: added `typeof v === 'object'` branch returning `JSON.stringify(v)`
   - Fixed: array elements that are objects now JSON-stringified (not `.toString()`)

---

## Test Results

| Source | Auth | Result |
|--------|------|--------|
| CoinGecko Bitcoin price | None | `crypto/bitcoin` entity ✅ |
| httpbin.org/json | None | `httpbin/test` entity ✅ |
| Polygon AAPL prev close | Bearer token | `stocks/AAPL` entity ✅ |
| Brave Search | API key header (X-Subscription-Token) | `brave/brave` entity ✅ |

Delete and restart persistence also confirmed working.

---

## Checklist Completion

- [x] READ FIRST
- [x] VERIFY ASSUMPTIONS
- [x] MAKE CHANGES
- [x] TEST & VERIFY
- [x] DOCUMENT
- [x] REPORT

---

## Issues Encountered

- `interval` field guessed without checking Bento docs — corrected to `rate_limit_resources`
- UI `formatValue` didn't handle nested JSON — fixed with `JSON.stringify` branch

---

## Next Steps

Phase 3B: Named Connector (Singer taps)
