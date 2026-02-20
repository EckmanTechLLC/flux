const http = require('http');
const fs = require('fs');
const path = require('path');
const WebSocket = require('ws');
const { execSync, spawn } = require('child_process');

const PORT = process.env.UI_PORT || 8082;
const FLUX_API = process.env.FLUX_API || 'http://localhost:3000';
const FLUX_WS = process.env.FLUX_WS || 'ws://localhost:3000/api/ws';
const LOADGEN_HOST = 'etl@192.168.50.40';
const LOADGEN_SCRIPT = '/home/etl/flux-loadgen.sh';

// Track load generator state
let loadgenProc = null;
let loadgenStats = { running: false, sent: 0, errors: 0, rate: 0, elapsed: 0 };

function parseBody(req) {
  return new Promise((resolve, reject) => {
    let chunks = '';
    req.on('data', c => chunks += c);
    req.on('end', () => {
      try { resolve(JSON.parse(chunks)); } catch(e) { resolve({}); }
    });
    req.on('error', reject);
  });
}

// Serve static files
const server = http.createServer(async (req, res) => {

  // Load test control API
  if (req.url === '/loadtest/start' && req.method === 'POST') {
    if (loadgenProc) {
      res.writeHead(409, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'Already running' }));
      return;
    }
    const params = await parseBody(req);
    const rate = Math.min(parseInt(params.rate) || 100, 50000);
    const entities = Math.min(parseInt(params.entities) || 50, 50000);
    const prefix = (params.prefix || 'loadtest').replace(/[^a-zA-Z0-9-]/g, '');
    const batchSize = Math.min(parseInt(params.batchSize) || 100, 1000);
    const duration = parseInt(params.duration) || 0;

    const concurrency = Math.min(parseInt(params.concurrency) || 20, 100);
    const env = `FLUX_URL=https://flux.eckman-tech.com RATE=${rate} ENTITIES=${entities} PREFIX=${prefix} BATCH_SIZE=${batchSize} DURATION=${duration} CONCURRENCY=${concurrency}`;
    const script = rate > 500 ? '/home/etl/flux-loadgen-fast.py' : LOADGEN_SCRIPT;
    const cmd = rate > 500 ? `${env} python3 ${script}` : `${env} ${script}`;
    
    loadgenStats = { running: true, sent: 0, errors: 0, rate: 0, elapsed: 0, config: { rate, entities, prefix, batchSize, duration, concurrency } };
    
    loadgenProc = spawn('ssh', ['-o', 'StrictHostKeyChecking=no', LOADGEN_HOST, cmd], {
      stdio: ['ignore', 'pipe', 'pipe']
    });

    loadgenProc.stdout.on('data', (data) => {
      const line = data.toString().trim();
      // Parse stats lines: [5s] Sent: 300 | Errors: 0 | Rate: ~60/s
      const match = line.match(/\[(\d+)s\] Sent: (\d+) \| Errors: (\d+) \| Rate: ~(\d+)\/s/);
      if (match) {
        loadgenStats.elapsed = parseInt(match[1]);
        loadgenStats.sent = parseInt(match[2]);
        loadgenStats.errors = parseInt(match[3]);
        loadgenStats.rate = parseInt(match[4]);
      }
      // Parse done line
      const doneMatch = line.match(/Done.*Sent: (\d+).*Errors: (\d+).*Duration: (\d+)s.*Avg: (\d+)/);
      if (doneMatch) {
        loadgenStats.sent = parseInt(doneMatch[1]);
        loadgenStats.errors = parseInt(doneMatch[2]);
        loadgenStats.elapsed = parseInt(doneMatch[3]);
        loadgenStats.rate = parseInt(doneMatch[4]);
      }
    });

    loadgenProc.on('close', (code) => {
      loadgenStats.running = false;
      loadgenProc = null;
    });

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: 'started', config: loadgenStats.config }));
    return;
  }

  if (req.url === '/loadtest/stop' && req.method === 'POST') {
    if (loadgenProc) {
      // Kill the remote process
      try { execSync(`ssh -o StrictHostKeyChecking=no ${LOADGEN_HOST} "pkill -f flux-loadgen.sh"`, { timeout: 5000 }); } catch(e) {}
      loadgenProc.kill();
      loadgenProc = null;
      loadgenStats.running = false;
    }
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: 'stopped', stats: loadgenStats }));
    return;
  }

  if (req.url === '/loadtest/status' && req.method === 'GET') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(loadgenStats));
    return;
  }

  // API proxy
  if (req.url.startsWith('/api/')) {
    try {
      let body = '';
      if (['POST', 'PUT', 'PATCH'].includes(req.method)) {
        body = await new Promise((resolve, reject) => {
          let chunks = '';
          req.on('data', c => chunks += c);
          req.on('end', () => resolve(chunks));
          req.on('error', reject);
        });
      }
      const opts = {
        method: req.method,
        headers: { 'Content-Type': 'application/json' }
      };
      if (req.headers['authorization']) {
        opts.headers['Authorization'] = req.headers['authorization'];
      }
      if (body) opts.body = body;
      const resp = await fetch(`${FLUX_API}${req.url}`, { ...opts, redirect: 'manual' });
      if (resp.status >= 300 && resp.status < 400) {
        res.writeHead(resp.status, { 'Location': resp.headers.get('location') });
        res.end();
        return;
      }
      const data = await resp.text();
      res.writeHead(resp.status, {
        'Content-Type': 'application/json',
        'Access-Control-Allow-Origin': '*'
      });
      res.end(data);
    } catch (e) {
      res.writeHead(502);
      res.end(JSON.stringify({ error: e.message }));
    }
    return;
  }

  // Static files
  let filePath = req.url === '/' ? '/index.html' : req.url;
  filePath = path.join(__dirname, 'public', filePath);
  const ext = path.extname(filePath);
  const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css' };
  
  fs.readFile(filePath, (err, data) => {
    if (err) { res.writeHead(404); res.end('Not found'); return; }
    res.writeHead(200, { 'Content-Type': types[ext] || 'text/plain' });
    res.end(data);
  });
});

// WebSocket proxy
const wss = new WebSocket.Server({ server });
wss.on('connection', (clientWs) => {
  const fluxWs = new WebSocket(FLUX_WS);
  
  fluxWs.on('open', () => {
    fluxWs.send(JSON.stringify({ type: 'subscribe', entity_id: '*' }));
  });
  
  fluxWs.on('message', (data) => {
    if (clientWs.readyState === WebSocket.OPEN) {
      clientWs.send(data.toString());
    }
  });

  clientWs.on('message', (data) => {
    if (fluxWs.readyState === WebSocket.OPEN) {
      fluxWs.send(data.toString());
    }
  });

  clientWs.on('close', () => fluxWs.close());
  fluxWs.on('close', () => clientWs.close());
  fluxWs.on('error', () => {});
  clientWs.on('error', () => {});
});

server.listen(PORT, '0.0.0.0', () => {
  console.log(`Flux UI running at http://0.0.0.0:${PORT}`);
});
