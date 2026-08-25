import { test } from "node:test";
import assert from "node:assert/strict";
import { loadTool, runTool } from "../dist/index.js";

// Minimal bytecode: sets register "result" to constant 42, then responds with it.
const simpleBytecode = {
  format: "devlish-bytecode",
  format_version: 0,
  constant_pool: [42],
  instructions: [
    { op: "CONST", dest: "result", const: 0 },
    { op: "RESPOND", value: "result" },
  ],
  source_map: [],
};

test("runTool executes bytecode and returns result", async () => {
  const result = await runTool(simpleBytecode, {});
  assert.equal(result.success, true);
  assert.equal(result.responded, true);
  assert.equal(result.response, 42);
});

test("loadTool returns a reusable DevlishTool", async () => {
  const tool = await loadTool({ bytecode: simpleBytecode, mainThread: true });
  const r1 = await tool.run({});
  assert.equal(r1.success, true);
  assert.equal(r1.response, 42);

  // Can run multiple times
  const r2 = await tool.run({});
  assert.equal(r2.success, true);
  tool.dispose();
});

test("rejects bytecode requiring http permissions", async () => {
  const httpBytecode = {
    format: "devlish-bytecode",
    format_version: 0,
    constant_pool: [],
    instructions: [],
    source_map: [],
    manifest: {
      permissions: [{ kind: "http_request", scope: "https://example.com" }],
    },
  };
  const result = await runTool(httpBytecode, {});
  assert.equal(result.success, false);
  assert.match(result.error, /permissions unavailable in WASM/);
});

test("rejects bytecode requiring filesystem permissions", async () => {
  const fsBytecode = {
    format: "devlish-bytecode",
    format_version: 0,
    constant_pool: [],
    instructions: [],
    source_map: [],
    manifest: {
      permissions: [{ kind: "filesystem" }],
    },
  };
  const result = await runTool(fsBytecode, {});
  assert.equal(result.success, false);
  assert.match(result.error, /permissions unavailable in WASM/);
});

test("instruction limit prevents infinite loops", async () => {
  const loopBytecode = {
    format: "devlish-bytecode",
    format_version: 0,
    constant_pool: [1],
    instructions: [
      { op: "CONST", dest: "x", const: 0 },
      { op: "JUMP", target: 0 },
    ],
    source_map: [],
  };
  const result = await runTool(loopBytecode, {});
  assert.equal(result.success, false);
  assert.match(result.error, /Instruction limit exceeded/);
});

test("instructionLimit option configures the limit", async () => {
  const loopBytecode = {
    format: "devlish-bytecode",
    format_version: 0,
    constant_pool: [1],
    instructions: [
      { op: "CONST", dest: "x", const: 0 },
      { op: "JUMP", target: 0 },
    ],
    source_map: [],
  };
  // With a very low limit, should fail fast
  const tool = await loadTool({
    bytecode: loopBytecode,
    mainThread: true,
    instructionLimit: 50,
  });
  const result = await tool.run({});
  assert.equal(result.success, false);
  assert.match(result.error, /Instruction limit exceeded \(50 instructions\)/);
  tool.dispose();
});

test("rejects bytecode with out-of-range jump target", async () => {
  const evilBytecode = {
    format: "devlish-bytecode",
    format_version: 0,
    constant_pool: [],
    instructions: [{ op: "JUMP", target: 99 }],
    source_map: [],
  };
  const result = await runTool(evilBytecode, {});
  assert.equal(result.success, false);
  assert.match(result.error, /out of range/);
});

test("repeated runs on one tool are stable", async () => {
  const tool = await loadTool({ bytecode: simpleBytecode, mainThread: true });
  for (let i = 0; i < 50; i++) {
    const result = await tool.run({});
    assert.equal(result.success, true);
    assert.equal(result.response, 42);
  }
  tool.dispose();
});

test("a failed run does not break subsequent runs", async () => {
  const failingBytecode = {
    format: "devlish-bytecode",
    format_version: 0,
    constant_pool: [],
    instructions: [{ op: "FAIL", message: "intentional failure" }],
    source_map: [],
  };
  const failing = await loadTool({ bytecode: failingBytecode, mainThread: true });
  const bad = await failing.run({});
  assert.equal(bad.success, false);
  // The same instance stays usable after a failed run.
  const badAgain = await failing.run({});
  assert.equal(badAgain.success, false);
  failing.dispose();

  const good = await loadTool({ bytecode: simpleBytecode, mainThread: true });
  const ok = await good.run({});
  assert.equal(ok.success, true);
  good.dispose();
});

