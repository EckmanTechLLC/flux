/**
 * Flux Federation Bridge
 * 
 * Syncs entities between two Flux instances bidirectionally.
 * Each instance maintains sovereignty — federation is additive.
 * 
 * Remote entities get prefixed with their origin to avoid collisions:
 *   remote entity "arc-01" → local entity "remote/arc-01"
 *   local entity "kannaka-local" → remote entity "nick-local/kannaka-local"
 */

const http = require('http');
const https = require('https');

const LOCAL = 'http://localhost:3000';
const REMOTE = 'https://flux.eckman-tech.com';
const SYNC_INTERVAL_MS = 30000; // 30 seconds
const LOCAL_PREFIX = 'nick-local';  // prefix for our entities on remote
const REMOTE_PREFIX = 'remote';     // prefix for remote entities locally

// Track what we've already synced to avoid loops
const syncedRemoteEntities = new Map(); // entity_id -> lastUpdated
const syncedLocalEntities = new Map();

function fetch(url) {
  return new Promise((resolve, reject) => {
    const mod = url.startsWith('https') ? https : http;
    mod.get(url, (res) => {
      let data = '';
      res.on('data', chunk => data += chunk);
      res.on('end', () => {
        try { resolve(JSON.parse(data)); } 
        catch (e) { reject(new Error(`Parse error: ${data.substring(0, 200)}`)); }
      });
    }).on('error', reject);
  });
}

function postEvent(baseUrl, stream, source, entityId, properties) {
  return new Promise((resolve, reject) => {
    const mod = baseUrl.startsWith('https') ? https : http;
    const body = JSON.stringify({
      stream,
      source: source,
      entityId,
      timestamp: Date.now(),
      payload: {
        entity_id: entityId,
        properties
      }
    });
    
    const url = new URL('/api/events', baseUrl);
    const req = mod.request(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) }
    }, (res) => {
      let data = '';
      res.on('data', chunk => data += chunk);
      res.on('end', () => {
        try { resolve(JSON.parse(data)); } 
        catch (e) { resolve(data); }
      });
    });
    req.on('error', reject);
    req.write(body);
    req.end();
  });
}

async function syncRemoteToLocal() {
  try {
    const entities = await fetch(`${REMOTE}/api/state/entities`);
    let synced = 0;
    
    for (const entity of entities) {
      // Skip our own federated entities (avoid loops)
      if (entity.id.startsWith(`${LOCAL_PREFIX}/`)) continue;
      
      const lastKnown = syncedRemoteEntities.get(entity.id);
      if (lastKnown === entity.lastUpdated) continue;
      
      const localId = `${REMOTE_PREFIX}/${entity.id}`;
      const props = { ...entity.properties, _federated_from: REMOTE, _source_id: entity.id };
      
      await postEvent(LOCAL, 'federation', 'flux-bridge', localId, props);
      syncedRemoteEntities.set(entity.id, entity.lastUpdated);
      synced++;
    }
    
    if (synced > 0) console.log(`[→ local] Synced ${synced} remote entities`);
  } catch (e) {
    console.error('[→ local] Sync failed:', e.message);
  }
}

async function syncLocalToRemote() {
  try {
    const entities = await fetch(`${LOCAL}/api/state/entities`);
    let synced = 0;
    
    for (const entity of entities) {
      // Skip federated entities (avoid loops)
      if (entity.id.startsWith(`${REMOTE_PREFIX}/`)) continue;
      
      const lastKnown = syncedLocalEntities.get(entity.id);
      if (lastKnown === entity.lastUpdated) continue;
      
      const remoteId = `${LOCAL_PREFIX}/${entity.id}`;
      const props = { ...entity.properties, _federated_from: LOCAL, _source_id: entity.id };
      
      await postEvent(REMOTE, 'federation', 'flux-bridge', remoteId, props);
      syncedLocalEntities.set(entity.id, entity.lastUpdated);
      synced++;
    }
    
    if (synced > 0) console.log(`[→ remote] Synced ${synced} local entities`);
  } catch (e) {
    console.error('[→ remote] Sync failed:', e.message);
  }
}

async function sync() {
  await syncRemoteToLocal();
  await syncLocalToRemote();
}

console.log(`⟁ Flux Federation Bridge`);
console.log(`  Local:  ${LOCAL}`);
console.log(`  Remote: ${REMOTE}`);
console.log(`  Interval: ${SYNC_INTERVAL_MS / 1000}s`);
console.log(`  Local prefix on remote: ${LOCAL_PREFIX}/`);
console.log(`  Remote prefix on local: ${REMOTE_PREFIX}/`);
console.log('');

// Initial sync
sync().then(() => {
  console.log('Initial sync complete. Running every 30s...');
  setInterval(sync, SYNC_INTERVAL_MS);
});
