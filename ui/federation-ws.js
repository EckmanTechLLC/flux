/**
 * Flux Federation Bridge (WebSocket Real-Time)
 * 
 * Uses WebSocket subscriptions for instant sync instead of polling.
 * Falls back to HTTP polling for initial state load.
 * 
 * Remote entities get prefixed: "remote/arc-01"
 * Local entities get prefixed on remote: "nick-local/kannaka-local"
 */

const http = require('http');
const https = require('https');
const WebSocket = require('ws') || null;

const LOCAL = 'http://localhost:3000';
const REMOTE = 'https://flux.eckman-tech.com';
const LOCAL_WS = 'ws://localhost:3000/api/ws';
const REMOTE_WS = 'wss://flux.eckman-tech.com/api/ws';
const LOCAL_PREFIX = 'nick-local';
const REMOTE_PREFIX = 'remote';

// ─── HTTP helpers ───────────────────────────────────────────────────────────

function httpGet(url) {
  return new Promise((resolve, reject) => {
    const mod = url.startsWith('https') ? https : http;
    mod.get(url, res => {
      let d = '';
      res.on('data', c => d += c);
      res.on('end', () => { try { resolve(JSON.parse(d)); } catch(e) { reject(e); } });
    }).on('error', reject);
  });
}

function postEvent(baseUrl, stream, source, entityId, properties) {
  return new Promise((resolve, reject) => {
    const mod = baseUrl.startsWith('https') ? https : http;
    const body = JSON.stringify({
      stream, source, entityId,
      timestamp: Date.now(),
      payload: { entity_id: entityId, properties }
    });
    const url = new URL('/api/events', baseUrl);
    const req = mod.request(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) }
    }, res => {
      let d = '';
      res.on('data', c => d += c);
      res.on('end', () => resolve(d));
    });
    req.on('error', reject);
    req.write(body);
    req.end();
  });
}

// ─── Entity state tracking ──────────────────────────────────────────────────

// Accumulate property updates per entity, flush as complete updates
const pendingRemoteUpdates = new Map(); // entityId -> { props, timer }
const pendingLocalUpdates = new Map();
const DEBOUNCE_MS = 500; // batch rapid property updates

function scheduleFlush(pending, entityId, direction) {
  const entry = pending.get(entityId);
  if (entry.timer) clearTimeout(entry.timer);
  entry.timer = setTimeout(() => flushEntity(pending, entityId, direction), DEBOUNCE_MS);
}

async function flushEntity(pending, entityId, direction) {
  const entry = pending.get(entityId);
  if (!entry || Object.keys(entry.props).length === 0) return;
  
  const props = { ...entry.props };
  entry.props = {};
  
  try {
    if (direction === 'to-local') {
      const localId = `${REMOTE_PREFIX}/${entityId}`;
      props._federated_from = REMOTE;
      props._source_id = entityId;
      await postEvent(LOCAL, 'federation', 'flux-bridge', localId, props);
    } else {
      const remoteId = `${LOCAL_PREFIX}/${entityId}`;
      props._federated_from = LOCAL;
      props._source_id = entityId;
      await postEvent(REMOTE, 'federation', 'flux-bridge', remoteId, props);
    }
  } catch (e) {
    console.error(`[${direction}] Failed to flush ${entityId}:`, e.message);
  }
}

// ─── WebSocket bridge ───────────────────────────────────────────────────────

function connectWs(wsUrl, label, onUpdate) {
  let ws;
  let subscribedEntities = new Set();
  let reconnectTimer = null;
  
  function connect() {
    console.log(`[${label}] Connecting to ${wsUrl}...`);
    ws = new WebSocket(wsUrl);
    
    ws.on('open', () => {
      console.log(`[${label}] Connected!`);
      // Re-subscribe to known entities
      for (const eid of subscribedEntities) {
        ws.send(JSON.stringify({ type: 'subscribe', entity_id: eid }));
      }
    });
    
    ws.on('message', (data) => {
      try {
        const msg = JSON.parse(data.toString());
        if (msg.type === 'state_update') {
          onUpdate(msg.entity_id, msg.property, msg.value);
        } else if (msg.type === 'entity_deleted') {
          console.log(`[${label}] Entity deleted: ${msg.entity_id}`);
        }
        // Ignore metrics_update
      } catch (e) {}
    });
    
    ws.on('close', () => {
      console.log(`[${label}] Disconnected, reconnecting in 3s...`);
      reconnectTimer = setTimeout(connect, 3000);
    });
    
    ws.on('error', (e) => {
      console.error(`[${label}] WS error:`, e.message);
    });
  }
  
  connect();
  
  return {
    subscribe(entityId) {
      subscribedEntities.add(entityId);
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'subscribe', entity_id: entityId }));
      }
    },
    subscribeAll(entityIds) {
      for (const eid of entityIds) this.subscribe(eid);
    }
  };
}

// ─── Main ───────────────────────────────────────────────────────────────────

