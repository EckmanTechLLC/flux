const http = require('http');
const fs = require('fs');
const path = require('path');
const url = require('url');

const DIST = path.join(__dirname, 'dist');
const FLUX_API = 'http://localhost:3000';
const PORT = 8080;

const MIME = {
  '.html': 'text/html',
  '.js': 'application/javascript',
  '.wasm': 'application/wasm',
  '.css': 'text/css',
};

const server = http.createServer((req, res) => {
  const parsed = url.parse(req.url);
  
  // Proxy /api/* to Flux
  if (parsed.pathname.startsWith('/api/')) {
    const proxyUrl = FLUX_API + parsed.path;
    const proxyReq = http.request(proxyUrl, { method: req.method, headers: req.headers }, (proxyRes) => {
      res.writeHead(proxyRes.statusCode, proxyRes.headers);
      proxyRes.pipe(res);
    });
    proxyReq.on('error', (e) => {
      res.writeHead(502);
      res.end('Flux proxy error: ' + e.message);
    });
    req.pipe(proxyReq);
    return;
  }
  
  // Serve static files
  let filePath = path.join(DIST, parsed.pathname === '/' ? 'index.html' : parsed.pathname);
  if (!fs.existsSync(filePath)) {
    filePath = path.join(DIST, 'index.html');
  }
  
  const ext = path.extname(filePath);
  const mime = MIME[ext] || 'application/octet-stream';
  
  fs.readFile(filePath, (err, data) => {
    if (err) {
      res.writeHead(404);
      res.end('Not found');
      return;
    }
    res.writeHead(200, { 'Content-Type': mime });
    res.end(data);
  });
});

server.listen(PORT, () => {
  console.log(`Ratzilla serving at http://localhost:${PORT}`);
  console.log(`Proxying /api/* to ${FLUX_API}`);
});
