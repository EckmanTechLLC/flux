// Simple proxy server for Ratzilla dashboard → Flux Universe
const http = require("http");
const https = require("https");
const fs = require("fs");
const path = require("path");

const PORT = 8080;
const FLUX_HOST = "api.flux-universe.com";
const DIST = path.join(__dirname, "dist");

const MIME = {
  ".html": "text/html",
  ".js": "application/javascript",
  ".wasm": "application/wasm",
  ".css": "text/css",
};

const server = http.createServer((req, res) => {
  // Proxy /api/* to Flux Universe
  if (req.url.startsWith("/api/")) {
    const options = {
      hostname: FLUX_HOST,
      port: 443,
      path: req.url,
      method: req.method,
      headers: { ...req.headers, host: FLUX_HOST },
    };
    const proxy = https.request(options, (proxyRes) => {
      res.writeHead(proxyRes.statusCode, proxyRes.headers);
      proxyRes.pipe(res);
    });
    proxy.on("error", (e) => {
      res.writeHead(502);
      res.end("Proxy error: " + e.message);
    });
    req.pipe(proxy);
    return;
  }

  // Serve static files from dist/
  let filePath = path.join(DIST, req.url === "/" ? "index.html" : req.url);
  const ext = path.extname(filePath);
  
  fs.readFile(filePath, (err, data) => {
    if (err) {
      // SPA fallback
      fs.readFile(path.join(DIST, "index.html"), (err2, html) => {
        if (err2) { res.writeHead(404); res.end("Not found"); return; }
        res.writeHead(200, { "Content-Type": "text/html" });
        res.end(html);
      });
      return;
    }
    res.writeHead(200, { "Content-Type": MIME[ext] || "application/octet-stream" });
    res.end(data);
  });
});

// WebSocket proxy for /api/ws
const WebSocket = require("ws") || null;
server.on("upgrade", (req, socket, head) => {
  if (req.url === "/api/ws") {
    const target = new (require("ws"))(`wss://${FLUX_HOST}/api/ws`);
    target.on("open", () => {
      // Complete the WebSocket handshake manually
      const wss = new (require("ws").Server)({ noServer: true });
      wss.handleUpgrade(req, socket, head, (clientWs) => {
        // Relay messages both ways
        clientWs.on("message", (data) => target.send(data));
        target.on("message", (data) => clientWs.send(data));
        target.on("close", () => clientWs.close());
        clientWs.on("close", () => target.close());
      });
    });
    target.on("error", () => socket.destroy());
  }
});

server.listen(PORT, () => {
  console.log(`🐀 Ratzilla dashboard → http://localhost:${PORT}`);
  console.log(`   Proxying to ${FLUX_HOST}`);
});
