# Session: Fix WebSocket Real-Time Updates

**Date:** 2026-02-14
**Phase:** 4A (Real-Time Metrics & Entity Management)
**Type:** Bugfix
**Status:** Complete ✅

---

## Task

Fix real-time entity updates via WebSocket. Entities were not appearing in UI without browser refresh.

---

## Issues Found & Fixed

### Issue 1: WebSocket Proxy Field Name Mismatch

**Problem:**
- UI proxy sends `entityId` (camelCase)
- Flux backend expects `entity_id` (snake_case)
- Subscribe messages failed to parse

**Fix:** `/home/etl/projects/flux/ui/server.js` (line 155)
```diff
- fluxWs.send(JSON.stringify({ type: 'subscribe', entityId: '*' }));
+ fluxWs.send(JSON.stringify({ type: 'subscribe', entity_id: '*' }));
```

### Issue 2: Wildcard Subscription Not Supported

**Problem:**
- Browser subscribes to `"*"` (all entities)
- `should_forward_update()` only checked exact entity_id match
- Updates never forwarded to wildcard subscribers

**Fix:** `/home/etl/projects/flux/src/subscription/manager.rs` (line 162-171)
```diff
 fn should_forward_update(&self, update: &StateUpdate) -> bool {
     if self.subscriptions.is_empty() {
         return true;
     }

+    // Check for wildcard subscription
+    if self.subscriptions.contains("*") {
+        return true;
+    }
+
     self.subscriptions.contains(&update.entity_id)
 }
```

### Issue 3: Undefined Variable in Browser Code

**Problem:**
- `flushUpdates()` referenced `eventWindow` variable that was never declared
- JavaScript error crashed update handler
- No real-time updates displayed

**Fix:** `/home/etl/projects/flux/ui/public/index.html` (line 998)
```diff
 function flushUpdates() {
   renderScheduled = false;
-
-  const now = Date.now();
-  eventWindow = eventWindow.filter(t => t > now - 5000);

   // Update pipeline stats
   updatePipeline();
```

---

## Event Payload Format Discovery

**Issue:** Events published with flat payload were ignored by state engine.

**Expected payload format:**
```json
{
  "entity_id": "entity-name",
  "properties": {
    "prop1": value1,
    "prop2": value2
  }
}
```

**Reference:** `/home/etl/projects/flux/src/state/engine.rs` (lines 170-177, 183-204)

---

## Testing

**Commands:**
```bash
# Rebuild Flux
docker-compose down && docker-compose up --build -d

# Publish test entity
curl -X POST http://localhost:3000/api/events \
  -H "Content-Type: application/json" \
  -d '{
    "stream": "test",
    "source": "claude",
    "key": "sensor-temp-1",
    "timestamp": '$(date +%s%3N)',
    "schema": "test.v1",
    "payload": {
      "entity_id": "sensor-temp-1",
      "properties": {
        "status": "healthy",
        "temperature": 21.3,
        "humidity": 72
      }
    }
  }'
```

**Verification:**
- ✅ Entity appears instantly in UI (no refresh)
- ✅ Event stream panel shows updates
- ✅ Metrics update in real-time
- ✅ No browser console errors

---

## Session Summary

**Duration:** ~30 minutes
**Files Modified:** 3
**Lines Changed:** ~10

**Checklist:**
- [x] Read CLAUDE.md
- [x] Read protocol.rs (verify field names)
- [x] Read manager.rs (wildcard logic)
- [x] Read index.html (JavaScript errors)
- [x] Fix server.js field name
- [x] Fix manager.rs wildcard support
- [x] Fix index.html undefined variable
- [x] Rebuild & test
- [x] Verify real-time updates work
- [x] Update session notes

**Result:** Real-time WebSocket updates now working correctly. Entities appear instantly without browser refresh.
