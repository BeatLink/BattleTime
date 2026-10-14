// Makes a web request the user set up, for the desktop app and the command line tool.
//
// The TLS connection is made first and its certificate checked here, as src/services/web-request.ts
// asks: one the system trusts, one with a pinned SHA-256 fingerprint, or any. Only then is the request,
// with its headers and login, sent over it, so nothing reaches a server that failed the check.

const http = require('node:http');
const net = require('node:net');
const tls = require('node:tls');

// The most of a reply kept, so a huge page cannot fill memory.
const MOST_REPLY_BYTES = 4 * 1024 * 1024;

// How many redirects are followed before giving up.
const MOST_REDIRECTS = 5;

// The host without the brackets an IPv6 address has in a URL.
const bareHost = (url) => url.hostname.replace(/^\[(.*)\]$/, '$1');

const fingerprintDigits = (text) => String(text || '').replace(/[\s:]/g, '').toLowerCase();

// Opens a TLS connection to the address and checks its certificate as the request asks.
function connectTls(url, request) {
  return new Promise((resolve, reject) => {
    const host = bareHost(url);
    const socket = tls.connect({
      host,
      port: Number(url.port) || 443,
      servername: net.isIP(host) ? undefined : host,
      rejectUnauthorized: request.certificate === 'system',
    });
    socket.once('error', reject);
    socket.once('secureConnect', () => {
      if (request.certificate === 'pinned') {
        const shown = fingerprintDigits(socket.getPeerCertificate().fingerprint256);
        if (shown !== fingerprintDigits(request.certificateSha256)) {
          socket.destroy();
          reject(new Error(`${url.host} showed a certificate with SHA-256 fingerprint ${shown}, not the pinned one.`));
          return;
        }
      }
      socket.off('error', reject);
      resolve(socket);
    });
  });
}

// Sends one request and resolves with its status, headers and body, without following a redirect.
async function sendOnce(url, request, headers) {
  const socket = url.protocol === 'https:' ? await connectTls(url, request) : null;
  return new Promise((resolve, reject) => {
    const outgoing = http.request(
      {
        protocol: 'http:',
        host: bareHost(url),
        port: Number(url.port) || (url.protocol === 'https:' ? 443 : 80),
        path: `${url.pathname}${url.search}`,
        method: request.method,
        headers: { Host: url.host, ...headers },
        ...(socket ? { createConnection: () => socket } : { agent: false }),
      },
      (reply) => {
        const chunks = [];
        let size = 0;
        reply.on('data', (chunk) => {
          size += chunk.length;
          if (size > MOST_REPLY_BYTES) {
            reply.destroy(new Error(`The reply is over ${MOST_REPLY_BYTES / 1024 / 1024} MB.`));
            return;
          }
          chunks.push(chunk);
        });
        reply.on('error', reject);
        reply.on('end', () =>
          resolve({
            status: reply.statusCode,
            statusText: reply.statusMessage || '',
            location: reply.headers.location,
            body: new Uint8Array(Buffer.concat(chunks)),
          }),
        );
      },
    );
    outgoing.on('error', reject);
    if (request.body !== null && request.body !== undefined) {
      outgoing.write(request.body);
    }
    outgoing.end();
  });
}

// Makes the request, following redirects, and resolves with { status, statusText, body }.
async function webRequest(request) {
  let url = new URL(request.url);
  let current = { ...request };
  let headers = { ...request.headers };
  const work = (async () => {
    for (let redirects = 0; ; redirects++) {
      if (url.protocol !== 'http:' && url.protocol !== 'https:') {
        throw new Error(`${url.href} is not an http or https URL.`);
      }
      const reply = await sendOnce(url, current, headers);
      if (![301, 302, 303, 307, 308].includes(reply.status) || !reply.location) {
        return { status: reply.status, statusText: reply.statusText, body: reply.body };
      }
      if (redirects >= MOST_REDIRECTS) {
        throw new Error(`Gave up after ${MOST_REDIRECTS} redirects.`);
      }
      const next = new URL(reply.location, url);
      // The login and a pinned certificate belong to the first host, so neither goes on to another.
      if (next.host !== url.host) {
        headers = Object.fromEntries(Object.entries(headers).filter(([name]) => name.toLowerCase() !== 'authorization'));
        if (current.certificate === 'pinned') current = { ...current, certificate: 'system' };
      }
      if (reply.status === 303 || ((reply.status === 301 || reply.status === 302) && current.method === 'POST')) {
        current = { ...current, method: 'GET', body: null };
      }
      url = next;
    }
  })();
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`No answer within ${request.timeoutMs / 1000} s.`)), request.timeoutMs);
  });
  try {
    return await Promise.race([work, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

module.exports = { webRequest };
