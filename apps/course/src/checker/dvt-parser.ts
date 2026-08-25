/**
 * Parser for .dvt (Devlish Test) files.
 *
 * DVT format example:
 *   Scenario "description"
 *   When I run "../examples/file.dvl"
 *   When I run "../examples/file.dvl" method "name" with [arg1, arg2]
 *   Then run should succeed
 *   Then variable_name should equal value
 *   Then return value should equal value
 */

import type { ExpectedOutput } from "./exercise-checker";

export interface DvtScenario {
  description: string;
  file: string;
  method?: string;
  args?: unknown[];
  expected: ExpectedOutput;
}

/**
 * Parse a .dvt file's text content into an array of scenarios.
 */
export function parseDvtFile(content: string): DvtScenario[] {
  const scenarios: DvtScenario[] = [];
  const lines = content.split("\n").map((l) => l.trim()).filter((l) => l.length > 0);

  let current: Partial<DvtScenario> | null = null;

  for (const line of lines) {
    // Scenario line
    const scenarioMatch = line.match(/^Scenario\s+"(.+)"/);
    if (scenarioMatch) {
      if (current) {
        scenarios.push(finalize(current));
      }
      current = {
        description: scenarioMatch[1],
        expected: { variables: {} },
      };
      continue;
    }

    if (!current) continue;

    // When I run line
    const runMatch = line.match(
      /^When I run\s+"([^"]+)"(?:\s+method\s+"([^"]+)")?(?:\s+with\s+\[([^\]]*)\])?/
    );
    if (runMatch) {
      current.file = runMatch[1];
      if (runMatch[2]) current.method = runMatch[2];
      if (runMatch[3]) {
        current.args = parseArgs(runMatch[3]);
      }
      continue;
    }

    // Then run should succeed (we already require success by default)
    if (/^Then run should succeed/.test(line)) {
      continue;
    }

    // Then return value should equal <value>
    const returnMatch = line.match(/^Then return value should equal\s+(.+)/);
    if (returnMatch) {
      if (!current.expected) current.expected = { variables: {} };
      if (!current.expected.variables) current.expected.variables = {};
      current.expected.variables["__return__"] = parseValue(returnMatch[1]);
      continue;
    }

    // Then <variable> should equal <value>
    const varMatch = line.match(/^Then\s+(\S+)\s+should equal\s+(.+)/);
    if (varMatch) {
      if (!current.expected) current.expected = { variables: {} };
      if (!current.expected.variables) current.expected.variables = {};
      current.expected.variables[varMatch[1]] = parseValue(varMatch[2]);
      continue;
    }

    // Then the route should be "<value>"
    const routeMatch = line.match(/^Then the route should be\s+"([^"]+)"/);
    if (routeMatch) {
      if (!current.expected) current.expected = { variables: {} };
      if (!current.expected.variables) current.expected.variables = {};
      current.expected.variables["route"] = routeMatch[1];
      continue;
    }
  }

  if (current) {
    scenarios.push(finalize(current));
  }

  return scenarios;
}

function finalize(partial: Partial<DvtScenario>): DvtScenario {
  return {
    description: partial.description || "Untitled scenario",
    file: partial.file || "",
    method: partial.method,
    args: partial.args,
    expected: partial.expected || {},
  };
}

function parseArgs(argsStr: string): unknown[] {
  return argsStr
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
    .map(parseValue);
}

function parseValue(raw: string): unknown {
  const trimmed = raw.trim();

  // Quoted string
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return trimmed.slice(1, -1);
  }

  // Boolean
  if (trimmed === "true") return true;
  if (trimmed === "false") return false;

  // Number
  const num = Number(trimmed);
  if (!isNaN(num) && trimmed.length > 0) return num;

  // Fallback: string
  return trimmed;
}
