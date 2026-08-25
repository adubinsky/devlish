import type { RunResult } from "./types.js";

const encoder = new TextEncoder();
const decoder = new TextDecoder();

interface WasmExports {
  memory: WebAssembly.Memory;
  devlish_alloc(len: number): number;
  devlish_free(ptr: number, len: number): void;
  devlish_run(
    bytecodePtr: number,
    bytecodeLen: number,
    inputPtr: number,
    inputLen: number
  ): number;
  devlish_result_ptr(): number;
  devlish_result_len(): number;
  devlish_set_instruction_limit(limit: bigint): void;
}

export function decodeBase64(base64: string): Uint8Array {
  if (typeof atob === "function") {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }
  return new Uint8Array(Buffer.from(base64, "base64"));
}

export async function instantiateWasm(
  wasmBytes: Uint8Array
): Promise<WasmExports> {
  const imports = {
    devlish_host: {
      emit_event(_ptr: number, _len: number) {
        // No-op: events are disabled in the WASM runner.
      },
      write_file(_ptr: number, _len: number): number {
        // File writes are not supported in the browser sandbox.
        return 1;
      },
    },
  };

  const { instance } = await WebAssembly.instantiate(
    wasmBytes as BufferSource,
    imports
  );
  return instance.exports as unknown as WasmExports;
}

export function setInstructionLimit(exports: WasmExports, limit: number): void {
  exports.devlish_set_instruction_limit(BigInt(limit));
}

export function runBytecode(
  exports: WasmExports,
  bytecodeJson: string,
  inputJson: string
): RunResult {
  const bytecodeBytes = encoder.encode(bytecodeJson);
  const inputBytes = encoder.encode(inputJson);

  const bytecodePtr = exports.devlish_alloc(bytecodeBytes.length);
  new Uint8Array(exports.memory.buffer, bytecodePtr, bytecodeBytes.length).set(
    bytecodeBytes
  );

  const inputPtr = exports.devlish_alloc(inputBytes.length);
  new Uint8Array(exports.memory.buffer, inputPtr, inputBytes.length).set(
    inputBytes
  );

  let status: number;
  try {
    status = exports.devlish_run(
      bytecodePtr,
      bytecodeBytes.length,
      inputPtr,
      inputBytes.length
    );
  } catch (err) {
    // A Rust panic inside WASM surfaces as a trap (RuntimeError). The
    // instance state is unknown afterward, so skip devlish_free and let
    // the caller replace the instance (see `trapped`).
    const message = err instanceof Error ? err.message : String(err);
    return { success: false, error: `WASM execution trap: ${message}`, trapped: true };
  }

  exports.devlish_free(bytecodePtr, bytecodeBytes.length);
  exports.devlish_free(inputPtr, inputBytes.length);

  const resultPtr = exports.devlish_result_ptr();
  const resultLen = exports.devlish_result_len();
  const resultText = decoder.decode(
    new Uint8Array(exports.memory.buffer, resultPtr, resultLen)
  );

  try {
    const result: RunResult = JSON.parse(resultText);
    if (status === 0 && result.success === undefined) {
      result.success = true;
    }
    return result;
  } catch {
    return { success: false, error: `Invalid VM output: ${resultText.slice(0, 200)}` };
  }
}
