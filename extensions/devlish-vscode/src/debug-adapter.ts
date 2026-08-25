import * as vscode from "vscode";
import { DevlishDebugSession } from "./debug-session";

class DevlishInlineDebugAdapterFactory
  implements vscode.DebugAdapterDescriptorFactory
{
  createDebugAdapterDescriptor(
    _session: vscode.DebugSession
  ): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {
    return new vscode.DebugAdapterInlineImplementation(
      new DevlishDebugSession()
    );
  }
}

class DevlishDebugConfigurationProvider
  implements vscode.DebugConfigurationProvider
{
  resolveDebugConfiguration(
    _folder: vscode.WorkspaceFolder | undefined,
    config: vscode.DebugConfiguration,
    _token?: vscode.CancellationToken
  ): vscode.ProviderResult<vscode.DebugConfiguration> {
    // If the user presses F5 with no launch.json, provide defaults
    if (!config.type && !config.request && !config.name) {
      const editor = vscode.window.activeTextEditor;
      if (editor && editor.document.languageId === "devlish") {
        config.type = "devlish";
        config.request = "launch";
        config.name = "Debug Devlish";
        config.program = "${file}";
      }
    }

    if (!config.program) {
      return vscode.window
        .showInformationMessage("Cannot find a .dvl file to debug.")
        .then(() => undefined);
    }

    return config;
  }
}

export function registerDebugAdapter(
  context: vscode.ExtensionContext
): void {
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider(
      "devlish",
      new DevlishDebugConfigurationProvider()
    )
  );

  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory(
      "devlish",
      new DevlishInlineDebugAdapterFactory()
    )
  );
}
