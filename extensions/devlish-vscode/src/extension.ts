import * as vscode from "vscode";
import { registerLinter } from "./linter";
import { registerRunner } from "./runner";
import { registerDebugAdapter } from "./debug-adapter";
import { registerFormatter } from "./formatter";
import { registerHoverProvider } from "./hover";
import { registerCompletionProvider } from "./completion";
import { setupMcp } from "./mcp-setup";

export function activate(context: vscode.ExtensionContext): void {
  console.log("Devlish language support is now active.");
  registerLinter(context);
  registerRunner(context);
  registerDebugAdapter(context);
  registerFormatter(context);
  registerHoverProvider(context);
  registerCompletionProvider(context);

  context.subscriptions.push(
    vscode.commands.registerCommand("devlish.setupMcp", () => setupMcp()),
  );
}

export function deactivate(): void {}
