import * as vscode from "vscode";
import { execFile } from "child_process";

export function registerFormatter(context: vscode.ExtensionContext): void {
  const formatter = vscode.languages.registerDocumentFormattingEditProvider(
    "devlish",
    {
      provideDocumentFormattingEdits(
        document: vscode.TextDocument
      ): Promise<vscode.TextEdit[]> {
        return formatDocument(document);
      },
    }
  );
  context.subscriptions.push(formatter);
}

function formatDocument(
  document: vscode.TextDocument
): Promise<vscode.TextEdit[]> {
  return new Promise((resolve) => {
    const config = vscode.workspace.getConfiguration("devlish");
    const cliPath = config.get<string>("cliPath", "") || "devlish-core";
    const filePath = document.uri.fsPath;

    execFile(cliPath, ["fmt", filePath], (error, stdout, _stderr) => {
      if (error && (error as NodeJS.ErrnoException).code === "ENOENT") {
        vscode.window.showWarningMessage(
          `Devlish CLI not found at "${cliPath}". Set devlish.cliPath in settings.`
        );
        resolve([]);
        return;
      }

      if (!stdout) {
        resolve([]);
        return;
      }

      const fullRange = new vscode.Range(
        document.positionAt(0),
        document.positionAt(document.getText().length)
      );
      resolve([vscode.TextEdit.replace(fullRange, stdout)]);
    });
  });
}
