import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import ExcelJS from "exceljs";
import mammoth from "mammoth";
import { PDFParse } from "pdf-parse";
import { loadDevlishWorkflow } from "./index.mjs";

export async function runBenchmark(manifestPath, options = {}) {
  const manifestAbsolute = path.resolve(manifestPath);
  const manifestDir = path.dirname(manifestAbsolute);
  const manifest = await readJson(manifestAbsolute);
  assertSchema(manifest, "devlish-benchmark-v0", manifestPath);

  const baselinePath = resolveManifestPath(manifestDir, manifest.baseline.path);
  const inputPath = resolveManifestPath(manifestDir, manifest.input.path);
  const bytecodePath = resolveManifestPath(manifestDir, manifest.workflow.bytecode_path);
  const assertionReportPath = resolveManifestPath(manifestDir, manifest.outputs.assertion_report_path);
  const benchmarkReportPath = resolveManifestPath(manifestDir, manifest.outputs.benchmark_report_path);

  await verifySha256(baselinePath, manifest.baseline.sha256);
  await verifySha256(inputPath, manifest.input.sha256);
  await verifySha256(bytecodePath, manifest.workflow.bytecode_sha256);

  const baseline = await readJson(baselinePath);
  validateBaselineTranscript(baseline);
  const input = await readJson(inputPath);
  assertSchema(input, "devlish-benchmark-input-v0", inputPath);
  const bytecode = await readJson(bytecodePath);
  const inputDir = path.dirname(inputPath);
  const xlsxCells = input.workbook ? await preloadXlsxCells(path.resolve(inputDir, input.workbook), bytecode) : {};
  const pdfTexts = await preloadPdfTexts(inputDir, input, bytecode);
  const docxTexts = await preloadDocxTexts(inputDir, input, bytecode);

  const runInput = {
    ...input,
    benchmark: manifest.benchmark,
    assertion_report_path: assertionReportPath,
    benchmark_report_path: benchmarkReportPath,
    __xlsx_cells__: xlsxCells,
    __pdf_texts__: pdfTexts,
    __docx_texts__: docxTexts
  };

  const workflow = await loadDevlishWorkflow({
    wasmPath: options.wasmPath || manifest.wasm_path,
    bytecode
  });
  const startedAt = new Date();
  const started = process.hrtime.bigint();
  const runResult = await workflow.run(runInput);
  const durationMs = Number(process.hrtime.bigint() - started) / 1_000_000;
  if (!runResult.success) {
    throw new Error(runResult.error || "Devlish workflow failed");
  }

  const assertionReport = runResult.results.assertion_report;
  if (!assertionReport || assertionReport.success !== true) {
    throw new Error("Assertion report failed");
  }
  if (manifest.expected_assertion_report?.path) {
    const expected = await readJson(resolveManifestPath(manifestDir, manifest.expected_assertion_report.path));
    if (stableStringify(assertionReport) !== stableStringify(expected)) {
      throw new Error("Assertion report did not match expected fixture");
    }
  }

  await fs.mkdir(path.dirname(assertionReportPath), { recursive: true });
  await fs.writeFile(assertionReportPath, `${JSON.stringify(assertionReport, null, 2)}\n`);

  const benchmarkReport = buildBenchmarkReport({
    manifest,
    baseline,
    bytecode,
    bytecodePath,
    startedAt,
    durationMs,
    runResult
  });
  await fs.mkdir(path.dirname(benchmarkReportPath), { recursive: true });
  await fs.writeFile(benchmarkReportPath, `${JSON.stringify(benchmarkReport, null, 2)}\n`);

  validateSavings(benchmarkReport, manifest.thresholds || {});
  return {
    assertionReport,
    benchmarkReport,
    summary: {
      success: true,
      assertion_report_path: assertionReportPath,
      benchmark_report_path: benchmarkReportPath,
      estimated_avoided_tokens: benchmarkReport.savings.estimated_tokens.value,
      model_visible_byte_reduction_ratio: benchmarkReport.savings.model_visible_bytes.reduction_ratio
    }
  };
}

