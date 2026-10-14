// Serves the exported web build to the Electron window.
//
// Expo's static export uses absolute URLs, which break under file://, so the
// main process serves the directory over loopback instead.

const { createServer } = require('node:http');
const { createReadStream, existsSync, statSync } = require('node:fs');
const { extname, join, normalize } = require('node:path');

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.map': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.ttf': 'font/ttf',
  '.otf': 'font/otf',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
};

// Map a URL onto a file in the export: a route becomes <name>.html.
function resolve(root, urlPath) {
  const clean = decodeURIComponent(urlPath.split('?')[0].split('#')[0]);
  const safe = normalize(clean).replace(/^(\.\.[/\\])+/, '');

  const candidates = [];
  if (safe === '/' || safe === '') {
    candidates.push('index.html');
  } else {
    const relative = safe.replace(/^\//, '');
    candidates.push(relative);
    candidates.push(`${relative}.html`);
    candidates.push(join(relative, 'index.html'));
  }
  candidates.push('+not-found.html');
  candidates.push('index.html');

  for (const candidate of candidates) {
    const full = join(root, candidate);
    if (full.startsWith(root) && existsSync(full) && statSync(full).isFile()) {
      return full;
    }
  }
  return null;
}

// Start on an ephemeral port and report the URL to load.
function serve(root) {
  return new Promise((resolve_, reject) => {
    const server = createServer((request, response) => {
      const file = resolve(root, request.url || '/');
      if (!file) {
        response.writeHead(404);
        response.end('Not found');
        return;
      }
      response.writeHead(200, {
        'Content-Type': TYPES[extname(file)] || 'application/octet-stream',
      });
      createReadStream(file).pipe(response);
    });

    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve_({ url: `http://127.0.0.1:${port}`, server });
    });
  });
}

module.exports = { serve, resolve };
