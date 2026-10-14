// The HTML skeleton both webviews share: a strict content policy, their stylesheets and one script.

export interface PageParts {
  title: string;
  /** The webview's own origin, which may serve the stylesheets. */
  csp_source: string;
  /** Lets only the page's own script run. */
  nonce: string;
  style_uris: string[];
  script_uri: string;
  body: string;
}

export function page_html(parts: PageParts): string {
  const styles = parts.style_uris.map((uri) => `<link rel="stylesheet" href="${uri}">`).join("\n");
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${parts.csp_source}; script-src 'nonce-${parts.nonce}';">
<meta name="viewport" content="width=device-width, initial-scale=1">
${styles}
<title>${parts.title}</title>
</head>
<body>
${parts.body}
<script nonce="${parts.nonce}" src="${parts.script_uri}"></script>
</body>
</html>`;
}