export async function preloadXlsxCells(workbookPath, bytecodePackage) {
  const effects = Array.isArray(bytecodePackage.effect_table) ? bytecodePackage.effect_table : [];
  const reads = effects.filter((effect) => effect.kind === "xlsx_read_cell");
  const workbook = new ExcelJS.Workbook();
  await workbook.xlsx.readFile(workbookPath);
  const cells = {};
  for (const read of reads) {
    cells[`${read.sheet}!${read.cell}`] = readCell(workbook, read.sheet, read.cell);
  }
  return cells;
}

export async function preloadPdfTexts(inputDir, input, bytecodePackage) {
  const effects = Array.isArray(bytecodePackage.effect_table) ? bytecodePackage.effect_table : [];
  const reads = effects.filter((effect) => effect.kind === "pdf_read_text");
  const texts = {};
  for (const read of reads) {
    const pdfPath = resolvePdfPath(inputDir, input, read.path);
    const buffer = await fs.readFile(pdfPath);
    const parser = new PDFParse({ data: buffer });
    let parsed;
    try {
      parsed = await parser.getText();
    } finally {
      await parser.destroy();
    }
    texts[read.path] = {
      kind: "text",
      value: parsed.text || "",
      pages: parsed.total || null,
      source: read.path
    };
  }
  return texts;
}

export async function preloadDocxTexts(inputDir, input, bytecodePackage) {
  const effects = Array.isArray(bytecodePackage.effect_table) ? bytecodePackage.effect_table : [];
  const reads = effects.filter((effect) => effect.kind === "docx_read_text");
  const texts = {};
  for (const read of reads) {
    const docxPath = resolveDocxPath(inputDir, input, read.path);
    const extracted = await mammoth.extractRawText({ path: docxPath });
    texts[read.path] = {
      kind: "text",
      value: extracted.value || "",
      messages: extracted.messages || [],
      source: read.path
    };
  }
  return texts;
}

export function validateBaselineTranscript(baseline) {
  assertSchema(baseline, "devlish-baseline-transcript-v0", "baseline transcript");
  if (baseline.kind !== "captured_transcript") {
    throw new Error("Only captured_transcript baselines can support public savings claims");
  }
  const promptBytes = sum(Array.isArray(baseline.prompts) ? baseline.prompts.map((item) => item.utf8_bytes || 0) : []);
  const requestBytes = sum(Array.isArray(baseline.tool_calls) ? baseline.tool_calls.map((item) => item.request_utf8_bytes || 0) : []);
  const toolOutputBytes = sum(
    Array.isArray(baseline.tool_calls)
      ? baseline.tool_calls
          .filter((item) => item.model_visible_response !== false)
          .map((item) => item.response_utf8_bytes || 0)
      : []
  );
  if (baseline.prompt_bytes !== promptBytes + requestBytes) {
    throw new Error(`Baseline prompt_bytes mismatch: expected ${promptBytes + requestBytes}, got ${baseline.prompt_bytes}`);
  }
  if (baseline.tool_output_bytes !== toolOutputBytes) {
    throw new Error(`Baseline tool_output_bytes mismatch: expected ${toolOutputBytes}, got ${baseline.tool_output_bytes}`);
  }
  const toolCalls = Array.isArray(baseline.tool_calls) ? baseline.tool_calls : [];
  if (baseline.agent_observed_tool_calls !== toolCalls.length) {
    throw new Error("Baseline agent_observed_tool_calls mismatch");
  }
  const modelCheckpoints = Array.isArray(baseline.model_checkpoints) ? baseline.model_checkpoints : [];
  if (baseline.model_checkpoint_count !== modelCheckpoints.length) {
    throw new Error("Baseline model_checkpoint_count mismatch");
  }
}

