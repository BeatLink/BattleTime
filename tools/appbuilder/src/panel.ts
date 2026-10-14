// The builder panel: the palette on the left, next to the app's source in the editor, and the simulated
// watch on the right. Saving the app recompiles it and restarts the simulator.

import { spawn } from "node:child_process";
import * as crypto from "node:crypto";
import * as path from "node:path";
import * as vscode from "vscode";

import { read_catalog, type Catalog, type CatalogEntry } from "./catalog";
import { AppSession, type CompileError } from "./session";
import { add_import, import_for, snippet_for } from "./snippets";

// ============================================================================
// Messages from the webview
// ============================================================================

type WebviewMessage =
  | { type: "ready" }
  | { type: "pointer"; action: "press" | "move" | "release"; x: number; y: number }
  | { type: "button" }
  | { type: "theme" }
  | { type: "restart" }
  | { type: "insert"; kind: CatalogEntry["kind"]; name: string };

const SECTIONS: Record<CatalogEntry["kind"], keyof Catalog> = {
  widget: "widgets",
  event: "events",
  effect: "effects",
  value: "values",
};

// ============================================================================
// The panel
// ============================================================================

export class BuilderPanel {
  private static current: BuilderPanel | undefined;

  private readonly session: AppSession;
  private readonly catalog: Catalog;
  private readonly disposables: vscode.Disposable[] = [];

