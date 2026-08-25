import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { promisify } from "node:util";
import { preloadDocxTexts, preloadPdfTexts, preloadXlsxCells, runBenchmark, validateBaselineTranscript } from "../js/benchmark.mjs";
import { loadDevlishWorkflow } from "../js/index.mjs";

const execFileAsync = promisify(execFile);
const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(packageRoot, "../..");
const exampleRoot = path.join(repoRoot, "examples/xlsx_expected_cells");
const dueDiligenceRoot = path.join(repoRoot, "examples/xlsx_due_diligence_packet");
const pdfRoot = path.join(repoRoot, "examples/pdf_transfer_packet");
const docxRoot = path.join(repoRoot, "examples/docx_contract_review");
const wasmPath = path.join(repoRoot, "pkg/devlish_wasm_runner/devlish_runner.wasm");

test("runBenchmark executes the XLSX fixture and reports deterministic savings", async () => {
  const result = await runBenchmark(path.join(exampleRoot, "benchmark.json"), { wasmPath });

  assert.equal(result.assertionReport.success, true);
  assert.equal(result.benchmarkReport.savings.estimated_tokens.value, 16500);
  assert.equal(result.benchmarkReport.savings.provider_tokens.measurement, "unavailable");
  assert.equal(result.summary.model_visible_byte_reduction_ratio, 1);
});

test("runBenchmark executes the due-diligence XLSX fixture with multi-tool-loop savings", async () => {
  const result = await runBenchmark(path.join(dueDiligenceRoot, "benchmark.json"), { wasmPath });

  assert.equal(result.assertionReport.success, true);
  assert.equal(result.assertionReport.assertions.length, 22);
  assert.equal(result.benchmarkReport.baseline.agent_observed_tool_calls, 16);
  assert.equal(result.benchmarkReport.baseline.model_checkpoint_count, 9);
  assert.equal(result.benchmarkReport.savings.estimated_tokens.value, 40505);
  assert.equal(result.summary.model_visible_byte_reduction_ratio, 1);
});

test("runBenchmark executes the PDF transfer packet fixture with skill-loop savings", async () => {
  const result = await runBenchmark(path.join(pdfRoot, "benchmark.json"), { wasmPath });

  assert.equal(result.assertionReport.success, true);
  assert.equal(result.assertionReport.assertions.length, 10);
  assert.equal(result.benchmarkReport.baseline.agent_observed_tool_calls, 10);
  assert.equal(result.benchmarkReport.baseline.model_checkpoint_count, 6);
  assert.equal(result.benchmarkReport.compiled.workflow_shape.pdf_text_reads, 1);
  assert.equal(result.benchmarkReport.savings.estimated_tokens.value, 35105);
  assert.equal(result.summary.model_visible_byte_reduction_ratio, 1);
});

test("runBenchmark executes the DOCX contract review fixture with skill-loop savings", async () => {
  const result = await runBenchmark(path.join(docxRoot, "benchmark.json"), { wasmPath });

  assert.equal(result.assertionReport.success, true);
  assert.equal(result.assertionReport.assertions.length, 10);
  assert.equal(result.benchmarkReport.baseline.agent_observed_tool_calls, 8);
  assert.equal(result.benchmarkReport.baseline.model_checkpoint_count, 5);
  assert.equal(result.benchmarkReport.compiled.workflow_shape.docx_text_reads, 1);
  assert.equal(result.benchmarkReport.savings.estimated_tokens.value, 30938);
  assert.equal(result.summary.model_visible_byte_reduction_ratio, 1);
});

test("CLI run preloads PDF text effects", async () => {
  const { stdout } = await execFileAsync(process.execPath, [
    path.join(packageRoot, "bin/devlish-runner.mjs"),
    "run",
    path.join(pdfRoot, "workflow.dvlc.json"),
    "--input",
    path.join(pdfRoot, "input.json"),
    "--wasm",
    wasmPath
  ]);
  const result = JSON.parse(stdout);

  assert.equal(result.success, true);
  assert.equal(result.results.assertion_report.success, true);
  assert.equal(result.results.pdf_texts.length, 1);
});

test("CLI run preloads DOCX text effects", async () => {
  const { stdout } = await execFileAsync(process.execPath, [
    path.join(packageRoot, "bin/devlish-runner.mjs"),
    "run",
    path.join(docxRoot, "workflow.dvlc.json"),
    "--input",
    path.join(docxRoot, "input.json"),
    "--wasm",
    wasmPath
  ]);
  const result = JSON.parse(stdout);

  assert.equal(result.success, true);
  assert.equal(result.results.assertion_report.success, true);
  assert.equal(result.results.docx_texts.length, 1);
});