function buildBenchmarkReport({ manifest, baseline, bytecode, bytecodePath, startedAt, durationMs, runResult }) {
  const baselineVisibleBytes = baseline.prompt_bytes + baseline.tool_output_bytes;
  const compiledRunnerOutputBytes = utf8Bytes(JSON.stringify(runResult));
  const compiledVisibleBytes = 0;
  const avoidedBytes = baselineVisibleBytes - compiledVisibleBytes;
  const baselineToolCalls = Array.isArray(baseline.tool_calls) ? baseline.tool_calls : [];
  const baselineCheckpoints = Array.isArray(baseline.model_checkpoints) ? baseline.model_checkpoints : [];
  const effectTable = Array.isArray(bytecode.effect_table) ? bytecode.effect_table : [];
  const assertionReport = runResult.results?.assertion_report;
  const assertions = Array.isArray(assertionReport?.assertions) ? assertionReport.assertions : [];
  const xlsxReadCount = effectTable.filter((effect) => effect.kind === "xlsx_read_cell").length;
  const pdfReadCount = effectTable.filter((effect) => effect.kind === "pdf_read_text").length;
  const docxReadCount = effectTable.filter((effect) => effect.kind === "docx_read_text").length;
  return {
    schema_version: "devlish-benchmark-report-v0",
    run_id: `${startedAt.toISOString().replace(/[-:.]/g, "").slice(0, 15)}-${manifest.benchmark}`,
    benchmark: manifest.benchmark,
    timing: {
      source: "process.hrtime.bigint",
      started_at: startedAt.toISOString(),
      duration_ms: Number(durationMs.toFixed(3))
    },
    baseline: {
      kind: baseline.kind,
      source: manifest.baseline.path,
      prompt_bytes: baseline.prompt_bytes,
      tool_output_bytes: baseline.tool_output_bytes,
      model_visible_bytes: baselineVisibleBytes,
      agent_observed_tool_calls: baseline.agent_observed_tool_calls,
      model_checkpoint_count: baseline.model_checkpoint_count,
      provider_usage: baseline.provider_usage || null,
      workflow_shape: {
        phases: unique(baselineToolCalls.map((toolCall) => toolCall.phase).filter(Boolean)),
        tool_call_sequence: baselineToolCalls.map((toolCall) => ({
          id: toolCall.id,
          phase: toolCall.phase || null,
          name: toolCall.name,
          reason: toolCall.reason || null
        })),
        checkpoint_sequence: baselineCheckpoints.map((checkpoint) => ({
          id: checkpoint.id,
          reason: checkpoint.reason || null
        }))
      }
    },
    compiled: {
      kind: "devlish_bytecode_wasm",
      execution_mode: "direct_cli",
      source: path.basename(bytecodePath),
      prompt_bytes: 0,
      tool_output_bytes: 0,
      model_visible_bytes: compiledVisibleBytes,
      runner_output_bytes: compiledRunnerOutputBytes,
      agent_observed_tool_calls: 0,
      runner_events: Array.isArray(runResult.results?.events) ? runResult.results.events.length : 0,
      host_callbacks:
        (Array.isArray(runResult.results?.xlsx_cells) ? runResult.results.xlsx_cells.length : 0) +
        (Array.isArray(runResult.results?.pdf_texts) ? runResult.results.pdf_texts.length : 0) +
        (Array.isArray(runResult.results?.docx_texts) ? runResult.results.docx_texts.length : 0),
      model_checkpoint_count: 0,
      workflow_shape: {
        effect_table_entries: effectTable.length,
        xlsx_cell_reads: xlsxReadCount,
        pdf_text_reads: pdfReadCount,
        docx_text_reads: docxReadCount,
        assertions: assertions.length,
        assertion_ids: assertions.map((assertion) => assertion.id),
        file_writes: effectTable.filter((effect) => effect.kind === "file_write").length
      }
    },
    savings: {
      model_visible_bytes: {
        value: avoidedBytes,
        reduction_ratio: baselineVisibleBytes === 0 ? 0 : avoidedBytes / baselineVisibleBytes,
        measurement: "estimated"
      },
      estimated_tokens: {
        value: Math.ceil(avoidedBytes / 4),
        measurement: "estimated"
      },
      prompt_bytes: {
        value: baseline.prompt_bytes,
        measurement: "estimated"
      },
      tool_output_bytes: {
        value: baseline.tool_output_bytes,
        measurement: "estimated"
      },
      agent_observed_tool_calls: {
        value: baseline.agent_observed_tool_calls,
        measurement: "estimated"
      },
      model_checkpoint_count: {
        value: baseline.model_checkpoint_count,
        measurement: "estimated"
      },
      provider_tokens: {
        value: null,
        measurement: "unavailable"
      }
    }
  };
}

