import type { Course, Chapter, Lesson } from "./manifest";

const PROGRESS_KEY = "devlish-course-progress";

export interface Route {
  chapterId: string;
  lessonId: string;
}

export function parseHash(): Route | null {
  const hash = window.location.hash.replace(/^#\/?/, "");
  if (!hash) return null;
  const parts = hash.split("/");
  if (parts.length < 2) return null;
  return { chapterId: parts[0], lessonId: parts[1] };
}

export function buildHash(chapterId: string, lessonId: string): string {
  return `#/${chapterId}/${lessonId}`;
}

export function findLesson(
  course: Course,
  route: Route
): { chapter: Chapter; lesson: Lesson; index: number } | null {
  const chapter = course.chapters.find((c) => c.id === route.chapterId);
  if (!chapter) return null;
  const idx = chapter.lessons.findIndex((l) => l.id === route.lessonId);
  if (idx === -1) return null;
  return { chapter, lesson: chapter.lessons[idx], index: idx };
}

export interface FlatEntry {
  chapterId: string;
  lessonId: string;
  title: string;
}

export function flatLessons(course: Course): FlatEntry[] {
  const entries: FlatEntry[] = [];
  for (const ch of course.chapters) {
    for (const ls of ch.lessons) {
      entries.push({ chapterId: ch.id, lessonId: ls.id, title: ls.title });
    }
  }
  return entries;
}

export function getAdjacentLessons(
  course: Course,
  chapterId: string,
  lessonId: string
): { prev: FlatEntry | null; next: FlatEntry | null } {
  const flat = flatLessons(course);
  const idx = flat.findIndex(
    (e) => e.chapterId === chapterId && e.lessonId === lessonId
  );
  return {
    prev: idx > 0 ? flat[idx - 1] : null,
    next: idx < flat.length - 1 ? flat[idx + 1] : null,
  };
}

// ---- Progress ----

function loadProgress(): Set<string> {
  try {
    const raw = localStorage.getItem(PROGRESS_KEY);
    if (raw) return new Set(JSON.parse(raw));
  } catch {
    // ignore
  }
  return new Set();
}

function saveProgress(set: Set<string>): void {
  localStorage.setItem(PROGRESS_KEY, JSON.stringify([...set]));
}

export function markCompleted(chapterId: string, lessonId: string): void {
  const set = loadProgress();
  set.add(`${chapterId}/${lessonId}`);
  saveProgress(set);
}

export function isCompleted(chapterId: string, lessonId: string): boolean {
  return loadProgress().has(`${chapterId}/${lessonId}`);
}

// ---- Sidebar rendering ----

export function renderSidebar(
  container: HTMLElement,
  course: Course,
  activeRoute: Route | null,
  onNavigate: (chapterId: string, lessonId: string) => void
): void {
  container.innerHTML = "";

  for (const chapter of course.chapters) {
    const group = document.createElement("div");
    group.className = "chapter-group";

    const isActiveChapter =
      activeRoute && activeRoute.chapterId === chapter.id;

    const toggle = document.createElement("button");
    toggle.className = "chapter-toggle";
    toggle.innerHTML = `<span class="arrow ${isActiveChapter ? "open" : ""}">\u25B6</span> ${chapter.title}`;
    group.appendChild(toggle);

    const lessonList = document.createElement("div");
    lessonList.className = `chapter-lessons ${isActiveChapter ? "open" : ""}`;

    for (const lesson of chapter.lessons) {
      const link = document.createElement("a");
      link.className = "lesson-link";
      link.href = buildHash(chapter.id, lesson.id);
      link.textContent = lesson.title;

      if (
        activeRoute &&
        activeRoute.chapterId === chapter.id &&
        activeRoute.lessonId === lesson.id
      ) {
        link.classList.add("active");
      }

      if (isCompleted(chapter.id, lesson.id)) {
        link.classList.add("completed");
      }

      link.addEventListener("click", (e) => {
        e.preventDefault();
        onNavigate(chapter.id, lesson.id);
      });

      lessonList.appendChild(link);
    }

    toggle.addEventListener("click", () => {
      const arrow = toggle.querySelector(".arrow")!;
      arrow.classList.toggle("open");
      lessonList.classList.toggle("open");
    });

    group.appendChild(lessonList);
    container.appendChild(group);
  }
}

export function renderLessonNav(
  container: HTMLElement,
  course: Course,
  chapterId: string,
  lessonId: string,
  onNavigate: (chapterId: string, lessonId: string) => void
): void {
  const { prev, next } = getAdjacentLessons(course, chapterId, lessonId);
  container.innerHTML = "";

  if (prev) {
    const a = document.createElement("a");
    a.href = buildHash(prev.chapterId, prev.lessonId);
    a.textContent = `\u2190 ${prev.title}`;
    a.addEventListener("click", (e) => {
      e.preventDefault();
      onNavigate(prev.chapterId, prev.lessonId);
    });
    container.appendChild(a);
  } else {
    container.appendChild(document.createElement("span"));
  }

  if (next) {
    const a = document.createElement("a");
    a.href = buildHash(next.chapterId, next.lessonId);
    a.textContent = `${next.title} \u2192`;
    a.addEventListener("click", (e) => {
      e.preventDefault();
      onNavigate(next.chapterId, next.lessonId);
    });
    container.appendChild(a);
  }
}
