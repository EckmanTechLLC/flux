# Session: flux-interact Skill Accuracy Fixes
**Date:** 2026-02-26

## Task
Fix accuracy errors in the flux-interact OpenClaw skill documentation.

## Files Modified

### skills/flux-interact/flux-interact/README.md
- ClawHub Install: Removed "Future" from heading, replaced install command with URL
- Prerequisites: Added public instance URL (https://api.flux-universe.com)
- Usage Examples: Added namespace prefix note at top of section
- Architecture diagram: Added public instance to curl line
- Limitations: Replaced with accurate content (no WS in flux.sh, domain-agnostic)
- Future Enhancements: Removed WebSocket and Authentication items (already done)
- Contributing: Changed NYX project link to Flux

### skills/flux-interact/flux-interact/references/api.md
- timestamp field: Marked as required with usage examples
- Batch response: Removed `"error": null` from success result objects
- WebSocket heading: Removed "(Future)", updated description
- WebSocket subscribe message: Changed `entityId` to `entity_id`
- WebSocket update notification: Replaced with correct `state_update` format (no entity wrapper)

## Known Issue (Not Fixed)
The skill has a double-nested directory structure: `skills/flux-interact/flux-interact/`.
The outer `flux-interact/` appears to be the repo/package directory and the inner `flux-interact/`
is the actual skill directory. This is likely confusing for ClawHub or local installs.
Should be addressed separately — do not fix in this session.
