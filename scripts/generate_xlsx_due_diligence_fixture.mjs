import path from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const ExcelJS = require("../crates/devlish_wasm_runner/node_modules/exceljs");

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outputPath = path.join(root, "examples/xlsx_due_diligence_packet/fixture.xlsx");

const workbook = new ExcelJS.Workbook();
workbook.creator = "Devlish";
workbook.created = new Date("2026-06-16T21:04:00Z");

const summary = workbook.addWorksheet("Summary");
summary.columns = [
  { header: "Field", key: "field", width: 32 },
  { header: "Value", key: "value", width: 36 }
];
summary.getCell("A1").value = "Tax Credit Transfer Review";
summary.getCell("A2").value = "Site Name";
summary.getCell("B2").value = "Solar Site Alpha";
summary.getCell("A3").value = "Site Address";
summary.getCell("B3").value = "1120 County Road 18, Mesa County, CO";
summary.getCell("A4").value = "Credit Value";
summary.getCell("B4").value = 250000;
summary.getCell("B4").numFmt = "$#,##0";
summary.getCell("A5").value = "Placed In Service Date";
summary.getCell("B5").value = "2025-12-15";

const legal = workbook.addWorksheet("Legal");
legal.columns = summary.columns;
legal.getCell("A1").value = "Legal Diligence Fields";
legal.getCell("A2").value = "Project Name";
legal.getCell("B2").value = "Solar Site Alpha";
legal.getCell("A3").value = "Legal Address";
legal.getCell("B3").value = "1120 County Road 18, Mesa County, CO";

const ppa = workbook.addWorksheet("PPA");
ppa.columns = summary.columns;
ppa.getCell("A1").value = "Power Purchase Agreement";
ppa.getCell("A2").value = "Facility Name";
ppa.getCell("B2").value = "Solar Site Alpha";
ppa.getCell("A3").value = "Facility Address";
ppa.getCell("B3").value = "1120 County Road 18, Mesa County, CO";

const costSeg = workbook.addWorksheet("Cost Seg");
costSeg.columns = summary.columns;
costSeg.getCell("A1").value = "Cost Segregation Summary";
costSeg.getCell("A6").value = "Total Section 48 Property";
costSeg.getCell("B6").value = 337500;
costSeg.getCell("A8").value = "Eligible Basis";
costSeg.getCell("B8").value = 833333.33;
costSeg.getCell("A9").value = "Calculated Credit";
costSeg.getCell("B9").value = { formula: "ROUND(B8*0.3,0)", result: 250000 };

const domestic = workbook.addWorksheet("Domestic Content");
domestic.columns = [
  { header: "Key", key: "key", width: 22 },
  { header: "Description", key: "description", width: 36 },
  { header: "Value", key: "value", width: 42 }
];
domestic.getCell("A5").value = "Affidavit";
domestic.getCell("B5").value = "Domestic content support";
domestic.getCell("C5").value = "Signed domestic content affidavit received";
domestic.getCell("A8").value = "Steel Origin";
domestic.getCell("B8").value = "Iron and steel status";
domestic.getCell("C8").value = "US-sourced steel documented";

const payroll = workbook.addWorksheet("Payroll");
payroll.columns = [
  { header: "Area", key: "area", width: 24 },
  { header: "Period", key: "period", width: 18 },
  { header: "Reviewer", key: "reviewer", width: 24 },
  { header: "Status", key: "status", width: 38 }
];
payroll.getCell("A12").value = "Prevailing Wage";
payroll.getCell("B12").value = "2025";
payroll.getCell("C12").value = "CPA Wage Desk";
payroll.getCell("D12").value = "Certified payroll package complete";

const equipment = workbook.addWorksheet("Equipment");
equipment.columns = [
  { header: "Category", key: "category", width: 28 },
  { header: "Vendor", key: "vendor", width: 24 },
  { header: "Origin", key: "origin", width: 18 },
  { header: "Notes", key: "notes", width: 32 },
  { header: "Count", key: "count", width: 12 }
];
equipment.getCell("A15").value = "US-origin equipment";
equipment.getCell("E15").value = 18;
equipment.getCell("A16").value = "Total equipment";
equipment.getCell("E16").value = 18;

const controls = workbook.addWorksheet("Controls");
controls.columns = summary.columns;
controls.getCell("A2").value = "Formula Health";
controls.getCell("B2").value = { formula: "IF(Summary!B4=250000,\"OK\",\"#VALUE!\")", result: "OK" };

const documents = workbook.addWorksheet("Documents");
documents.columns = [
  { header: "Document", key: "document", width: 36 },
  { header: "Owner", key: "owner", width: 22 },
  { header: "Status", key: "status", width: 18 }
];
documents.getCell("A2").value = "IRS transfer form";
documents.getCell("C2").value = "Present";
documents.getCell("A3").value = "Power purchase agreement";
documents.getCell("C3").value = "Present";
documents.getCell("A4").value = "Land lease";
documents.getCell("C4").value = "Present";
documents.getCell("A5").value = "Cost segregation report";
documents.getCell("C5").value = "Present";
documents.getCell("A6").value = "Domestic content package";
documents.getCell("C6").value = "Present";
documents.getCell("A7").value = "Payroll package";
documents.getCell("C7").value = "Present";

await workbook.xlsx.writeFile(outputPath);
process.stdout.write(`${outputPath}\n`);
