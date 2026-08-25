import { marked } from "marked";
import type { Lesson } from "./manifest";
import { DevlishEditor } from "./editor/DevlishEditor";

declare global {
  interface Window {
    compileAndRun?: (source: string, input?: Record<string, unknown>) => unknown;
  }
}

/** Active editors for the current lesson, cleaned up on re-render. */
let activeEditors: DevlishEditor[] = [];

export function renderLesson(lesson: Lesson, container: HTMLElement): void {
  // Destroy previous editors
  for (const editor of activeEditors) {
    editor.destroy();
  }
  activeEditors = [];

  const html = marked.parse(lesson.markdown, { async: false }) as string;
  container.innerHTML = html;

  // Find all <pre><code> blocks with dvl or text language that match .dvl examples
  const codeBlocks = container.querySelectorAll("pre code");
  const exampleMap = new Map(lesson.examples.map((e) => [e.source.trim(), e]));

  codeBlocks.forEach((codeEl) => {
    const pre = codeEl.parentElement;
    if (!pre) return;

    const source = codeEl.textContent?.trim() || "";

    // Check if this code block matches a known .dvl example or looks like devlish code
    const isDvl =
      codeEl.classList.contains("language-text") ||
      codeEl.classList.contains("language-dvl");

    // Also match blocks whose content matches an inline example
    const matchesExample = exampleMap.has(source);

    if (!isDvl && !matchesExample) return;

    // Only wrap blocks that look like devlish: must contain at least one devlish keyword
    const dvlKeywords =
      /\b(equals|must contain|must be|must equal|must match|Load|Set|If|Otherwise|For each|While|Until|Print|Fail with|Require|Expect|Import|Define|Return|Try)\b/i;
    if (!dvlKeywords.test(source)) return;

    // Determine if this block is from an example file (editable) or inline (read-only)
    const isExample = matchesExample;

    // Replace the <pre> element with a DevlishEditor
    const editorContainer = document.createElement("div");
    pre.parentNode!.insertBefore(editorContainer, pre);
    pre.remove();

    const fullSource = buildSourceWithFixtures(source, lesson);

    const editor = new DevlishEditor(editorContainer, fullSource, {
      readonly: !isExample,
    });

    activeEditors.push(editor);
  });
}

function buildSourceWithFixtures(source: string, _lesson: Lesson): string {
  // For Load statements that reference fixture files, we cannot actually
  // load files in the browser. The source is used as-is since the WASM
  // compile-and-run will handle it. Fixtures are embedded in the manifest
  // for potential future use with a virtual filesystem.
  return source;
}
