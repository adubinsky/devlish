/**
 * Exercise checker: compares a compileAndRun result against expected output.
 */

export interface ExpectedOutput {
  outputs?: string[];
  variables?: Record<string, unknown>;
}

export interface RunResult {
  success: boolean;
  events?: Array<Record<string, unknown>>;
  diagnostics?: Array<{ message: string }>;
}

export interface CheckResult {
  passed: boolean;
  feedback: string[];
}

/**
 * Check whether a program's run result matches the expected output.
 */
export function checkExercise(
  result: RunResult,
  expected: ExpectedOutput
): CheckResult {
  const feedback: string[] = [];
  let passed = true;

  // The program must succeed
  if (!result.success) {
    passed = false;
    const messages =
      result.diagnostics?.map((d) => d.message).join("; ") || "unknown error";
    feedback.push(`Program failed: ${messages}`);
    return { passed, feedback };
  }

  // Compare printed outputs
  if (expected.outputs && expected.outputs.length > 0) {
    const actualOutputs = extractPrintOutputs(result.events || []);

    for (let i = 0; i < expected.outputs.length; i++) {
      const exp = expected.outputs[i].trim();
      if (i >= actualOutputs.length) {
        passed = false;
        feedback.push(`Expected output '${exp}' but program produced no output at position ${i + 1}`);
      } else {
        const act = actualOutputs[i].trim();
        if (act !== exp) {
          passed = false;
          feedback.push(`Expected output '${exp}' but got '${act}'`);
        }
      }
    }

    if (actualOutputs.length > expected.outputs.length) {
      feedback.push(
        `Program produced ${actualOutputs.length - expected.outputs.length} extra output(s)`
      );
    }
  }

  // Compare variable bindings
  if (expected.variables) {
    const actualVars = extractVariables(result.events || []);
    for (const [name, expectedVal] of Object.entries(expected.variables)) {
      if (!(name in actualVars)) {
        passed = false;
        feedback.push(`Expected variable '${name}' to be set but it was not found`);
      } else {
        const actualVal = actualVars[name];
        if (!valuesEqual(actualVal, expectedVal)) {
          passed = false;
          feedback.push(
            `Expected '${name}' to equal ${JSON.stringify(expectedVal)} but got ${JSON.stringify(actualVal)}`
          );
        }
      }
    }
  }

  if (passed && feedback.length === 0) {
    feedback.push("All checks passed.");
  }

  return { passed, feedback };
}

function extractPrintOutputs(events: Array<Record<string, unknown>>): string[] {
  return events
    .filter((e) => e.type === "print")
    .map((e) => String(e.value ?? ""));
}

function extractVariables(
  events: Array<Record<string, unknown>>
): Record<string, unknown> {
  const vars: Record<string, unknown> = {};
  for (const e of events) {
    if (e.type === "binding" && typeof e.name === "string") {
      vars[e.name] = e.value;
    }
  }
  return vars;
}

function valuesEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  // Loose numeric comparison (e.g. 1000 === 1000.0)
  if (typeof a === "number" && typeof b === "number") return a === b;
  // String comparison, trimmed
  if (typeof a === "string" && typeof b === "string") {
    return a.trim() === b.trim();
  }
  return JSON.stringify(a) === JSON.stringify(b);
}
