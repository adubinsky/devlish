import * as vscode from "vscode";

interface KeywordDoc {
  syntax: string;
  description: string;
}

const KEYWORD_DOCS: Record<string, KeywordDoc> = {
  // Control flow
  If: {
    syntax: "If <condition>\n  <body>\nOtherwise\n  <body>",
    description:
      "Conditional branch. Runs the indented body when the condition is true. Use Otherwise for the else branch.",
  },
  Otherwise: {
    syntax: "If <condition>\n  <body>\nOtherwise\n  <body>",
    description: "Else branch of an If statement. Runs when the condition is false.",
  },
  "For each": {
    syntax: "For each <item> in <list>:\n  <body>",
    description: "Loop over each element in a list. The item variable holds the current element.",
  },
  While: {
    syntax: "While <condition>:\n  <body>",
    description: "Loop that repeats while the condition is true.",
  },
  Until: {
    syntax: "Until <condition>:\n  <body>",
    description: "Loop that repeats until the condition becomes true.",
  },
  Break: {
    syntax: "Break",
    description: "Exit the current loop immediately.",
  },
  Continue: {
    syntax: "Continue",
    description: "Skip to the next iteration of the current loop.",
  },
  Try: {
    syntax: "Try:\n  <body>\nOtherwise:\n  <recovery>",
    description:
      "Error recovery. If the body fails (validation, file error, Fail with), runs the Otherwise block instead of stopping.",
  },

  // I/O
  Print: {
    syntax: "Print <value>",
    description: "Output a value to the console.",
  },
  Ask: {
    syntax: 'Ask "prompt" as <name>',
    description: "Show a prompt and save the user's input as a variable.",
  },
  Load: {
    syntax: 'Load "file.txt" as Document',
    description: "Read a file into context as a named variable.",
  },
  Import: {
    syntax: 'Import "shared_rules.dvl"',
    description:
      "Include another .dvl file at compile time. Searches relative to current file, DEVLISH_PATH, project lib/, and ~/.devlish/lib/.",
  },
  Export: {
    syntax: 'Export <value> to "path.json"',
    description: "Write a value to a file. Records and lists are serialized as JSON.",
  },
  Write: {
    syntax: 'Write <value> to "path.txt"',
    description: "Write text to a file.",
  },
  Append: {
    syntax: 'Append <value> to file "path.txt"',
    description: "Append text to an existing file (or create it).",
  },
  Checkpoint: {
    syntax: 'Checkpoint "Review before continuing"',
    description:
      "Pause execution and return context to the caller. Used for LLM-assisted review points.",
  },

  // Structured output
  Respond: {
    syntax: "Respond with <value>",
    description:
      "Return structured JSON to the caller and stop execution (exit 0). The program author controls the shape.",
  },
  Fail: {
    syntax: 'Fail with "error message"\nFail with record with "error" as status',
    description:
      "Stop execution with an error (exit 1). When given a record, serializes as JSON.",
  },
  Require: {
    syntax: 'Require <condition> otherwise fail with "message"',
    description: "Assert a condition. If false, stops with the given error message.",
  },
  Expect: {
    syntax: 'Expect <value> equals <expected> as "test-id"',
    description: "Test assertion. Records pass/fail without stopping. Use --test for exit code.",
  },

  // File reads
  "Read JSON": {
    syntax: 'Read JSON from "data.json" as <name>',
    description: "Read and parse a JSON file into a variable.",
  },
  "Read CSV": {
    syntax: 'Read CSV from "data.csv" as <name>',
    description: "Read a CSV file into a list of records (one per row, headers as keys).",
  },
  "Read text": {
    syntax: 'Read text from "file.txt" as <name>',
    description: "Read a file as a plain text string.",
  },

  // HTTP
  Get: {
    syntax: 'Get the url at "https://api.example.com" as response',
    description: "HTTP GET request. Response is a record with status, content_type, and body.",
  },
  Post: {
    syntax: 'Post to "https://api.example.com" with payload as response',
    description: "HTTP POST with a JSON body. Response is a record with status, content_type, and body.",
  },
  Put: {
    syntax: 'Put to "https://api.example.com/item" with data as response',
    description: "HTTP PUT with a JSON body.",
  },
  Patch: {
    syntax: 'Patch to "https://api.example.com/item" with data as response',
    description: "HTTP PATCH with a JSON body.",
  },
  Delete: {
    syntax: 'Delete the url at "https://api.example.com/item" as response',
    description: "HTTP DELETE request.",
  },
  Download: {
    syntax: 'Download "https://example.com/file.pdf" to "local.pdf"',
    description: "Download a file from a URL and save it to a local path.",
  },

  // Filesystem
  "Copy file": {
    syntax: 'Copy file from "source" to "destination"',
    description: "Copy a file or directory. Creates parent directories automatically.",
  },
  "Move file": {
    syntax: 'Move file from "source" to "destination"',
    description: "Move or rename a file or directory.",
  },
  "Create directory": {
    syntax: 'Create directory "path"',
    description: "Create a directory (and any missing parents).",
  },
  "Delete file": {
    syntax: 'Delete file "path"',
    description: "Delete a file or directory (recursive).",
  },
  "Check if": {
    syntax: 'Check if "path" exists as <name>',
    description: "Check whether a path exists. Stores true or false.",
  },
  "Get file info": {
    syntax: 'Get file info for "path" as <name>',
    description:
      "Get file metadata: a record with path, type (file/directory/symlink), size (bytes), and modified (Unix timestamp).",
  },
  "List files": {
    syntax: 'List files in "directory" as <name>',
    description: "List filenames in a directory (sorted, not full paths).",
  },
  "Find files matching": {
    syntax: 'Find files matching "*.pdf" in "directory" as <name>',
    description: "Glob pattern search. Returns a sorted list of full paths.",
  },

  // Manifest
  Permissions: {
    syntax: "Permissions:\n  Read files from \"/inbox/\"\n  Write files to \"/output/\"\n  HTTP requests\n  Filesystem operations",
    description:
      "Declare required permissions. When present, the VM enforces them at runtime. Undeclared effects fail with Permission denied.",
  },
  Boundaries: {
    syntax: 'Boundaries:\n  No writes outside "/Users/admin/Dropbox/"',
    description: "Declare resource boundaries that constrain where effects can reach.",
  },
  Callers: {
    syntax: "Callers:\n  Any MCP client",
    description:
      "Declare who can invoke this program. Compiled into bytecode metadata for tooling inspection.",
  },
};

