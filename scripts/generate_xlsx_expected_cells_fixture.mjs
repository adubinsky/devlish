import path from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const ExcelJS = require("../crates/devlish_wasm_runner/node_modules/exceljs");

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outputPath = path.join(root, "examples/xlsx_expected_cells/fixture.xlsx");

const workbook = new ExcelJS.Workbook();
workbook.creator = "Devlish";
workbook.created = new Date("2026-06-16T18:31:28Z");

const summary = workbook.addWorksheet("Summary");
summary.columns = [
  { header: "Field", key: "field", width: 28 },
  { header: "Value", key: "value", width: 24 }
];
summary.getCell("A1").value = "Tax Credit Review Fixture";
summary.getCell("A2").value = "Site Name";
summary.getCell("B2").value = "Solar Site Alpha";
summary.getCell("A4").value = "Credit Value";
summary.getCell("B4").value = 250000;
summary.getCell("B4").numFmt = "$#,##0";

const inputs = workbook.addWorksheet("Inputs");
inputs.columns = [
  { header: "Key", key: "key", width: 18 },
  { header: "Description", key: "description", width: 32 },
  { header: "Value", key: "value", width: 40 }
];
inputs.getCell("A8").value = "Domestic Content";
inputs.getCell("B8").value = "Supporting status";
inputs.getCell("C8").value = "Domestic content documentation received";
inputs.getCell("A9").value = "Error Check";
inputs.getCell("B9").value = "Formula health";
inputs.getCell("C9").value = "OK";

await workbook.xlsx.writeFile(outputPath);
process.stdout.write(`${outputPath}\n`);
