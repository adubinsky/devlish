import { decodeBase64, instantiateWasm, runBytecode, setInstructionLimit } from "./engine.js";
import { WASM_BASE64 } from "./generated/wasm-base64.js";

interface WorkerMessage {
  id: number;
  type: "init" | "run";
  bytecode?: string;
  input?: string;
  instructionLimit?: number;
}

interface WorkerResponse {
  id: number;
  result?: unknown;
  error?: string;
}

// Held as a promise so a run that arrives while a trapped instance is
// being replaced awaits the fresh instance instead of using the dead one.
let exportsPromise: Promise<Awaited<ReturnType<typeof instantiateWasm>>> | null =
  null;
let bytecodeJson: string = "";
let instructionLimit: number | undefined;

async function freshExports() {
  const wasmBytes = decodeBase64(WASM_BASE64);
  const fresh = await instantiateWasm(wasmBytes);
  if (instructionLimit !== undefined) {
    setInstructionLimit(fresh, instructionLimit);
  }
  return fresh;
}

self.onmessage = async (event: MessageEvent<WorkerMessage>) => {
  const { id, type } = event.data;
  const respond = (response: Omit<WorkerResponse, "id">) => {
    self.postMessage({ id, ...response } as WorkerResponse);
  };

  try {
    if (type === "init") {
      bytecodeJson = event.data.bytecode!;
      instructionLimit = event.data.instructionLimit;
      exportsPromise = freshExports();
      await exportsPromise;
      respond({ result: { ready: true } });
    } else if (type === "run") {
      if (!exportsPromise) {
        respond({ error: "Worker not initialized" });
        return;
      }
      const exports = await exportsPromise;
      const result = runBytecode(exports, bytecodeJson, event.data.input || "{}");
      if (result.trapped) {
        // The trapped instance is in an unknown state; replace it so the
        // next run starts clean.
        exportsPromise = freshExports();
      }
      respond({ result });
    }
  } catch (err) {
    respond({ error: err instanceof Error ? err.message : String(err) });
  }
};
