import type {
  AuditRecord,
  DevlishTool,
  LoadToolOptions,
  RunResult,
  RuleInfo,
  ToolInfo,
} from "./types.js";
import { decodeBase64, instantiateWasm, runBytecode, setInstructionLimit } from "./engine.js";
import type { RuleVersion } from "./artifact.js";
import {
  ArtifactError,
  SUPPORTED_FORMAT_VERSIONS,
  isValidIsoDate,
  selectVersion,
  validateArtifact,
  verifySha256,
} from "./artifact.js";
import { WASM_BASE64 } from "./generated/wasm-base64.js";

export type {
  AuditRecord,
  DevlishTool,
  LoadToolOptions,
  RunResult,
  RuleInfo,
  RuleVersion,
  ToolInfo,
};
export { ArtifactError, SUPPORTED_FORMAT_VERSIONS, isValidIsoDate, selectVersion };

/**
 * Load a compiled Devlish tool. By default runs in a Web Worker
 * to keep the main thread responsive.
 */
export async function loadTool(options: LoadToolOptions): Promise<DevlishTool> {
  const bytecodeJson =
    typeof options.bytecode === "string"
      ? options.bytecode
      : JSON.stringify(options.bytecode);

  const info = validateArtifact(bytecodeJson);
  if (options.expectedSha256 !== undefined) {
    await verifySha256(bytecodeJson, options.expectedSha256);
  }

  const limit = options.instructionLimit;

  if (options.mainThread) {
    return loadMainThread(bytecodeJson, info, limit, options.onAuditRecord);
  }

  if (typeof Worker === "undefined") {
    return loadMainThread(bytecodeJson, info, limit, options.onAuditRecord);
  }

  return loadWorker(bytecodeJson, info, limit, options.onAuditRecord);
}

/**
 * Reserved envelope key the WASM runner uses to carry audit records out of
 * the sandbox (it has no clock or side channel). Namespaced so program-
 * controlled keys cannot collide with the transport.
 */
const AUDIT_TRANSPORT_KEY = "__devlish_audit__";

/**
 * Strip the audit transport off the result -- the VM's output_sha256 covers
 * the result WITHOUT it -- stamp the delivery time, and hand each record to
 * the embedder's callback. The runner guarantees the key only ever holds
 * records it produced, but guard the shape anyway.
 */
function dispatchAudit(
  result: RunResult,
  onAuditRecord?: (record: AuditRecord) => void
): RunResult {
  const envelope = result as RunResult & Record<string, unknown>;
  const records = envelope[AUDIT_TRANSPORT_KEY];
  if (records === undefined) {
    return result;
  }
  delete envelope[AUDIT_TRANSPORT_KEY];
  if (onAuditRecord && Array.isArray(records)) {
    const timestamp = Math.floor(Date.now() / 1000);
    for (const record of records) {
      if (record === null || typeof record !== "object") continue;
      (record as AuditRecord).timestamp = timestamp;
      onAuditRecord(record as AuditRecord);
    }
  }
  return result;
}

/**
 * One-shot convenience: load and immediately run a tool.
 * Disposes the tool after execution. Audit records are discarded on this
 * path -- governed rules whose provenance must be persisted should go
 * through loadTool with onAuditRecord.
 */
export async function runTool(
  bytecode: unknown,
  input?: Record<string, unknown>
): Promise<RunResult> {
  const tool = await loadTool({ bytecode, mainThread: true });
  try {
    return await tool.run(input);
  } finally {
    tool.dispose();
  }
}

async function loadMainThread(
  bytecodeJson: string,
  info: ToolInfo,
  limit?: number,
  onAuditRecord?: (record: AuditRecord) => void
): Promise<DevlishTool> {
  const wasmBytes = decodeBase64(WASM_BASE64);

  async function freshExports() {
    const exports = await instantiateWasm(wasmBytes);
    if (limit !== undefined) {
      setInstructionLimit(exports, limit);
    }
    return exports;
  }

  // Held as a promise so a run that arrives while a trapped instance is
  // being replaced awaits the fresh instance instead of using the dead one.
  let exportsPromise = freshExports();
  await exportsPromise;

  return {
    info,
    async run(input = {}) {
      const exports = await exportsPromise;
      const result = runBytecode(exports, bytecodeJson, JSON.stringify(input));
      if (result.trapped) {
        // The trapped instance is in an unknown state; replace it so the
        // next run starts clean.
        exportsPromise = freshExports();
      }
      return dispatchAudit(result, onAuditRecord);
    },
    dispose() {
      // Nothing to clean up on main thread.
    },
  };
}

async function loadWorker(
  bytecodeJson: string,
  info: ToolInfo,
  limit?: number,
  onAuditRecord?: (record: AuditRecord) => void
): Promise<DevlishTool> {
  let nextId = 0;
  const pending = new Map<
    number,
    { resolve: (v: unknown) => void; reject: (e: Error) => void }
  >();

  // Create worker from the worker entry point.
  // Bundlers (Vite, Webpack, esbuild) handle this import.meta.url pattern.
  const worker = new Worker(new URL("./worker.js", import.meta.url), {
    type: "module",
  });

  worker.onmessage = (event: MessageEvent) => {
    const { id, result, error } = event.data;
    const handler = pending.get(id);
    if (!handler) return;
    pending.delete(id);
    if (error) {
      handler.reject(new Error(error));
    } else {
      handler.resolve(result);
    }
  };

  worker.onerror = (event: ErrorEvent) => {
    const err = new Error(`Worker error: ${event.message}`);
    for (const [, handler] of pending) {
      handler.reject(err);
    }
    pending.clear();
  };

  function send(msg: Record<string, unknown>): Promise<unknown> {
    const id = nextId++;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      worker.postMessage({ id, ...msg });
    });
  }

  // Initialize the worker with WASM + bytecode.
  await send({ type: "init", bytecode: bytecodeJson, instructionLimit: limit });

  return {
    info,
    async run(input = {}) {
      const result = await send({ type: "run", input: JSON.stringify(input) });
      return dispatchAudit(result as RunResult, onAuditRecord);
    },
    dispose() {
      worker.terminate();
      for (const [, handler] of pending) {
        handler.reject(new Error("Worker terminated"));
      }
      pending.clear();
    },
  };
}
