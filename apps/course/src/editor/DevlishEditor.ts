import { EditorView, keymap, lineNumbers, highlightActiveLine } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import {
  syntaxHighlighting,
  defaultHighlightStyle,
  bracketMatching,
} from "@codemirror/language";
import { oneDark } from "@codemirror/theme-one-dark";
import { devlishLanguageDef } from "./devlish-language";
import { OutputPanel } from "./output-panel";

export interface DevlishEditorOptions {
  readonly?: boolean;
  onRun?: (source: string) => void;
}

export class DevlishEditor {
  readonly dom: HTMLElement;
  private editorView: EditorView;
  private outputPanel: OutputPanel;
  private originalSource: string;

  constructor(
    container: HTMLElement,
    source: string,
    options: DevlishEditorOptions = {}
  ) {
    this.originalSource = source;
    this.dom = document.createElement("div");
    this.dom.className = "dvl-editor-widget";
    container.appendChild(this.dom);

    // Toolbar
    const toolbar = document.createElement("div");
    toolbar.className = "dvl-editor-toolbar";

    const runBtn = document.createElement("button");
    runBtn.className = "dvl-editor-run-btn";
    runBtn.innerHTML = "&#9654; Run";
    runBtn.addEventListener("click", () => this.run(options.onRun));
    toolbar.appendChild(runBtn);

    if (!options.readonly) {
      const resetBtn = document.createElement("button");
      resetBtn.className = "dvl-editor-reset-btn";
      resetBtn.textContent = "Reset";
      resetBtn.addEventListener("click", () => this.reset());
      toolbar.appendChild(resetBtn);
    }

    this.dom.appendChild(toolbar);

    // Editor
    const editorContainer = document.createElement("div");
    editorContainer.className = "dvl-editor-cm";
    this.dom.appendChild(editorContainer);

    const extensions = [
      lineNumbers(),
      history(),
      bracketMatching(),
      highlightActiveLine(),
      syntaxHighlighting(defaultHighlightStyle),
      oneDark,
      devlishLanguageDef,
      keymap.of([...defaultKeymap, ...historyKeymap]),
      EditorView.lineWrapping,
    ];

    if (options.readonly) {
      extensions.push(EditorState.readOnly.of(true));
      extensions.push(EditorView.editable.of(false));
    }

    this.editorView = new EditorView({
      state: EditorState.create({
        doc: source,
        extensions,
      }),
      parent: editorContainer,
    });

    // Output panel
    this.outputPanel = new OutputPanel();
    this.dom.appendChild(this.outputPanel.dom);
  }

  getSource(): string {
    return this.editorView.state.doc.toString();
  }

  reset(): void {
    this.editorView.dispatch({
      changes: {
        from: 0,
        to: this.editorView.state.doc.length,
        insert: this.originalSource,
      },
    });
    this.outputPanel.clear();
  }

  private run(onRun?: (source: string) => void): void {
    const source = this.getSource();
    this.outputPanel.clear();

    if (onRun) {
      onRun(source);
      return;
    }

    if (!window.compileAndRun) {
      this.outputPanel.showError("WASM runtime not loaded. Place WASM files in public/wasm/.");
      return;
    }

    try {
      const result = window.compileAndRun(source, {}) as Record<string, unknown>;

      if (result && (result as { success?: boolean }).success === false) {
        const diagnostics = (result as { diagnostics?: Array<{ message: string; line?: number }> }).diagnostics;
        if (diagnostics && diagnostics.length > 0) {
          const msg = diagnostics.map((d) => d.message).join("\n");
          const firstLine = diagnostics[0]?.line;
          this.outputPanel.showError(msg, firstLine);
        } else {
          this.outputPanel.showError(JSON.stringify(result, null, 2));
        }
        return;
      }

      const events = (result as { events?: Array<Record<string, unknown>> }).events;
      if (events && events.length > 0) {
        const lines = events.map((e) => formatEvent(e));
        this.outputPanel.showOutput(lines);
      } else {
        this.outputPanel.showSuccess("Program completed successfully.");
      }
    } catch (err) {
      this.outputPanel.showError(String(err));
    }
  }

  destroy(): void {
    this.editorView.destroy();
    this.dom.remove();
  }
}

function formatEvent(event: Record<string, unknown>): string {
  const type = event.type as string;
  switch (type) {
    case "print":
      return String(event.value ?? "");
    case "validation_pass":
      return `PASS: ${event.message ?? event.description ?? ""}`;
    case "validation_fail":
      return `FAIL: ${event.message ?? event.description ?? ""}`;
    case "binding":
      return `${event.name} = ${JSON.stringify(event.value)}`;
    case "assertion_pass":
      return `PASS: ${event.description ?? ""}`;
    case "assertion_fail":
      return `FAIL: ${event.description ?? ""}`;
    default:
      return JSON.stringify(event);
  }
}
