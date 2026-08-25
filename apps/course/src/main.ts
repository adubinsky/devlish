import { getCourse } from "./manifest";
import type { Course } from "./manifest";
import {
  parseHash,
  buildHash,
  renderSidebar,
  renderLessonNav,
  findLesson,
  markCompleted,
} from "./navigation";
import { renderLesson } from "./lesson-renderer";
import { createBrowserHost } from "./wasm-host";
import { getAllFixtures } from "./fixtures";
import "./styles.css";

let course: Course;
let devlish: {
  compileAndRun: (source: string, input?: Record<string, unknown>, host?: unknown) => unknown;
} | null = null;

async function initWasm(): Promise<void> {
  // The WASM modules are loaded dynamically. For now, we attempt to load
  // them from a well-known path. If they are not available, the Run buttons
  // will show a message.
  try {
    const { loadDevlishFull } = await import(
      /* @vite-ignore */ "/wasm/compiler.mjs"
    );
    devlish = await loadDevlishFull({
      compilerWasmUrl: "/wasm/devlish_compiler.wasm",
      runnerWasmUrl: "/wasm/devlish_runner.wasm",
    });
    window.compileAndRun = (source: string, input?: Record<string, unknown>) => {
      if (!devlish) throw new Error("WASM not loaded");
      const host = createBrowserHost();
      return devlish.compileAndRun(source, input || {}, host);
    };
  } catch {
    console.warn(
      "Devlish WASM modules not found. Run buttons will be disabled until WASM files are placed in public/wasm/."
    );
  }

  // Pre-warm the fixture map so it is ready when Load statements execute
  getAllFixtures();
}

function navigate(chapterId: string, lessonId: string): void {
  window.location.hash = buildHash(chapterId, lessonId);
}

function render(): void {
  const route = parseHash();
  const sidebarNav = document.getElementById("sidebar-nav")!;
  const lessonContainer = document.getElementById("lesson")!;
  const lessonNav = document.getElementById("lesson-nav")!;

  // If no route, navigate to the first lesson
  if (!route) {
    const first = course.chapters[0]?.lessons[0];
    if (first) {
      navigate(course.chapters[0].id, first.id);
    }
    return;
  }

  const found = findLesson(course, route);
  if (!found) {
    lessonContainer.innerHTML = "<p>Lesson not found.</p>";
    lessonNav.innerHTML = "";
    renderSidebar(sidebarNav, course, route, navigate);
    return;
  }

  renderSidebar(sidebarNav, course, route, navigate);
  renderLesson(found.lesson, lessonContainer);
  renderLessonNav(lessonNav, course, route.chapterId, route.lessonId, navigate);

  // Mark as completed after viewing
  markCompleted(route.chapterId, route.lessonId);

  // Scroll to top
  window.scrollTo(0, 0);

  // Close sidebar on mobile after navigation
  document.getElementById("sidebar")?.classList.remove("open");
}

function init(): void {
  course = getCourse();

  // Sidebar toggle for mobile
  const toggleBtn = document.getElementById("sidebar-toggle");
  const sidebar = document.getElementById("sidebar");
  if (toggleBtn && sidebar) {
    toggleBtn.addEventListener("click", () => {
      sidebar.classList.toggle("open");
    });
  }

  window.addEventListener("hashchange", render);
  render();
  initWasm();
}

init();
