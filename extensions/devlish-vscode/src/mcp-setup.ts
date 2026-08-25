/**
 * Configure the Devlish MCP server for the user's AI tools (Claude Desktop,
 * Claude Code, Cursor). The server is built into the devlish-core CLI and
 * started with `devlish-core mcp`.
 */
import * as fs from "fs/promises";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";

interface Target {
  name: string;
  configPath: string;
  detectPath?: string;
  key: string;
  entryKey: string;
}

function getTargets(): Target[] {
  const home = os.homedir();
  const p = os.platform();

  const claudeDesktop =
    p === "darwin"
      ? path.join(
          home,
          "Library",
          "Application Support",
          "Claude",
          "claude_desktop_config.json",
        )
      : p === "win32"
        ? path.join(
            home,
            "AppData",
            "Roaming",
            "Claude",
            "claude_desktop_config.json",
          )
        : path.join(home, ".config", "Claude", "claude_desktop_config.json");

  return [
    {
      name: "Claude Desktop",
      configPath: claudeDesktop,
      key: "mcpServers",
      entryKey: "devlish",
    },
    {
      name: "Claude Code",
      configPath: path.join(home, ".claude.json"),
      detectPath: path.join(home, ".claude"),
      key: "mcpServers",
      entryKey: "devlish",
    },
    {
      name: "Cursor",
      configPath: path.join(home, ".cursor", "mcp.json"),
      detectPath: path.join(home, ".cursor"),
      key: "mcpServers",
      entryKey: "devlish",
    },
  ];
}

function mcpEntry(): Record<string, unknown> {
  const cliPath =
    vscode.workspace.getConfiguration("devlish").get<string>("cliPath") || "";
  const command = cliPath || "devlish-core";
  return { type: "stdio", command, args: ["mcp"] };
}

async function exists(p: string): Promise<boolean> {
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

export async function detectInstalled(): Promise<Target[]> {
  const found: Target[] = [];
  for (const t of getTargets()) {
    if (await exists(t.detectPath || path.dirname(t.configPath))) {
      found.push(t);
    }
  }
  return found;
}

async function addToConfig(t: Target): Promise<void> {
  let config: Record<string, Record<string, unknown>> = {};
  try {
    config = JSON.parse(await fs.readFile(t.configPath, "utf-8"));
  } catch {
    /* fresh or invalid config, start clean */
  }
  if (!config[t.key]) {
    config[t.key] = {};
  }
  config[t.key][t.entryKey] = mcpEntry();
  await fs.mkdir(path.dirname(t.configPath), { recursive: true });
  await fs.writeFile(
    t.configPath,
    JSON.stringify(config, null, 2) + "\n",
    "utf-8",
  );
}

export async function setupMcp(): Promise<void> {
  const installed = await detectInstalled();
  if (installed.length === 0) {
    vscode.window.showWarningMessage(
      "No supported AI tool found (Claude Desktop, Claude Code, or Cursor). " +
        'Install one, then run "Devlish: Set up MCP for Claude / Cursor" again.',
    );
    return;
  }

  let targets: Target[];
  if (installed.length === 1) {
    targets = installed;
  } else {
    const picks = await vscode.window.showQuickPick(
      installed.map((t) => ({ label: t.name, target: t })),
      {
        canPickMany: true,
        title: "Configure Devlish MCP server for:",
        placeHolder: "Select one or more AI tools",
      },
    );
    if (!picks || picks.length === 0) {
      return;
    }
    targets = picks.map((p) => p.target);
  }

  const updated: string[] = [];
  for (const t of targets) {
    try {
      await addToConfig(t);
      updated.push(`${t.name} (${t.configPath})`);
    } catch (err) {
      vscode.window.showErrorMessage(
        `Failed to update ${t.name}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  if (updated.length > 0) {
    vscode.window.showInformationMessage(
      `Devlish MCP configured for ${updated.join(", ")}. Restart the tool to pick up the change.`,
    );
  }
}
