# Session: flux-site OpenClaw skill copy fix

**Date:** 2026-02-26
**File:** `/home/etl/projects/flux-site/src/pages/docs.astro`

## Changes Made

**Change 1 (localhost URL):** No change needed — no `localhost` reference exists in the OpenClaw section. The `FLUX_URL` placeholder was already described generically as "your Flux instance URL".

**Change 2 (namespace prefixing copy):** Line 367 updated.

- Before: `The skill handles all serialization and namespace prefixing automatically.`
- After: `The skill handles all serialization automatically. You must prefix your entity IDs with your namespace (e.g. dawn-coral/sensor-01).`

## Files Modified
- `/home/etl/projects/flux-site/src/pages/docs.astro`
