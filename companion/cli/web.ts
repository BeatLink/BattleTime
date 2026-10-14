// Web requests from the terminal, through node/web-request.js, so a URL may use a self-signed certificate.

import { bridgeRequester, normalizeFingerprint, type WebAccess, type WebRequester } from '@/services/web-request';

import { webRequest } from '../node/web-request';

export function terminalRequester(): WebRequester {
  return bridgeRequester({
    async request(request) {
      try {
        return { ok: true, value: await webRequest(request) };
      } catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : String(error) };
      }
    },
  });
}

// Takes --header 'Name: value', --user name:password, --pin <sha256> and --insecure out of the arguments,
// and gives the access settings they make with the arguments left over.
export function takeWebAccess(args: string[]): { access: WebAccess; rest: string[] } {
  const access: WebAccess = {};
  const rest: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    const value = () => {
      const next = args[++i];
      if (next === undefined) throw new Error(`${arg} needs a value.`);
      return next;
    };
    switch (arg) {
      case '--header': {
        const header = value();
        const at = header.indexOf(':');
        if (at < 1) throw new Error(`--header takes "Name: value", not "${header}".`);
        access.headers = { ...access.headers, [header.slice(0, at).trim()]: header.slice(at + 1).trim() };
        break;
      }
      case '--user': {
        const login = value();
        const at = login.indexOf(':');
        access.username = at < 0 ? login : login.slice(0, at);
        if (at >= 0) access.password = login.slice(at + 1);
        break;
      }
      case '--pin': {
        const fingerprint = normalizeFingerprint(value());
        if (!fingerprint) throw new Error('--pin takes the SHA-256 fingerprint, 64 hex digits.');
        access.certificate = 'pinned';
        access.certificateSha256 = fingerprint;
        break;
      }
      case '--insecure':
        access.certificate = 'any';
        break;
      default:
        rest.push(arg);
    }
  }
  return { access, rest };
}
