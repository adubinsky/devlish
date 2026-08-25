#!/usr/bin/env node

import { readdir, readFile, stat } from "node:fs/promises";
import { writeFile } from "node:fs/promises";
import { join, basename, extname } from "node:path";

const COURSE_DIR = join(import.meta.dirname, "..", "..", "..", "docs", "course");
const OUTPUT = join(import.meta.dirname, "..", "src", "course-data.json");

const CHAPTER_ORDER = [
  "00-getting-started",
  "01-values-and-names",
  "02-decisions-and-logic",
  "03-repetition-and-collections",
  "04-methods-and-classes",
  "05-real-programs",
  "06-testing-and-debugging",
  "projects",
];

// Heading patterns that indicate an exercise section
const EXERCISE_HEADING_RE =
  /^#{1,4}\s+(.*(?:Exercise|Practice|Try it|Modify|Challenge).*)$/i;

async function main() {
  const chapters = [];

  for (const dirName of CHAPTER_ORDER) {
    const dirPath = join(COURSE_DIR, dirName);
    const dirStat = await stat(dirPath).catch(() => null);
    if (!dirStat || !dirStat.isDirectory()) {
      console.warn(`Skipping missing directory: ${dirName}`);
      continue;
    }

    const chapter = await buildChapter(dirPath, dirName);
    chapters.push(chapter);
  }

  const course = {
    title: "Devlish Course",
    chapters,
  };

  await writeFile(OUTPUT, JSON.stringify(course, null, 2), "utf8");
  console.log(`Wrote ${OUTPUT}`);
  console.log(
    `  ${chapters.length} chapters, ${chapters.reduce((n, c) => n + c.lessons.length, 0)} lessons`
  );

  const totalExercises = chapters.reduce(
    (n, c) => n + c.lessons.reduce((m, l) => m + l.exercises.length, 0),
    0
  );
  const totalScenarios = chapters.reduce(
    (n, c) => n + c.lessons.reduce((m, l) => m + l.dvtScenarios.length, 0),
    0
  );
  console.log(`  ${totalExercises} exercises, ${totalScenarios} dvt scenarios`);
}

async function buildChapter(dirPath, dirName) {
  // Read README.md for title and description
  const readmePath = join(dirPath, "README.md");
  const readmeText = await readFile(readmePath, "utf8").catch(() => "");
  const title = extractTitle(readmeText) || formatDirName(dirName);
  const description = extractDescription(readmeText);

  // Find lesson markdown files (numbered, not README)
  const entries = await readdir(dirPath);
  const lessonFiles = entries
    .filter((f) => /^\d+.*\.md$/.test(f) && f !== "README.md")
    .sort();

  // Read examples
  const examplesDir = join(dirPath, "examples");
  const exampleFiles = await readdir(examplesDir).catch(() => []);
  const dvlFiles = exampleFiles.filter((f) => f.endsWith(".dvl")).sort();
  const txtFiles = exampleFiles.filter((f) => f.endsWith(".txt")).sort();

  // Read all .dvl sources
  const examples = [];
  for (const f of dvlFiles) {
    const source = await readFile(join(examplesDir, f), "utf8");
    examples.push({ filename: f, source });
  }

  // Read all .txt fixtures
  const fixtures = [];
  for (const f of txtFiles) {
    const content = await readFile(join(examplesDir, f), "utf8");
    fixtures.push({ filename: f, content });
  }

  // Read .dvt check files
  const checksDir = join(dirPath, "checks");
  const checkFiles = await readdir(checksDir).catch(() => []);
  const dvtFiles = checkFiles.filter((f) => f.endsWith(".dvt")).sort();

  const dvtScenarios = [];
  for (const f of dvtFiles) {
    const content = await readFile(join(checksDir, f), "utf8");
    const scenarios = parseDvtContent(content);
    dvtScenarios.push(...scenarios);
  }

  // Build lessons
  const lessons = [];
  for (const file of lessonFiles) {
    const markdown = await readFile(join(dirPath, file), "utf8");
    const lessonTitle = extractTitle(markdown) || formatFileName(file);
    const lessonId = basename(file, ".md");

    // Match examples to this lesson by number prefix
    const lessonNum = file.match(/^(\d+)/)?.[1];
    const lessonExamples = lessonNum
      ? examples.filter((e) => e.filename.startsWith(lessonNum + "_"))
      : [];
    const lessonFixtures = lessonNum
      ? fixtures.filter((f) => f.filename.startsWith(lessonNum + "_"))
      : [];

    // Parse exercises from the markdown
    const exercises = extractExercises(markdown, lessonId, lessonExamples);

    // Match dvt scenarios to this lesson by file prefix
    const lessonDvtScenarios = lessonNum
      ? dvtScenarios.filter((s) => {
          const fileBase = basename(s.file, ".dvl");
          return fileBase.startsWith(lessonNum + "_");
        })
      : [];

    lessons.push({
      id: lessonId,
      title: lessonTitle,
      markdown,
      examples: lessonExamples,
      fixtures: lessonFixtures,
      exercises,
      dvtScenarios: lessonDvtScenarios,
    });
  }

  return {
    id: dirName,
    title,
    description,
    lessons,
  };
}

/**
 * Extract exercises from markdown lesson content.
 * Looks for headings containing exercise-related keywords,
 * then extracts the description text and links to starter code.
 */
