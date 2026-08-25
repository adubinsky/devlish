import * as vscode from "vscode";

interface CompletionEntry {
  label: string;
  detail: string;
  insertText: string;
  kind: vscode.CompletionItemKind;
}

const COMPLETIONS: CompletionEntry[] = [
  // Control flow
  {
    label: "If",
    detail: "Conditional branch",
    insertText: "If ${1:condition}\n  ${2:body}\nOtherwise\n  ${3:else_body}",
    kind: vscode.CompletionItemKind.Keyword,
  },
  {
    label: "For each",
    detail: "Loop over a list",
    insertText: "For each ${1:item} in ${2:list}:\n  ${3:body}",
    kind: vscode.CompletionItemKind.Keyword,
  },
  {
    label: "While",
    detail: "Loop while condition is true",
    insertText: "While ${1:condition}:\n  ${2:body}",
    kind: vscode.CompletionItemKind.Keyword,
  },
  {
    label: "Until",
    detail: "Loop until condition is true",
    insertText: "Until ${1:condition}:\n  ${2:body}",
    kind: vscode.CompletionItemKind.Keyword,
  },
  {
    label: "Try",
    detail: "Error recovery block",
    insertText: "Try:\n  ${1:body}\nOtherwise:\n  ${2:recovery}",
    kind: vscode.CompletionItemKind.Keyword,
  },
  {
    label: "Break",
    detail: "Exit the current loop",
    insertText: "Break",
    kind: vscode.CompletionItemKind.Keyword,
  },
  {
    label: "Continue",
    detail: "Skip to next loop iteration",
    insertText: "Continue",
    kind: vscode.CompletionItemKind.Keyword,
  },

  // I/O
  {
    label: "Print",
    detail: "Output a value",
    insertText: "Print ${1:value}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Ask",
    detail: "Prompt for input",
    insertText: 'Ask "${1:prompt}" as ${2:name}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Load",
    detail: "Load a file into context",
    insertText: 'Load "${1:file.txt}" as ${2:name}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Import",
    detail: "Include another .dvl file",
    insertText: 'Import "${1:shared.dvl}"',
    kind: vscode.CompletionItemKind.Module,
  },
  {
    label: "Export",
    detail: "Write value to file",
    insertText: 'Export ${1:value} to "${2:path}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Append",
    detail: "Append text to a file",
    insertText: 'Append ${1:value} to file "${2:path}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Read JSON from",
    detail: "Parse a JSON file",
    insertText: 'Read JSON from "${1:data.json}" as ${2:name}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Read CSV from",
    detail: "Parse a CSV file into records",
    insertText: 'Read CSV from "${1:data.csv}" as ${2:name}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Read text from",
    detail: "Read a file as plain text",
    insertText: 'Read text from "${1:file.txt}" as ${2:name}',
    kind: vscode.CompletionItemKind.Function,
  },

  // Structured output
  {
    label: "Respond with",
    detail: "Return JSON to caller (exit 0)",
    insertText: "Respond with ${1:value}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Fail with",
    detail: "Stop with error (exit 1)",
    insertText: 'Fail with "${1:error message}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Checkpoint",
    detail: "Pause for LLM review",
    insertText: 'Checkpoint "${1:Review before continuing}"',
    kind: vscode.CompletionItemKind.Function,
  },

  // Validation
  {
    label: "Require",
    detail: "Assert a condition",
    insertText:
      'Require ${1:condition} otherwise fail with "${2:message}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Expect",
    detail: "Test assertion",
    insertText: 'Expect ${1:value} equals ${2:expected} as "${3:test-id}"',
    kind: vscode.CompletionItemKind.Function,
  },

  // HTTP
  {
    label: "Get the url at",
    detail: "HTTP GET request",
    insertText: 'Get the url at "${1:url}" as ${2:response}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Post to",
    detail: "HTTP POST with body",
    insertText: 'Post to "${1:url}" with ${2:body} as ${3:response}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Put to",
    detail: "HTTP PUT with body",
    insertText: 'Put to "${1:url}" with ${2:body} as ${3:response}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Patch to",
    detail: "HTTP PATCH with body",
    insertText: 'Patch to "${1:url}" with ${2:body} as ${3:response}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Delete the url at",
    detail: "HTTP DELETE request",
    insertText: 'Delete the url at "${1:url}" as ${2:response}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Download",
    detail: "Download a file from URL",
    insertText: 'Download "${1:url}" to "${2:local_path}"',
    kind: vscode.CompletionItemKind.Function,
  },

  // Filesystem
  {
    label: "Copy file from",
    detail: "Copy a file or directory",
    insertText: 'Copy file from "${1:source}" to "${2:destination}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Move file from",
    detail: "Move or rename a file",
    insertText: 'Move file from "${1:source}" to "${2:destination}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Create directory",
    detail: "Create a directory (recursive)",
    insertText: 'Create directory "${1:path}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Delete file",
    detail: "Delete a file or directory",
    insertText: 'Delete file "${1:path}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Check if",
    detail: "Check if a path exists",
    insertText: 'Check if "${1:path}" exists as ${2:found}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Get file info for",
    detail: "Get file metadata (size, type, modified)",
    insertText: 'Get file info for "${1:path}" as ${2:info}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "List files in",
    detail: "List directory contents",
    insertText: 'List files in "${1:directory}" as ${2:entries}',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "Find files matching",
    detail: "Glob pattern search",
    insertText:
      'Find files matching "${1:*.pdf}" in "${2:directory}" as ${3:files}',
    kind: vscode.CompletionItemKind.Function,
  },

  // Manifest
  {
    label: "Permissions:",
    detail: "Declare required permissions",
    insertText:
      "Permissions:\n  ${1:Read files}\n  ${2:Write files}\n  ${3:Filesystem operations}",
    kind: vscode.CompletionItemKind.Struct,
  },
  {
    label: "Boundaries:",
    detail: "Declare resource boundaries",
    insertText: 'Boundaries:\n  No writes outside "${1:/path}"',
    kind: vscode.CompletionItemKind.Struct,
  },
  {
    label: "Callers:",
    detail: "Declare allowed callers",
    insertText: "Callers:\n  ${1:Any MCP client}",
    kind: vscode.CompletionItemKind.Struct,
  },

  // Data
  {
    label: "record with",
    detail: "Create a record",
    insertText: 'record with ${1:value} as ${2:key} and ${3:value2} as ${4:key2}',
    kind: vscode.CompletionItemKind.Constructor,
  },
  {
    label: "list of",
    detail: "Create a list",
    insertText: 'list of ${1:"a"}, ${2:"b"}, ${3:"c"}',
    kind: vscode.CompletionItemKind.Constructor,
  },

  // Builtins
  {
    label: "count of",
    detail: "Count elements in a list",
    insertText: "count of ${1:list}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "first of",
    detail: "First element of a list",
    insertText: "first of ${1:list}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "last of",
    detail: "Last element of a list",
    insertText: "last of ${1:list}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "sort",
    detail: "Sort a list",
    insertText: "sort ${1:list}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "reverse",
    detail: "Reverse a list",
    insertText: "reverse ${1:list}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "unique",
    detail: "Remove duplicates from a list",
    insertText: "unique ${1:list}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "filter",
    detail: "Filter a list by condition",
    insertText: "filter ${1:list} where ${2:condition}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "reject",
    detail: "Remove items matching condition",
    insertText: "reject ${1:list} where ${2:condition}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "map",
    detail: "Transform each item in a list",
    insertText: "map ${1:list} to ${2:transform}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "split",
    detail: "Split a string by delimiter",
    insertText: 'split ${1:text} by "${2:delimiter}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "join",
    detail: "Join a list into a string",
    insertText: 'join ${1:list} with "${2:separator}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "replace",
    detail: "Replace text in a string",
    insertText: 'replace "${1:needle}" in ${2:haystack} with "${3:replacement}"',
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "uppercase",
    detail: "Convert text to uppercase",
    insertText: "uppercase ${1:text}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "lowercase",
    detail: "Convert text to lowercase",
    insertText: "lowercase ${1:text}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "trim",
    detail: "Remove leading/trailing whitespace",
    insertText: "trim ${1:text}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "type of",
    detail: "Get the type of a value",
    insertText: "type of ${1:value}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "keys of",
    detail: "Get record keys as a list",
    insertText: "keys of ${1:record}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "values of",
    detail: "Get record values as a list",
    insertText: "values of ${1:record}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "sum of",
    detail: "Sum numbers in a list",
    insertText: "sum of ${1:list}",
    kind: vscode.CompletionItemKind.Function,
  },
  {
    label: "average of",
    detail: "Average numbers in a list",
    insertText: "average of ${1:list}",
    kind: vscode.CompletionItemKind.Function,
  },
];

export function registerCompletionProvider(
  context: vscode.ExtensionContext
): void {
  const provider = vscode.languages.registerCompletionItemProvider(
    "devlish",
    {
      provideCompletionItems(
        _document: vscode.TextDocument,
        _position: vscode.Position
      ): vscode.CompletionItem[] {
        return COMPLETIONS.map((entry) => {
          const item = new vscode.CompletionItem(
            entry.label,
            entry.kind
          );
          item.detail = entry.detail;
          item.insertText = new vscode.SnippetString(entry.insertText);
          item.sortText = entry.label.toLowerCase();
          return item;
        });
      },
    },
    // Trigger on first letter of any line
    ...'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz'.split('')
  );
  context.subscriptions.push(provider);
}