function validateSavings(report, thresholds = {}) {
  const minimumReductionRatio = thresholds.minimum_model_visible_byte_reduction_ratio ?? 0.8;
  const minimumAvoidedTokens = thresholds.minimum_estimated_avoided_tokens ?? 1000;
  if (report.savings.model_visible_bytes.reduction_ratio < minimumReductionRatio) {
    throw new Error(`Model-visible byte reduction is below the ${minimumReductionRatio} benchmark threshold`);
  }
  if (report.savings.estimated_tokens.value <= minimumAvoidedTokens) {
    throw new Error(`Estimated avoided tokens did not exceed the ${minimumAvoidedTokens}-token benchmark threshold`);
  }
}

function unique(values) {
  return [...new Set(values)];
}

function readCell(workbook, sheetName, address) {
  const sheet = workbook.getWorksheet(sheetName);
  if (!sheet) throw new Error(`Missing workbook sheet: ${sheetName}`);
  const cell = sheet.getCell(address);
  const rowNumber = Number(address.match(/\d+$/)?.[0] || 0);
  const columnLetters = address.match(/^[A-Z]+/i)?.[0] || "";
  const columnNumber = columnNumberFromLetters(columnLetters);
  if (rowNumber > sheet.rowCount || columnNumber > sheet.columnCount) {
    return { kind: "missing", value: null, formatted: "" };
  }
  return normalizeCellValue(cell.value, cell.text);
}

function normalizeCellValue(value, formatted) {
  if (value == null) return { kind: "blank", value: null, formatted: formatted || "" };
  if (value instanceof Date) return { kind: "date", value: value.toISOString().slice(0, 10), formatted };
  if (typeof value === "string") return { kind: "string", value, formatted };
  if (typeof value === "number") return { kind: "number", value, formatted };
  if (typeof value === "boolean") return { kind: "boolean", value, formatted };
  if (typeof value === "object" && value.error) return { kind: "error", value: null, error: value.error, formatted };
  if (typeof value === "object" && value.formula) {
    const normalized = normalizeCellValue(value.result, formatted);
    return { ...normalized, formula: value.formula };
  }
  if (typeof value === "object" && value.text) return { kind: "string", value: value.text, formatted };
  return { kind: "string", value: String(value), formatted };
}

async function readJson(filePath) {
  return JSON.parse(await fs.readFile(filePath, "utf8"));
}

function assertSchema(value, schemaVersion, label) {
  if (!value || value.schema_version !== schemaVersion) {
    throw new Error(`Invalid ${label}: expected schema_version ${schemaVersion}`);
  }
}

function resolveManifestPath(manifestDir, relativePath) {
  return path.resolve(manifestDir, relativePath);
}

function resolvePdfPath(inputDir, input, sourcePath) {
  const configuredPath = input.pdfs && typeof input.pdfs === "object" ? input.pdfs[sourcePath] : null;
  return path.resolve(inputDir, configuredPath || sourcePath);
}

function resolveDocxPath(inputDir, input, sourcePath) {
  const configuredPath = input.docx && typeof input.docx === "object" ? input.docx[sourcePath] : null;
  return path.resolve(inputDir, configuredPath || sourcePath);
}

async function verifySha256(filePath, expected) {
  if (!expected || expected === "...") return;
  const digest = crypto.createHash("sha256").update(await fs.readFile(filePath)).digest("hex");
  if (digest !== expected) {
    throw new Error(`SHA256 mismatch for ${filePath}`);
  }
}

function utf8Bytes(value) {
  return Buffer.byteLength(value, "utf8");
}

function sum(values) {
  return values.reduce((total, value) => total + Number(value || 0), 0);
}

function columnNumberFromLetters(letters) {
  return letters.toUpperCase().split("").reduce((total, letter) => total * 26 + letter.charCodeAt(0) - 64, 0);
}

function stableStringify(value) {
  if (Array.isArray(value)) {
    return `[${value.map((item) => stableStringify(item)).join(",")}]`;
  }
  if (value && typeof value === "object") {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${stableStringify(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}