  /** Opens the builder on `app`, or moves the open builder to it. */
  static show(context: vscode.ExtensionContext, app: vscode.Uri, neotime_folder: string, diagnostics: vscode.DiagnosticCollection) {
    if (BuilderPanel.current && BuilderPanel.current.neotime_folder === neotime_folder) {
      BuilderPanel.current.app = app;
      BuilderPanel.current.panel.title = BuilderPanel.title_for(app);
      BuilderPanel.current.panel.reveal(vscode.ViewColumn.Beside);
      void BuilderPanel.current.run();
      return;
    }
    BuilderPanel.current?.panel.dispose();
    const panel = vscode.window.createWebviewPanel("neotime.appBuilder", BuilderPanel.title_for(app), vscode.ViewColumn.Beside, {
      enableScripts: true,
      retainContextWhenHidden: true,
      localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, "media")],
    });
    BuilderPanel.current = new BuilderPanel(context, panel, app, neotime_folder, diagnostics);
  }

  private static title_for(app: vscode.Uri): string {
    return `App Builder: ${path.basename(path.dirname(app.fsPath))}`;
  }

  private constructor(
    context: vscode.ExtensionContext,
    private readonly panel: vscode.WebviewPanel,
    private app: vscode.Uri,
    private readonly neotime_folder: string,
    private readonly diagnostics: vscode.DiagnosticCollection,
  ) {
    const output = vscode.window.createOutputChannel("NeoTime App Builder");
    this.disposables.push(output);
    this.catalog = read_catalog(path.join(neotime_folder, "sdk", "neotime", "src"));

    const toolchain = vscode.workspace.getConfiguration("neotime.appBuilder").get<string>("toolchainCommand", "nix develop --command");
    this.session = new AppSession(neotime_folder, toolchain, spawn, {
      status: (text) => this.post({ type: "status", text }),
      frame: (pixels) => this.post({ type: "frame", pixels: new Uint8Array(pixels.buffer) }),
      motor: (running) => this.post({ type: "motor", running }),
      log: (text) => {
        output.appendLine(text);
        this.post({ type: "log", text });
      },
      errors: (errors) => this.show_errors(errors),
    });

    panel.webview.html = this.html(context);
    panel.webview.onDidReceiveMessage((message: WebviewMessage) => this.receive(message), null, this.disposables);
    panel.onDidDispose(() => this.dispose(), null, this.disposables);
    vscode.workspace.onDidSaveTextDocument(
      (document) => {
        if (this.belongs_to_app(document.uri)) void this.run();
      },
      null,
      this.disposables,
    );
  }

  /** A saved file in the app's folder may change what the app shows. */
  private belongs_to_app(uri: vscode.Uri): boolean {
    return path.dirname(uri.fsPath) === path.dirname(this.app.fsPath) && /\.tsx?$/.test(uri.fsPath);
  }

  private run(): Promise<void> {
    return this.session.run(this.app.fsPath);
  }

  private post(message: unknown) {
    void this.panel.webview.postMessage(message);
  }

  private show_errors(errors: CompileError[]) {
    this.diagnostics.clear();
    const by_file = new Map<string, vscode.Diagnostic[]>();
    for (const error of errors) {
      const place = new vscode.Position(Math.max(error.line - 1, 0), Math.max(error.column - 1, 0));
      const diagnostic = new vscode.Diagnostic(new vscode.Range(place, place.translate(0, 1)), error.message, vscode.DiagnosticSeverity.Error);
      diagnostic.source = "neotime-compiler";
      by_file.set(error.file, [...(by_file.get(error.file) ?? []), diagnostic]);
    }
    for (const [file, list] of by_file) this.diagnostics.set(vscode.Uri.file(path.resolve(this.neotime_folder, file)), list);
    this.post({ type: "errors", errors });
  }

  private receive(message: WebviewMessage) {
    switch (message.type) {
      case "ready":
        this.post({ type: "catalog", catalog: this.catalog });
        void this.run();
        break;
      case "pointer":
        this.session.send(`${message.action} ${Math.round(message.x)} ${Math.round(message.y)}`);
        break;
      case "button":
        this.session.send("button");
        break;
      case "theme":
        this.session.send("theme");
        break;
      case "restart":
        void this.run();
        break;
      case "insert":
        void this.insert(message.kind, message.name);
        break;
    }
  }

  /** Puts a palette entry at the cursor in the app's editor, and imports what it needs. */
  private async insert(kind: CatalogEntry["kind"], name: string) {
    const entry = this.catalog[SECTIONS[kind]].find((candidate) => candidate.name === name);
    if (!entry) return;
    const visible = vscode.window.visibleTextEditors.find((editor) => editor.document.uri.toString() === this.app.toString());
    const editor = visible ?? (await vscode.window.showTextDocument(this.app, { viewColumn: vscode.ViewColumn.One }));

    const wanted = import_for(entry);
    const change = add_import(editor.document.getText(), wanted.name, wanted.module);
    if (change) {
      const range = new vscode.Range(editor.document.positionAt(change.start), editor.document.positionAt(change.end));
      await editor.edit((builder) => builder.replace(range, change.text));
    }
    await editor.insertSnippet(new vscode.SnippetString(snippet_for(entry)));
    await vscode.window.showTextDocument(editor.document, editor.viewColumn);
  }

  private html(context: vscode.ExtensionContext): string {
    const webview = this.panel.webview;
    const media = (name: string) => webview.asWebviewUri(vscode.Uri.joinPath(context.extensionUri, "media", name));
    const nonce = crypto.randomBytes(16).toString("base64");
    return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource}; script-src 'nonce-${nonce}';">
<meta name="viewport" content="width=device-width, initial-scale=1">
<link rel="stylesheet" href="${media("builder.css")}">
<title>App Builder</title>
</head>
<body>
<main>
  <section class="palette">
    <input id="search" type="search" placeholder="Search widgets, events, effects and values">
    <div id="sections"></div>
  </section>
  <section class="preview">
    <canvas id="screen" width="240" height="240" aria-label="The simulated watch screen"></canvas>
    <div class="controls">
      <button id="button" title="The side button, which goes back a screen">Side button</button>
      <button id="theme" title="Switch between the default and a light theme">Theme</button>
      <button id="restart" title="Compile the app again and restart it">Restart</button>
    </div>
    <p id="status" role="status">Starting…</p>
    <ul id="errors"></ul>
    <pre id="log" aria-label="Effects and messages from the simulator"></pre>
  </section>
</main>
<script nonce="${nonce}" src="${media("builder.js")}"></script>
</body>
</html>`;
  }

  private dispose() {
    if (BuilderPanel.current === this) BuilderPanel.current = undefined;
    this.session.stop();
    this.diagnostics.clear();
    for (const disposable of this.disposables) disposable.dispose();
  }
}
