// The NeoTime App Builder: the palette in the sidebar, the simulator beside the editor, and a command
// to start a new app.

import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";

import { PaletteView } from "./palette_view";
import { find_neotime_folder } from "./session";
import { SimulatorPanel } from "./simulator_panel";
import { is_app_id, starter_app } from "./starter";

function is_app_source(uri: vscode.Uri | undefined): uri is vscode.Uri {
  return !!uri && uri.scheme === "file" && uri.fsPath.endsWith(".tsx") && !!find_neotime_folder(path.dirname(uri.fsPath));
}

/** Gives the palette to whoever asks for this extension's exports, such as an editor test. */
export function activate(context: vscode.ExtensionContext): { palette: PaletteView } {
  const diagnostics = vscode.languages.createDiagnosticCollection("neotime-appbuilder");
  const palette = new PaletteView(context);
  context.subscriptions.push(diagnostics, vscode.window.registerWebviewViewProvider(PaletteView.view_id, palette));

  // The palette follows the app source file last in focus, and keeps it while the palette itself has focus.
  const follow = (editor: vscode.TextEditor | undefined) => {
    if (is_app_source(editor?.document.uri)) palette.set_app(editor.document.uri);
  };
  follow(vscode.window.activeTextEditor);
  context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(follow));

  /** Shows the palette and runs `app` in the simulator pane. */
  const open_builder = async (app: vscode.Uri) => {
    const neotime_folder = find_neotime_folder(path.dirname(app.fsPath))!;
    palette.set_app(app);
    await vscode.commands.executeCommand(`${PaletteView.view_id}.focus`);
    await vscode.window.showTextDocument(app, { viewColumn: vscode.ViewColumn.One });
    SimulatorPanel.show(context, app, neotime_folder, diagnostics);
  };

  context.subscriptions.push(
    vscode.commands.registerCommand("neotime.appBuilder.open", async (clicked?: vscode.Uri) => {
      const app = clicked ?? vscode.window.activeTextEditor?.document.uri;
      if (!app || !app.fsPath.endsWith(".tsx")) {
        void vscode.window.showErrorMessage("Open an app's .tsx file first, such as apps/hello/app.tsx.");
        return;
      }
      if (!is_app_source(app)) {
        void vscode.window.showErrorMessage("This file is not inside a NeoTime checkout, so there is no compiler or simulator to run it.");
        return;
      }
      await open_builder(app);
    }),

    vscode.commands.registerCommand("neotime.appBuilder.newApp", async () => {
      const workspace = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
      const neotime_folder = workspace && find_neotime_folder(workspace);
      if (!neotime_folder) {
        void vscode.window.showErrorMessage("Open the NeoTime repository first.");
        return;
      }
      const id = await vscode.window.showInputBox({
        prompt: "The new app's id, which names its folder in apps/",
        placeHolder: "my-app",
        validateInput: (text) => {
          if (!is_app_id(text)) return "Use lower case words joined by hyphens, such as my-app";
          if (fs.existsSync(path.join(neotime_folder, "apps", text))) return `apps/${text} already exists`;
          return undefined;
        },
      });
      if (!id) return;
      const app_file = path.join(neotime_folder, "apps", id, "app.tsx");
      fs.mkdirSync(path.dirname(app_file), { recursive: true });
      fs.writeFileSync(app_file, starter_app(id));
      await open_builder(vscode.Uri.file(app_file));
    }),
  );
  return { palette };
}

export function deactivate() {}
