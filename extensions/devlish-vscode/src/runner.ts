import * as vscode from "vscode";
import { execFile, ChildProcess } from "child_process";

let outputChannel: vscode.OutputChannel;
let statusBarItem: vscode.StatusBarItem;
let runningProcess: ChildProcess | undefined;

function getCliPath(): string {
  const configured = vscode.workspace
    .getConfiguration("devlish")
    .get<string>("cliPath");
  if (configured && configured.trim()) {
    return configured.trim();
  }
  return "devlish-core";
}

export function registerRunner(context: vscode.ExtensionContext): void {
  outputChannel = vscode.window.createOutputChannel("Devlish");
  context.subscriptions.push(outputChannel);

  statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100
  );
  statusBarItem.command = "devlish.runFile";
  context.subscriptions.push(statusBarItem);

  const command = vscode.commands.registerCommand("devlish.runFile", () => {
    if (runningProcess) {
      // Second press cancels the running process
      runningProcess.kill();
      runningProcess = undefined;
      outputChannel.appendLine("\n[cancelled]");
      statusBarItem.hide();
      return;
    }

    const editor = vscode.window.activeTextEditor;
    if (!editor) {
      vscode.window.showWarningMessage("No active editor.");
      return;
    }

    const doc = editor.document;
    if (doc.languageId !== "devlish" && !doc.fileName.endsWith(".dvl")) {
      vscode.window.showWarningMessage(
        "Active file is not a Devlish (.dvl) file."
      );
      return;
    }

    // Save the file before running
    doc.save().then((saved) => {
      if (!saved) {
        return;
      }
      runFile(doc.fileName);
    });
  });

  context.subscriptions.push(command);
}

function runFile(filePath: string): void {
  const cliPath = getCliPath();

  outputChannel.clear();
  outputChannel.show(true);
  outputChannel.appendLine(`> ${cliPath} run ${filePath}`);
  outputChannel.appendLine("");

  statusBarItem.text = "$(sync~spin) Devlish: Running...";
  statusBarItem.show();

  const startTime = Date.now();

  runningProcess = execFile(
    cliPath,
    ["run", filePath],
    { maxBuffer: 10 * 1024 * 1024, timeout: 60_000 },
    (error, stdout, stderr) => {
      runningProcess = undefined;
      const elapsed = Date.now() - startTime;

      // Show stderr events (host effects, diagnostics) if present
      if (stderr && stderr.trim()) {
        for (const line of stderr.split("\n")) {
          if (!line.trim()) {
            continue;
          }
          outputChannel.appendLine(line);
        }
        outputChannel.appendLine("");
      }

      if (error && !stdout) {
        // Binary not found or process error
        if ((error as NodeJS.ErrnoException).code === "ENOENT") {
          outputChannel.appendLine(
            `[error] devlish-core binary not found at "${cliPath}".`
          );
          outputChannel.appendLine(
            "Set the path in Settings > Devlish > CLI Path, or ensure devlish-core is on your PATH."
          );
        } else {
          outputChannel.appendLine(`[error] ${error.message}`);
        }
        statusBarItem.text = "$(error) Devlish: Error";
        setTimeout(() => statusBarItem.hide(), 5000);
        return;
      }

      // Parse the JSON result from stdout
      if (stdout && stdout.trim()) {
        try {
          const result = JSON.parse(stdout);
          formatResult(result);
        } catch {
          // Not JSON; show raw output
          outputChannel.appendLine(stdout);
        }
      }

      if (error) {
        // Process exited non-zero (runtime or compile error)
        statusBarItem.text = "$(error) Devlish: Failed";
      } else {
        statusBarItem.text = "$(check) Devlish: Done";
      }

      outputChannel.appendLine(`\n[completed in ${elapsed}ms]`);
      setTimeout(() => statusBarItem.hide(), 5000);
    }
  );
}

function formatResult(result: Record<string, unknown>): void {
  const success = result.success as boolean | undefined;
  const errorMsg = result.error as string | undefined;
  const results = result.results as Record<string, unknown> | undefined;
  const context = result.context as Record<string, unknown> | undefined;

  if (success === false && errorMsg) {
    outputChannel.appendLine(`[error] ${errorMsg}`);
    return;
  }

  // Show print output if present
  if (results) {
    const prints = results.prints as string[] | undefined;
    if (prints && Array.isArray(prints) && prints.length > 0) {
      for (const line of prints) {
        outputChannel.appendLine(String(line));
      }
      outputChannel.appendLine("");
    }

    // Show checkpoints
    const checkpoints = results.checkpoints as unknown[] | undefined;
    if (checkpoints && Array.isArray(checkpoints) && checkpoints.length > 0) {
      outputChannel.appendLine("--- Checkpoints ---");
      for (const cp of checkpoints) {
        outputChannel.appendLine(`  ${JSON.stringify(cp)}`);
      }
      outputChannel.appendLine("");
    }

    // Show assertions
    const assertions = results.assertions as Array<Record<string, unknown>> | undefined;
    if (assertions && Array.isArray(assertions) && assertions.length > 0) {
      outputChannel.appendLine("--- Assertions ---");
      for (const a of assertions) {
        const status = a.passed ? "PASS" : "FAIL";
        const msg = a.message || a.description || "";
        outputChannel.appendLine(`  [${status}] ${msg}`);
      }
      outputChannel.appendLine("");
    }

    // Show saved values (remaining keys)
    const skipKeys = new Set(["prints", "checkpoints", "assertions"]);
    const savedEntries = Object.entries(results).filter(
      ([key]) => !skipKeys.has(key)
    );
    if (savedEntries.length > 0) {
      outputChannel.appendLine("--- Results ---");
      for (const [key, value] of savedEntries) {
        outputChannel.appendLine(`  ${key}: ${JSON.stringify(value)}`);
      }
      outputChannel.appendLine("");
    }
  }

  // Show context values
  if (context && Object.keys(context).length > 0) {
    outputChannel.appendLine("--- Context ---");
    for (const [key, value] of Object.entries(context)) {
      outputChannel.appendLine(`  ${key}: ${JSON.stringify(value)}`);
    }
    outputChannel.appendLine("");
  }
}
