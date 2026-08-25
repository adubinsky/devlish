#!/usr/bin/env node
import fs from "node:fs/promises";
import fsSync from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadDevlishWorkflow } from "../js/index.mjs";
import { preloadDocxTexts, preloadPdfTexts, preloadXlsxCells, runBenchmark } from "../js/benchmark.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(here, "..");
const repoRoot = path.resolve(packageRoot, "../..");

async function main(argv) {
  const [command, ...args] = argv;
  if (command === "run") {
    await runCommand(args);
    return;
  }
  if (command === "benchmark") {
    await benchmarkCommand(args);
    return;
  }
  usage(1);
}

async function runCommand(args) {
  const bytecodePath = args[0];
  if (!bytecodePath) usage(1);
  const inputPath = optionValue(args, "--input");
  const wasmPath = optionValue(args, "--wasm") || defaultWasmPath();
  const input = inputPath ? JSON.parse(await fs.readFile(inputPath, "utf8")) : {};
  const bytecode = JSON.parse(await fs.readFile(bytecodePath, "utf8"));
  const runInput = await enrichInputForEffects(input, inputPath, bytecode);
  const workflow = await loadDevlishWorkflow({ wasmPath, bytecode });
  const result = await workflow.run(runInput);
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  process.exitCode = result.success ? 0 : 1;
}

async function enrichInputForEffects(input, inputPath, bytecode) {
  const effects = Array.isArray(bytecode.effect_table) ? bytecode.effect_table : [];
  const needsXlsx = effects.some((effect) => effect.kind === "xlsx_read_cell");
  const needsPdf = effects.some((effect) => effect.kind === "pdf_read_text");
  const needsDocx = effects.some((effect) => effect.kind === "docx_read_text");
  if (!needsXlsx && !needsPdf && !needsDocx) return input;

  const inputDir = inputPath ? path.dirname(path.resolve(inputPath)) : process.cwd();
  const enriched = { ...input };
  if (needsXlsx) {
    if (!input.workbook) throw new Error("Input JSON must include workbook for XLSX workflows");
    const workbookPath = path.resolve(inputDir, input.workbook);
    enriched.__xlsx_cells__ = await preloadXlsxCells(workbookPath, bytecode);
  }
  if (needsPdf) {
    enriched.__pdf_texts__ = await preloadPdfTexts(inputDir, input, bytecode);
  }
  if (needsDocx) {
    enriched.__docx_texts__ = await preloadDocxTexts(inputDir, input, bytecode);
  }
  if (enriched.assertion_report_path) {
    enriched.assertion_report_path = path.resolve(inputDir, enriched.assertion_report_path);
  }
  if (enriched.benchmark_report_path) {
    enriched.benchmark_report_path = path.resolve(inputDir, enriched.benchmark_report_path);
  }
  return enriched;
}

async function benchmarkCommand(args) {
  const manifestPath = args[0];
  if (!manifestPath) usage(1);
  const result = await runBenchmark(manifestPath, {
    wasmPath: optionValue(args, "--wasm") || defaultWasmPath()
  });
  process.stdout.write(`${JSON.stringify(result.summary, null, 2)}\n`);
  process.exitCode = result.summary.success ? 0 : 1;
}

function defaultWasmPath() {
  const candidates = [
    path.join(packageRoot, "devlish_runner.wasm"),
    path.join(repoRoot, "pkg/devlish_wasm_runner/devlish_runner.wasm")
  ];
  return candidates.find((candidate) => fsSync.existsSync(candidate)) || candidates[0];
}

function optionValue(args, name) {
  const index = args.indexOf(name);
  if (index === -1) return null;
  return args[index + 1] || null;
}

function usage(code) {
  process.stderr.write(`Usage:
  devlish-runner run <workflow.dvlc.json> --input <input.json> [--wasm <path>]
  devlish-runner benchmark <benchmark.json> [--wasm <path>]
`);
  process.exit(code);
}

main(process.argv.slice(2)).catch((error) => {
  process.stderr.write(`${error.stack || error.message}\n`);
  process.exit(1);
});
