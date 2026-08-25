import * as vscode from "vscode";
import { execFile } from "child_process";

interface LintDiagnostic {
  line: number;
  severity?: string;
  message: string;
  source_text: string;
}

interface LintOutput {
  file: string;
  valid: boolean;
  diagnostics: LintDiagnostic[];
}

let warnedAboutMissingCli = false;

export function registerLinter(context: vscode.ExtensionContext): void {
  const diagnosticCollection =
    vscode.languages.createDiagnosticCollection("devlish");
  context.subscriptions.push(diagnosticCollection);

  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument((document) => {
      if (document.languageId !== "devlish") {
        return;
      }
      const config = vscode.workspace.getConfiguration("devlish");
      if (!config.get<boolean>("lintOnSave", true)) {
        return;
      }
      lintDocument(document, diagnosticCollection);
    })
  );

  context.subscriptions.push(
    vscode.workspace.onDidCloseTextDocument((document) => {
      diagnosticCollection.delete(document.uri);
    })
  );
}

function lintDocument(
  document: vscode.TextDocument,
  diagnosticCollection: vscode.DiagnosticCollection
): void {
  const config = vscode.workspace.getConfiguration("devlish");
  const cliPath = config.get<string>("cliPath", "") || "devlish-core";
  const filePath = document.uri.fsPath;

  execFile(cliPath, ["lint", filePath, "--json"], (error, stdout, stderr) => {
    // If the binary was not found, warn once and bail out
    if (error && (error as NodeJS.ErrnoException).code === "ENOENT") {
      if (!warnedAboutMissingCli) {
        warnedAboutMissingCli = true;
        vscode.window.showWarningMessage(
          `Devlish CLI not found at "${cliPath}". Set devlish.cliPath in settings or add devlish-core to your PATH.`
        );
      }
      return;
    }

    // The CLI exits non-zero on lint errors but still writes valid JSON to
    // stdout, so we parse stdout regardless of exit code.
    const output = stdout.trim();
    if (!output) {
      return;
    }

    let result: LintOutput;
    try {
      result = JSON.parse(output) as LintOutput;
    } catch {
      return;
    }

    const diagnostics: vscode.Diagnostic[] = result.diagnostics.map((d) => {
      const line = Math.max(0, d.line - 1); // 1-indexed to 0-indexed
      const range = new vscode.Range(line, 0, line, Number.MAX_SAFE_INTEGER);
      const severity =
        d.severity === "warning"
          ? vscode.DiagnosticSeverity.Warning
          : vscode.DiagnosticSeverity.Error;
      const diagnostic = new vscode.Diagnostic(range, d.message, severity);
      diagnostic.source = "devlish";
      return diagnostic;
    });

    diagnosticCollection.set(document.uri, diagnostics);
  });
}
