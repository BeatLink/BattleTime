// The palette pane, in the BattleTime sidebar: widgets, events, effects and values to put into the app
// open in the editor. It inserts into the last app source file that had focus.

import * as crypto from "node:crypto";
import * as path from "node:path";
import * as vscode from "vscode";

import { read_catalog, type Catalog, type CatalogEntry } from "./catalog";
import { page_html } from "./page";
import { find_battletime_folder } from "./session";
import { add_import, import_for, snippet_for } from "./snippets";

// ============================================================================
// Messages from the webview
// ============================================================================

type WebviewMessage = { type: "ready" } | { type: "insert"; kind: CatalogEntry["kind"]; name: string };

const SECTIONS: Record<CatalogEntry["kind"], keyof Catalog> = {
  widget: "widgets",
  event: "events",
  effect: "effects",
  value: "values",
};

// ============================================================================
// The pane
// ============================================================================

export class PaletteView implements vscode.WebviewViewProvider {
  static readonly view_id = "battletime.palette";

  private view: vscode.WebviewView | undefined;
  private app: vscode.Uri | undefined;
  private battletime_folder: string | undefined;
  private catalog: Catalog | undefined;

  constructor(private readonly context: vscode.ExtensionContext) {}

  /** Makes `app` the file the palette inserts into, reading the palette from its checkout's SDK. */
  set_app(app: vscode.Uri) {
    const battletime_folder = find_battletime_folder(path.dirname(app.fsPath));
    if (!battletime_folder) return;
    this.app = app;
    if (battletime_folder !== this.battletime_folder) {
      this.battletime_folder = battletime_folder;
      this.catalog = read_catalog(path.join(battletime_folder, "sdk", "battletime", "src"));
    }
    this.show_state();
  }

  resolveWebviewView(view: vscode.WebviewView) {
    this.view = view;
    const media = (name: string) => view.webview.asWebviewUri(vscode.Uri.joinPath(this.context.extensionUri, "media", name)).toString();
    view.webview.options = { enableScripts: true, localResourceRoots: [vscode.Uri.joinPath(this.context.extensionUri, "media")] };
    view.webview.html = page_html({
      title: "Palette",
      csp_source: view.webview.cspSource,
      nonce: crypto.randomBytes(16).toString("base64"),
      style_uris: [media("common.css"), media("palette.css")],
      script_uri: media("palette.js"),
      body: `<p id="target"></p>
<input id="search" type="search" placeholder="Search widgets, events, effects and values">
<div id="sections"></div>`,
    });
    view.webview.onDidReceiveMessage((message: WebviewMessage) => {
      if (message.type === "ready") this.show_state();
      else void this.insert(message.kind, message.name);
    });
    view.onDidDispose(() => (this.view = undefined));
  }

  private show_state() {
    if (!this.view) return;
    const target = this.app && this.battletime_folder ? path.relative(this.battletime_folder, this.app.fsPath) : undefined;
    void this.view.webview.postMessage({ type: "state", target, catalog: this.catalog });
  }

  /** Puts a palette entry at the cursor in the app's editor, and imports what it needs. */
  async insert(kind: CatalogEntry["kind"], name: string) {
    const entry = this.catalog?.[SECTIONS[kind]].find((candidate) => candidate.name === name);
    if (!entry || !this.app) return;
    const app = this.app;
    const visible = vscode.window.visibleTextEditors.find((editor) => editor.document.uri.toString() === app.toString());
    // The editor takes focus first, because a snippet only stays in an editor the user is in.
    const options = { viewColumn: visible?.viewColumn ?? vscode.ViewColumn.One };
    const editor = visible ? await vscode.window.showTextDocument(visible.document, options) : await vscode.window.showTextDocument(app, options);

    const wanted = import_for(entry);
    const change = add_import(editor.document.getText(), wanted.name, wanted.module);
    if (change) {
      const range = new vscode.Range(editor.document.positionAt(change.start), editor.document.positionAt(change.end));
      await editor.edit((builder) => builder.replace(range, change.text));
    }
    const inserted = await editor.insertSnippet(new vscode.SnippetString(snippet_for(entry)));
    if (!inserted) void vscode.window.showWarningMessage(`${entry.name} could not be inserted; put the cursor in ${path.basename(app.fsPath)} and try again.`);
  }
}
