import { loadDevlishCompiler } from "../js/compiler.mjs";
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const WASM_PATH = resolve(__dirname, "../pkg/devlish_compiler.wasm");

test("compiles valid .dvl source to bytecode JSON", async () => {
  const compiler = await loadDevlishCompiler({ wasmPath: WASM_PATH });
  const source = 'invoice_amount equals 1200\nPrint invoice_amount';
  const result = compiler.compile(source);

  assert.equal(result.success, true, "compilation should succeed");
  assert.ok(result.bytecode, "result should contain bytecode");
  assert.ok(result.bytecode.format, "bytecode should have a format field");
});

test("returns diagnostics for invalid source", async () => {
  const compiler = await loadDevlishCompiler({ wasmPath: WASM_PATH });
  const source = "@@@ not valid devlish at all &&&";
  const result = compiler.compile(source);

  assert.equal(result.success, false, "compilation should fail");
  assert.ok(Array.isArray(result.diagnostics), "result should contain diagnostics array");
  assert.ok(result.diagnostics.length > 0, "diagnostics should not be empty");
});

test("handles empty source", async () => {
  const compiler = await loadDevlishCompiler({ wasmPath: WASM_PATH });
  const result = compiler.compile("");

  // Empty source may compile to an empty program or fail; either way it should not crash
  assert.ok(
    typeof result.success === "boolean",
    "result should have a boolean success field"
  );
});