test("loadTool surfaces artifact metadata", async () => {
  const withMeta = {
    ...simpleBytecode,
    compiler_version: "0.1.0",
    source_hash: "abc123",
    source_path: "tool.dvl",
    manifest: { permissions: [{ kind: "llm_checkpoint" }] },
  };
  const tool = await loadTool({ bytecode: withMeta, mainThread: true });
  assert.equal(tool.info.formatVersion, 0);
  assert.equal(tool.info.compilerVersion, "0.1.0");
  assert.equal(tool.info.sourceHash, "abc123");
  assert.equal(tool.info.sourcePath, "tool.dvl");
  assert.deepEqual(tool.info.permissions, ["llm_checkpoint"]);
  tool.dispose();
});

test("loadTool rejects a non-devlish artifact", async () => {
  await assert.rejects(
    loadTool({ bytecode: { format: "something-else", format_version: 0 }, mainThread: true }),
    /Not a Devlish bytecode package/
  );
});

test("loadTool rejects an unsupported format_version", async () => {
  await assert.rejects(
    loadTool({ bytecode: { ...simpleBytecode, format_version: 99 }, mainThread: true }),
    /Unsupported bytecode format_version 99/
  );
});

test("loadTool rejects structurally broken bytecode", async () => {
  const { instructions, ...withoutInstructions } = simpleBytecode;
  await assert.rejects(
    loadTool({ bytecode: withoutInstructions, mainThread: true }),
    /missing its 'instructions' array/
  );
  await assert.rejects(
    loadTool({ bytecode: "not json {", mainThread: true }),
    /not valid JSON/
  );
});

test("loadTool verifies expectedSha256 integrity", async () => {
  const bytecodeJson = JSON.stringify(simpleBytecode);
  const digest = await globalThis.crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(bytecodeJson)
  );
  const goodHash = Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");

  const tool = await loadTool({
    bytecode: bytecodeJson,
    mainThread: true,
    expectedSha256: goodHash.toUpperCase(),
  });
  const result = await tool.run({});
  assert.equal(result.success, true);
  tool.dispose();

  await assert.rejects(
    loadTool({ bytecode: bytecodeJson, mainThread: true, expectedSha256: "deadbeef" }),
    /integrity check failed/
  );
});

test("loadTool rejects non-object bytecode (array, null)", async () => {
  await assert.rejects(
    loadTool({ bytecode: [1, 2, 3], mainThread: true }),
    /must be a JSON object/
  );
  await assert.rejects(
    loadTool({ bytecode: "null", mainThread: true }),
    /must be a JSON object/
  );
});

test("loadTool surfaces empty permissions when no manifest", async () => {
  const tool = await loadTool({ bytecode: simpleBytecode, mainThread: true });
  assert.deepEqual(tool.info.permissions, []);
  assert.equal(tool.info.compilerVersion, null);
  tool.dispose();
});

import { selectVersion, isValidIsoDate } from "../dist/index.js";

function governed(id, version, from, until, extra = {}) {
  const rule = { id, version };
  if (from) rule.effective_from = from;
  if (until) rule.effective_until = until;
  return { ...simpleBytecode, manifest: { permissions: [], rule } };
}

test("tool.info surfaces rule governance metadata", async () => {
  const bc = governed("credit.dti", "2.1.0", "2026-01-01", "2026-12-31", {});
  bc.manifest.rule.author = "Andrew";
  const tool = await loadTool({ bytecode: bc, mainThread: true });
  assert.equal(tool.info.rule.id, "credit.dti");
  assert.equal(tool.info.rule.version, "2.1.0");
  assert.equal(tool.info.rule.effectiveFrom, "2026-01-01");
  assert.equal(tool.info.rule.effectiveUntil, "2026-12-31");
  tool.dispose();
});

test("tool.info.rule is null for an ungoverned artifact", async () => {
  const tool = await loadTool({ bytecode: simpleBytecode, mainThread: true });
  assert.equal(tool.info.rule, null);
  tool.dispose();
});

test("selectVersion picks the version in force on the as-of date", () => {
  const v1 = governed("credit.dti", "1.0.0", "2025-01-01", "2025-12-31");
  const v2 = governed("credit.dti", "2.0.0", "2026-01-01", "2026-12-31");
  const v3 = governed("credit.dti", "3.0.0", "2027-01-01", null); // open-ended
  assert.equal(selectVersion([v1, v2, v3], "2025-06-15").info.rule.version, "1.0.0");
  assert.equal(selectVersion([v1, v2, v3], "2026-03-15").info.rule.version, "2.0.0");
  assert.equal(selectVersion([v1, v2, v3], "2030-01-01").info.rule.version, "3.0.0");
});