export function registerHoverProvider(
  context: vscode.ExtensionContext
): void {
  const provider = vscode.languages.registerHoverProvider("devlish", {
    provideHover(
      document: vscode.TextDocument,
      position: vscode.Position
    ): vscode.Hover | undefined {
      const line = document.lineAt(position.line).text;
      const trimmed = line.trim();

      // Try multi-word matches first (longest match wins)
      const multiWordKeys = Object.keys(KEYWORD_DOCS)
        .filter((k) => k.includes(" "))
        .sort((a, b) => b.length - a.length);

      for (const key of multiWordKeys) {
        if (trimmed.toLowerCase().startsWith(key.toLowerCase())) {
          const doc = KEYWORD_DOCS[key];
          return makeHover(key, doc);
        }
      }

      // Single-word match at cursor position
      const wordRange = document.getWordRangeAtPosition(position);
      if (!wordRange) {
        return undefined;
      }
      const word = document.getText(wordRange);

      // Check direct match
      const doc = KEYWORD_DOCS[word];
      if (doc) {
        return makeHover(word, doc);
      }

      // Check case-insensitive
      const key = Object.keys(KEYWORD_DOCS).find(
        (k) => k.toLowerCase() === word.toLowerCase()
      );
      if (key) {
        return makeHover(key, KEYWORD_DOCS[key]);
      }

      return undefined;
    },
  });
  context.subscriptions.push(provider);
}

function makeHover(keyword: string, doc: KeywordDoc): vscode.Hover {
  const md = new vscode.MarkdownString();
  md.appendMarkdown(`**${keyword}**\n\n`);
  md.appendMarkdown(`${doc.description}\n\n`);
  md.appendCodeblock(doc.syntax, "devlish");
  return new vscode.Hover(md);
}
