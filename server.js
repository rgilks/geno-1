// Dev server for Generative Visualizer (Web)
const http = require("http");
const fs = require("fs");
const path = require("path");
const url = require("url");

const PORT = process.env.PORT || 8080;
const HOST = process.env.HOST || "localhost";

const ROOT = path.join(__dirname, "crates", "app-web");
const PKG = path.join(ROOT, "pkg");

const MIME_TYPES = {
  ".html": "text/html",
  ".css": "text/css",
  ".js": "application/javascript",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".gif": "image/gif",
  ".svg": "image/svg+xml",
  ".ico": "image/x-icon",
};

const SECURITY_HEADERS = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
  "Cross-Origin-Resource-Policy": "cross-origin",
};

function getContentType(filePath) {
  return MIME_TYPES[path.extname(filePath).toLowerCase()] || "application/octet-stream";
}

function serveFile(filePath, res) {
  fs.readFile(filePath, (err, data) => {
    if (err) {
      res.writeHead(404, { "Content-Type": "text/plain", ...SECURITY_HEADERS });
      res.end("404 Not Found");
      return;
    }
    res.writeHead(200, { "Content-Type": getContentType(filePath), ...SECURITY_HEADERS });
    res.end(data);
  });
}

function handleRequest(req, res) {
  const parsedUrl = url.parse(req.url);
  let pathname = parsedUrl.pathname;

  if (pathname === "/") pathname = "/index.html";
  if (pathname === "/favicon.ico") {
    res.writeHead(200, { "Content-Type": "image/svg+xml", ...SECURITY_HEADERS });
    res.end("<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    return;
  }

  // Prefer pkg (built wasm/js)
  let filePath = path.join(PKG, pathname);
  fs.access(filePath, fs.constants.F_OK, (err) => {
    if (err) {
      // Fallback to root static (index.html, import map, etc.)
      filePath = path.join(ROOT, pathname);
      fs.access(filePath, fs.constants.F_OK, (err2) => {
        if (err2) {
          res.writeHead(404, { "Content-Type": "text/html", ...SECURITY_HEADERS });
          res.end(`<h1>404</h1><p>Missing: ${pathname}</p>`);
          return;
        }
        serveFile(filePath, res);
      });
    } else {
      serveFile(filePath, res);
    }
  });
}

const server = http.createServer(handleRequest);
server.listen(PORT, HOST, () => {
  console.log(`Dev server: http://${HOST}:${PORT}`);
});
server.on("error", (err) => {
  console.error("Server error:", err);
  process.exit(1);
});
