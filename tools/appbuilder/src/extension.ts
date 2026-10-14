// The NeoTime App Builder: commands to open the builder on an app and to start a new app.

import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";

import { BuilderPanel } from "./panel";
import { find_neotime_folder } from "./session";
import { is_app_id, starter_app } from "./starter";

export function activate(context: vscode.ExtensionContext) {
  const diagnostics = vscode.languages.createDiagnosticCollection("neotime-appbuilder");
  context.subscriptions.push(diagnostics);

  context.subscriptions.push(
    vscode.commands.registerCommand("neotime.appBuilder.open", (clicked?: vscode.Uri) => {
      const app = clicked ?? vscode.window.activeTextEditor?.document.uri;
      if (!app || !app.fsPath.endsWith(".tsx")) {
        void vscode.window.showErrorMessage("Open an app's .tsx file first, such as apps/hello/app.tsx.");
        return;
      }
      const neotime_folder = find_neotime_folder(path.dirname(app.fsPath));
      if (!neotime_folder) {
        void vscode.window.showErrorMessage("This file is not inside a NeoTime checkout, so there is no compiler or simulator to run it.");
        return;
      }
      BuilderPanel.show(context, app, neotime_folder, diagnostics);
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
      const app = vscode.Uri.file(app_file);
      await vscode.window.showTextDocument(app, { viewColumn: vscode.ViewColumn.One });
      BuilderPanel.show(context, app, neotime_folder, diagnostics);
    }),
  );
}

export function deactivate() {}
