// Types for web-request.js, which the command line tool imports.

import type { NativeWebReply, NativeWebRequest } from '../src/services/web-request';

export function webRequest(request: NativeWebRequest): Promise<NativeWebReply>;
