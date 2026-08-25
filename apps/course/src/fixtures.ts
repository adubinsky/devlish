/**
 * Fixture management for bundled .txt files used by Load statements.
 * Fixtures are embedded in the course manifest at build time.
 */

import { getCourse } from "./manifest";

let fixtureMap: Map<string, string> | null = null;

function ensureLoaded(): Map<string, string> {
  if (fixtureMap) return fixtureMap;

  fixtureMap = new Map();
  const course = getCourse();

  for (const chapter of course.chapters) {
    for (const lesson of chapter.lessons) {
      for (const fixture of lesson.fixtures) {
        // Store under both the bare filename and a chapter-qualified path
        fixtureMap.set(fixture.filename, fixture.content);
        fixtureMap.set(
          `${chapter.id}/examples/${fixture.filename}`,
          fixture.content
        );
      }
    }
  }

  return fixtureMap;
}

/**
 * Look up a fixture by filename or path. Returns null if not found.
 */
export function getFixture(path: string): string | null {
  const map = ensureLoaded();

  // Try exact match first
  if (map.has(path)) return map.get(path)!;

  // Try just the basename
  const basename = path.split("/").pop() || path;
  if (map.has(basename)) return map.get(basename)!;

  return null;
}

/**
 * Return the full fixtures map (filename -> content).
 */
export function getAllFixtures(): Map<string, string> {
  return ensureLoaded();
}