test("runBenchmark resolves workbook paths relative to the input JSON", async () => {
  const tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), "devlish-benchmark-"));
  const inputDir = path.join(tempRoot, "inputs");
  await fs.mkdir(inputDir, { recursive: true });
  await fs.copyFile(path.join(exampleRoot, "fixture.xlsx"), path.join(inputDir, "fixture.xlsx"));
  await fs.writeFile(
    path.join(inputDir, "input.json"),
    JSON.stringify(
      {
        schema_version: "devlish-benchmark-input-v0",
        workbook: "fixture.xlsx"
      },
      null,
      2
    )
  );
  await fs.writeFile(
    path.join(tempRoot, "benchmark.json"),
    JSON.stringify(
      {
        schema_version: "devlish-benchmark-v0",
        benchmark: "xlsx_input_relative",
        baseline: { path: path.join(exampleRoot, "baseline.skill-transcript.json") },
        workflow: { bytecode_path: path.join(exampleRoot, "workflow.dvlc.json") },
        input: { path: "inputs/input.json" },
        outputs: {
          assertion_report_path: "out/report.json",
          benchmark_report_path: "out/benchmark-report.json"
        }
      },
      null,
      2
    )
  );

  const result = await runBenchmark(path.join(tempRoot, "benchmark.json"), { wasmPath });

  assert.equal(result.assertionReport.success, true);
});

test("runBenchmark honors benchmark-specific savings thresholds", async () => {
  const tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), "devlish-threshold-"));
  await fs.writeFile(
    path.join(tempRoot, "benchmark.json"),
    JSON.stringify(
      {
        schema_version: "devlish-benchmark-v0",
        benchmark: "xlsx_threshold",
        baseline: { path: path.join(exampleRoot, "baseline.skill-transcript.json") },
        workflow: { bytecode_path: path.join(exampleRoot, "workflow.dvlc.json") },
        input: { path: path.join(exampleRoot, "input.json") },
        outputs: {
          assertion_report_path: "out/report.json",
          benchmark_report_path: "out/benchmark-report.json"
        },
        thresholds: {
          minimum_model_visible_byte_reduction_ratio: 0.8,
          minimum_estimated_avoided_tokens: 999999
        }
      },
      null,
      2
    )
  );

  await assert.rejects(
    () => runBenchmark(path.join(tempRoot, "benchmark.json"), { wasmPath }),
    /999999-token benchmark threshold/
  );
});

test("validateBaselineTranscript rejects malformed aggregate counts", () => {
  assert.throws(
    () =>
      validateBaselineTranscript({
        schema_version: "devlish-baseline-transcript-v0",
        kind: "captured_transcript",
        prompts: [{ utf8_bytes: 10 }],
        tool_calls: [{ request_utf8_bytes: 2, response_utf8_bytes: 3 }],
        model_checkpoints: [],
        prompt_bytes: 999,
        tool_output_bytes: 3,
        agent_observed_tool_calls: 1,
        model_checkpoint_count: 0
      }),
    /prompt_bytes mismatch/
  );
});

test("WASM runner rejects unsupported bytecode format versions", async () => {
  const bytecode = JSON.parse(await fs.readFile(path.join(exampleRoot, "workflow.dvlc.json"), "utf8"));
  bytecode.format_version = 99;
  const workflow = await loadDevlishWorkflow({ wasmPath, bytecode });

  const result = await workflow.run({});

  assert.equal(result.success, false);
  assert.match(result.error, /Unsupported bytecode format_version/);
});

test("WASM runner fails clearly for async host write callbacks", async () => {
  const hostEvents = [];
  const bytecode = {
    format: "devlish-bytecode",
    format_version: 0,
    constant_pool: ["value", "tmp/out.txt"],
    instructions: [
      { op: "CONST", dest: "r0", const: 0 },
      { op: "CONST", dest: "r1", const: 1 },
      { op: "EXPORT", value: "r0", path: "r1", mode: "export" },
      { op: "RETURN" }
    ],
    effect_table: [{ kind: "file_write" }]
  };
  const workflow = await loadDevlishWorkflow({
    wasmPath,
    bytecode,
    host: {
      writeFile: async () => true,
      emitEvent: (event) => hostEvents.push(event)
    }
  });

  const result = await workflow.run({});

  assert.equal(result.success, false);
  assert.match(result.error, /Host write_file failed/);
  assert.match(hostEvents.find((event) => event.type === "host_error").error, /must be synchronous/);
});


test("preloadXlsxCells fails clearly for missing workbook files", async () => {
  await assert.rejects(
    () =>
      preloadXlsxCells(path.join(exampleRoot, "missing.xlsx"), {
        effect_table: [{ kind: "xlsx_read_cell", sheet: "Summary", cell: "B2" }]
      }),
    /file not found|no such file|ENOENT/i
  );
});

test("preloadPdfTexts fails clearly for missing PDF files", async () => {
  await assert.rejects(
    () =>
      preloadPdfTexts(
        pdfRoot,
        { pdfs: { "missing.pdf": "missing.pdf" } },
        { effect_table: [{ kind: "pdf_read_text", path: "missing.pdf" }] }
      ),
    /file not found|no such file|ENOENT/i
  );
});

test("preloadDocxTexts fails clearly for missing DOCX files", async () => {
  await assert.rejects(
    () =>
      preloadDocxTexts(
        docxRoot,
        { docx: { "missing.docx": "missing.docx" } },
        { effect_table: [{ kind: "docx_read_text", path: "missing.docx" }] }
      ),
    /file not found|no such file|ENOENT/i
  );
});
