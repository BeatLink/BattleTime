// The simulator pane: the app running on the simulated watch, beside its source in the editor.
// Saving the app recompiles it and restarts the simulator.

import { spawn } from "node:child_process";
import * as crypto from "node:crypto";
import * as path from "node:path";
import * as vscode from "vscode";

import { page_html } from "./page";
import { AppSession, type CompileError } from "./session";

// ============================================================================
// Messages from the webview
// ============================================================================

type WebviewMessage =
  | { type: "ready" }
  | { type: "pointer"; action: "press" | "move" | "release"; x: number; y: number }
  | { type: "button" }
  | { type: "theme" }
  | { type: "restart" };

// ============================================================================
// The pane
// ============================================================================

export class SimulatorPanel {
  private static current: SimulatorPanel | undefined;

  private readonly session: AppSession;
  private readonly disposables: vscode.Disposable[] = [];

  /** Runs `app` in the simulator pane, opening the pane if it is closed. */
  static show(context: vscode.ExtensionContext, app: vscode.Uri, battletime_folder: string, diagnostics: vscode.DiagnosticCollection) {
    const current = SimulatorPanel.current;
    if (current && current.battletime_folder === battletime_folder) {
      current.app = app;
      current.panel.title = SimulatorPanel.title_for(app);
      current.panel.reveal(vscode.ViewColumn.Beside, true);
      void current.run();
      return;
    }
    current?.panel.dispose();
    const panel = vscode.window.createWebviewPanel(
      "battletime.simulator",
      SimulatorPanel.title_for(app),
      { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, "media")],
      },
    );
    SimulatorPanel.current = new SimulatorPanel(context, panel, app, battletime_folder, diagnostics);
  }

  private static title_for(app: vscode.Uri): string {
    return `Watch: ${path.basename(path.dirname(app.fsPath))}`;
  }

  private constructor(
    context: vscode.ExtensionContext,
    private readonly panel: vscode.WebviewPanel,
    private app: vscode.Uri,
    private readonly battletime_folder: string,
    private readonly diagnostics: vscode.DiagnosticCollection,
  ) {
    const output = vscode.window.createOutputChannel("BattleTime App Builder");
    this.disposables.push(output);

    const toolchain = vscode.workspace.getConfiguration("battletime.appBuilder").get<string>("toolchainCommand", "nix develop --command");
    this.session = new AppSession(battletime_folder, toolchain, spawn, {
      status: (text) => this.post({ type: "status", text }),
      frame: (pixels) => this.post({ type: "frame", pixels: new Uint8Array(pixels.buffer) }),
      motor: (running) => this.post({ type: "motor", running }),
      log: (text) => {
        output.appendLine(text);
        this.post({ type: "log", text });
      },
      errors: (errors) => this.show_errors(errors),
    });

    panel.iconPath = vscode.Uri.joinPath(context.extensionUri, "media", "watch.svg");
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
      diagnostic.source = "battletime-compiler";
      by_file.set(error.file, [...(by_file.get(error.file) ?? []), diagnostic]);
    }
    for (const [file, list] of by_file) this.diagnostics.set(vscode.Uri.file(path.resolve(this.battletime_folder, file)), list);
    this.post({ type: "errors", errors });
  }

  private receive(message: WebviewMessage) {
    switch (message.type) {
      case "ready":
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
    }
  }

  private html(context: vscode.ExtensionContext): string {
    const webview = this.panel.webview;
    const media = (name: string) => webview.asWebviewUri(vscode.Uri.joinPath(context.extensionUri, "media", name)).toString();
    return page_html({
      title: "Watch",
      csp_source: webview.cspSource,
      nonce: crypto.randomBytes(16).toString("base64"),
      style_uris: [media("common.css"), media("simulator.css")],
      script_uri: media("simulator.js"),
      body: `<main>
  <canvas id="screen" width="240" height="240" aria-label="The simulated watch screen"></canvas>
  <div class="controls">
    <button id="button" title="The side button, which goes back a screen">Side button</button>
    <button id="theme" title="Switch between the default and a light theme">Theme</button>
    <button id="restart" title="Compile the app again and restart it">Restart</button>
  </div>
  <p id="status" role="status">Starting…</p>
  <ul id="errors"></ul>
  <pre id="log" aria-label="Effects and messages from the simulator"></pre>
</main>`,
    });
  }

  private dispose() {
    if (SimulatorPanel.current === this) SimulatorPanel.current = undefined;
    this.session.stop();
    this.diagnostics.clear();
    for (const disposable of this.disposables) disposable.dispose();
  }
}
