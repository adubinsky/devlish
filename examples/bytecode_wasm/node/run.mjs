import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadDevlishWorkflow } from "../../../crates/devlish_wasm_runner/js/index.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../..");
const wasmPath = path.join(root, "pkg/devlish_wasm_runner/devlish_runner.wasm");
const bytecodePath = path.join(root, "tmp/review_score.dvlc.json");

const workflow = await loadDevlishWorkflow({
  wasmPath,
  bytecodePath,
  host: {
    emitEvent(event) {
      if (event.type === "output_emitted") {
        process.stderr.write(`event:${event.type}\n`);
      }
    }
  }
});

const result = await workflow.run({ customer_tier: "priority" });
process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
