export class OutputPanel {
  readonly dom: HTMLElement;
  private contentEl: HTMLElement;

  constructor() {
    this.dom = document.createElement("div");
    this.dom.className = "dvl-editor-output";
    this.dom.style.display = "none";

    this.contentEl = document.createElement("pre");
    this.contentEl.className = "dvl-editor-output-content";
    this.dom.appendChild(this.contentEl);
  }

  clear(): void {
    this.contentEl.textContent = "";
    this.contentEl.className = "dvl-editor-output-content";
    this.dom.style.display = "none";
  }

  showOutput(lines: string[]): void {
    this.contentEl.className = "dvl-editor-output-content";
    this.contentEl.textContent = lines.join("\n") || "Program completed successfully.";
    this.dom.style.display = "block";
  }

  showError(message: string, _line?: number): void {
    this.contentEl.className = "dvl-editor-output-content error";
    this.contentEl.textContent = message;
    this.dom.style.display = "block";
  }

  showSuccess(message: string): void {
    this.contentEl.className = "dvl-editor-output-content success";
    this.contentEl.textContent = message;
    this.dom.style.display = "block";
  }
}