function extractExercises(markdown, lessonId, examples) {
  const lines = markdown.split("\n");
  const exercises = [];
  let exerciseIdx = 0;

  for (let i = 0; i < lines.length; i++) {
    const headingMatch = lines[i].match(EXERCISE_HEADING_RE);
    if (!headingMatch) continue;

    const heading = headingMatch[1].trim();
    exerciseIdx++;

    // Collect the body text until the next heading or end of file
    const bodyLines = [];
    const numberedItems = [];
    for (let j = i + 1; j < lines.length; j++) {
      if (/^#{1,4}\s/.test(lines[j])) break;
      const trimmed = lines[j].trim();
      if (trimmed) {
        bodyLines.push(trimmed);
        // Collect numbered items as potential exercise steps
        const numMatch = trimmed.match(/^\d+\.\s+(.+)/);
        if (numMatch) {
          numberedItems.push(numMatch[1]);
        }
      }
    }

    const description = bodyLines.join("\n");

    // Try to find a matching .dvl example to use as starter code
    let starterCode = "";
    const expected = { outputs: [], variables: {} };

    if (examples.length > 0) {
      // Use the first example as starter code for this lesson
      const example = examples[Math.min(exerciseIdx - 1, examples.length - 1)];
      if (example) {
        starterCode = example.source;
        // Derive expected outputs from Print statements
        const prints = extractPrintValues(example.source);
        if (prints.length > 0) {
          expected.outputs = prints;
        }
      }
    }

    exercises.push({
      id: `${lessonId}-exercise-${exerciseIdx}`,
      title: heading,
      description,
      starterCode,
      expected,
      hints: numberedItems.length > 0 ? numberedItems : undefined,
    });
  }

  return exercises;
}

/**
 * Extract expected print output values from Devlish source code.
 * Looks for Print statements with literal string/number arguments.
 */
function extractPrintValues(source) {
  const outputs = [];
  const lines = source.split("\n");
  for (const line of lines) {
    const trimmed = line.trim();
    // Match: Print "literal"
    const strMatch = trimmed.match(/^\s*Print\s+"([^"]+)"/);
    if (strMatch) {
      outputs.push(strMatch[1]);
      continue;
    }
    // Match: Print variableName (we cannot know the value without running)
    // Skip these since we cannot predict the output statically
  }
  return outputs;
}

/**
 * Parse .dvt file content into scenario objects.
 */
function parseDvtContent(content) {
  const scenarios = [];
  const lines = content
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l.length > 0);

  let current = null;

  for (const line of lines) {
    const scenarioMatch = line.match(/^Scenario\s+"(.+)"/);
    if (scenarioMatch) {
      if (current) scenarios.push(finalizeDvt(current));
      current = {
        description: scenarioMatch[1],
        file: "",
        expected: { variables: {} },
      };
      continue;
    }

    if (!current) continue;

    const runMatch = line.match(
      /^When I run\s+"([^"]+)"(?:\s+method\s+"([^"]+)")?(?:\s+with\s+\[([^\]]*)\])?/
    );
    if (runMatch) {
      current.file = runMatch[1];
      if (runMatch[2]) current.method = runMatch[2];
      if (runMatch[3]) {
        current.args = parseDvtArgs(runMatch[3]);
      }
      continue;
    }

    if (/^Then run should succeed/.test(line)) continue;

    const returnMatch = line.match(/^Then return value should equal\s+(.+)/);
    if (returnMatch) {
      current.expected.variables["__return__"] = parseDvtValue(
        returnMatch[1]
      );
      continue;
    }

    const routeMatch = line.match(/^Then the route should be\s+"([^"]+)"/);
    if (routeMatch) {
      current.expected.variables["route"] = routeMatch[1];
      continue;
    }

    const varMatch = line.match(/^Then\s+(\S+)\s+should equal\s+(.+)/);
    if (varMatch) {
      current.expected.variables[varMatch[1]] = parseDvtValue(varMatch[2]);
      continue;
    }
  }

  if (current) scenarios.push(finalizeDvt(current));
  return scenarios;
}

function finalizeDvt(partial) {
  return {
    description: partial.description || "Untitled scenario",
    file: partial.file || "",
    method: partial.method || undefined,
    args: partial.args || undefined,
    expected: partial.expected || {},
  };
}

function parseDvtArgs(argsStr) {
  return argsStr
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
    .map(parseDvtValue);
}

function parseDvtValue(raw) {
  const trimmed = raw.trim();
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return trimmed.slice(1, -1);
  }
  if (trimmed === "true") return true;
  if (trimmed === "false") return false;
  const num = Number(trimmed);
  if (!isNaN(num) && trimmed.length > 0) return num;
  return trimmed;
}

function extractTitle(markdown) {
  const match = markdown.match(/^#\s+(.+)/m);
  return match ? match[1].trim() : null;
}

function extractDescription(markdown) {
  const lines = markdown.split("\n");
  const introIdx = lines.findIndex(
    (l) => /^this (unit|folder|section)/i.test(l.trim())
  );
  const startIdx = introIdx !== -1 ? introIdx : -1;
  if (startIdx === -1) {
    const para = lines.find(
      (l) =>
        l.trim() &&
        !l.startsWith("#") &&
        !/^(Last updated|Status:)/i.test(l.trim())
    );
    return para?.trim() || "";
  }
  const desc = [];
  for (let i = startIdx; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line && desc.length > 0) break;
    if (line) desc.push(line);
  }
  return desc.join(" ");
}

function formatDirName(name) {
  return name
    .replace(/^\d+-/, "")
    .replace(/-/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function formatFileName(name) {
  return name
    .replace(/^\d+-/, "")
    .replace(/\.md$/, "")
    .replace(/-/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