test("selectVersion errors when no version is in force", () => {
  const v1 = governed("credit.dti", "1.0.0", "2025-01-01", "2025-12-31");
  assert.throws(() => selectVersion([v1], "2024-01-01"), /no version of credit.dti is in force/);
});

test("selectVersion rejects overlapping windows", () => {
  const a = governed("credit.dti", "1.0.0", "2026-01-01", "2026-12-31");
  const b = governed("credit.dti", "1.1.0", "2026-06-01", "2027-06-01");
  assert.throws(() => selectVersion([a, b], "2026-08-01"), /multiple versions .* in force/);
});

test("selectVersion rejects mixed rule ids and ungoverned artifacts", () => {
  const a = governed("credit.dti", "1.0.0", "2026-01-01", null);
  const b = governed("pricing.tier", "1.0.0", "2026-01-01", null);
  assert.throws(() => selectVersion([a, b], "2026-06-01"), /needs one rule id/);
  assert.throws(() => selectVersion([simpleBytecode], "2026-06-01"), /needs governed artifacts/);
});

test("selectVersion rejects an impossible as-of date", () => {
  const a = governed("credit.dti", "1.0.0", "2026-01-01", null);
  assert.throws(() => selectVersion([a], "2026-02-31"), /must be a real YYYY-MM-DD/);
  assert.equal(isValidIsoDate("2028-02-29"), true);
  assert.equal(isValidIsoDate("2027-02-29"), false);
});

// -- Audit records (DEVL-114) --------------------------------------------

const governedBytecode = {
  format: "devlish-bytecode",
  format_version: 0,
  constant_pool: [42],
  instructions: [
    { op: "CONST", dest: "r0", const: 0 },
    { op: "STORE", symbol: "answer", value: "r0" },
  ],
  source_map: [],
  manifest: {
    rule: { id: "pricing.tier", version: "1.0.0" },
  },
};

test("onAuditRecord fires once per governed run with the full record shape", async () => {
  const records = [];
  const tool = await loadTool({
    bytecode: governedBytecode,
    mainThread: true,
    onAuditRecord: (record) => records.push(record),
  });
  const result = await tool.run({ amount: 100 });
  tool.dispose();

  assert.equal(result.success, true);
  assert.equal(
    result.__devlish_audit__,
    undefined,
    "audit transport is stripped from the result"
  );
  assert.equal(records.length, 1);

  const record = records[0];
  assert.equal(record.rule_id, "pricing.tier");
  assert.equal(record.rule_version, "1.0.0");
  assert.equal(record.success, true);
  assert.equal(record.runtime.kind, "wasm");
  assert.match(record.runtime.version, /^\d+\.\d+\.\d+$/);
  assert.match(record.artifact_sha256, /^[0-9a-f]{64}$/);
  assert.match(record.input_sha256, /^[0-9a-f]{64}$/);
  assert.match(record.output_sha256, /^[0-9a-f]{64}$/);
  assert.equal(typeof record.instruction_count, "number");
  assert.equal(typeof record.timestamp, "number");
  assert.ok(record.timestamp > 1_700_000_000);
});

test("two runs of the same governed tool emit two records", async () => {
  const records = [];
  const tool = await loadTool({
    bytecode: governedBytecode,
    mainThread: true,
    onAuditRecord: (record) => records.push(record),
  });
  await tool.run({});
  await tool.run({});
  tool.dispose();
  assert.equal(records.length, 2);
  assert.equal(records[0].input_sha256, records[1].input_sha256);
});

test("ungoverned runs never invoke onAuditRecord", async () => {
  const records = [];
  const tool = await loadTool({
    bytecode: simpleBytecode,
    mainThread: true,
    onAuditRecord: (record) => records.push(record),
  });
  const result = await tool.run({});
  tool.dispose();
  assert.equal(result.success, true);
  assert.equal(records.length, 0);
});

test("audit transport is stripped even without a callback", async () => {
  const tool = await loadTool({ bytecode: governedBytecode, mainThread: true });
  const result = await tool.run({});
  tool.dispose();
  assert.equal(result.success, true);
  assert.equal(result.__devlish_audit__, undefined);
});