async function main() {
  console.log('⟁ Flux Federation Bridge (WebSocket Real-Time)');
  console.log(`  Local:  ${LOCAL} ↔ Remote: ${REMOTE}`);
  console.log('');
  
  // 1. Initial HTTP sync (full state)
  console.log('Phase 1: Initial HTTP sync...');
  
  const remoteEntities = await httpGet(`${REMOTE}/api/state/entities`);
  let remoteCount = 0;
  for (const entity of remoteEntities) {
    if (entity.id.startsWith(`${LOCAL_PREFIX}/`)) continue;
    const localId = `${REMOTE_PREFIX}/${entity.id}`;
    const props = { ...entity.properties, _federated_from: REMOTE, _source_id: entity.id };
    await postEvent(LOCAL, 'federation', 'flux-bridge', localId, props);
    remoteCount++;
  }
  console.log(`  → ${remoteCount} remote entities synced to local`);
  
  const localEntities = await httpGet(`${LOCAL}/api/state/entities`);
  let localCount = 0;
  for (const entity of localEntities) {
    if (entity.id.startsWith(`${REMOTE_PREFIX}/`)) continue;
    const remoteId = `${LOCAL_PREFIX}/${entity.id}`;
    const props = { ...entity.properties, _federated_from: LOCAL, _source_id: entity.id };
    await postEvent(REMOTE, 'federation', 'flux-bridge', remoteId, props);
    localCount++;
  }
  console.log(`  → ${localCount} local entities synced to remote`);
  
  // 2. Connect WebSockets for real-time updates
  console.log('');
  console.log('Phase 2: WebSocket subscriptions...');
  
  // Remote WS → sync updates to local
  const remoteWs = connectWs(REMOTE_WS, 'remote', (entityId, property, value) => {
    if (entityId.startsWith(`${LOCAL_PREFIX}/`)) return; // skip our own
    if (!pendingRemoteUpdates.has(entityId)) {
      pendingRemoteUpdates.set(entityId, { props: {}, timer: null });
    }
    pendingRemoteUpdates.get(entityId).props[property] = value;
    scheduleFlush(pendingRemoteUpdates, entityId, 'to-local');
  });
  
  // Local WS → sync updates to remote  
  const localWs = connectWs(LOCAL_WS, 'local', (entityId, property, value) => {
    if (entityId.startsWith(`${REMOTE_PREFIX}/`)) return; // skip federated
    if (!pendingLocalUpdates.has(entityId)) {
      pendingLocalUpdates.set(entityId, { props: {}, timer: null });
    }
    pendingLocalUpdates.get(entityId).props[property] = value;
    scheduleFlush(pendingLocalUpdates, entityId, 'to-remote');
  });
  
  // Subscribe to all known entities
  const remoteIds = remoteEntities.filter(e => !e.id.startsWith(`${LOCAL_PREFIX}/`)).map(e => e.id);
  const localIds = localEntities.filter(e => !e.id.startsWith(`${REMOTE_PREFIX}/`)).map(e => e.id);
  
  // Small delay to let WS connect
  setTimeout(() => {
    remoteWs.subscribeAll(remoteIds);
    localWs.subscribeAll(localIds);
    console.log(`  Subscribed to ${remoteIds.length} remote + ${localIds.length} local entities`);
    console.log('');
    console.log('Federation active! Real-time sync running. Ctrl+C to stop.');
  }, 2000);
  
  // Periodic re-discovery of new entities (every 60s)
  setInterval(async () => {
    try {
      const fresh = await httpGet(`${REMOTE}/api/state/entities`);
      for (const e of fresh) {
        if (e.id.startsWith(`${LOCAL_PREFIX}/`)) continue;
        if (!remoteIds.includes(e.id)) {
          remoteIds.push(e.id);
          remoteWs.subscribe(e.id);
          const localId = `${REMOTE_PREFIX}/${e.id}`;
          const props = { ...e.properties, _federated_from: REMOTE, _source_id: e.id };
          await postEvent(LOCAL, 'federation', 'flux-bridge', localId, props);
          console.log(`[discovery] New remote entity: ${e.id}`);
        }
      }
      
      const freshLocal = await httpGet(`${LOCAL}/api/state/entities`);
      for (const e of freshLocal) {
        if (e.id.startsWith(`${REMOTE_PREFIX}/`)) continue;
        if (!localIds.includes(e.id)) {
          localIds.push(e.id);
          localWs.subscribe(e.id);
          const remoteId = `${LOCAL_PREFIX}/${e.id}`;
          const props = { ...e.properties, _federated_from: LOCAL, _source_id: e.id };
          await postEvent(REMOTE, 'federation', 'flux-bridge', remoteId, props);
          console.log(`[discovery] New local entity: ${e.id}`);
        }
      }
    } catch (e) {
      // silent — discovery is best-effort
    }
  }, 60000);
}

main().catch(e => { console.error('Fatal:', e); process.exit(1); });
