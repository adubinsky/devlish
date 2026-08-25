const encoder = new TextEncoder();
const decoder = new TextDecoder();

export async function loadDevlishWorkflow(options = {}) {
  const host = await buildHost(options.host || {});
  const wasmBytes = await loadBytes(options.wasmBytes || options.wasmPath || options.wasmUrl);
  const bytecodeText = await loadText(options.bytecode || options.bytecodePath || options.bytecodeUrl);
  const events = [];
  let instance;
  let exports;

  const imports = {
    devlish_host: {
      emit_event(ptr, len) {
        const event = JSON.parse(readString(exports.memory, ptr, len));
        events.push(event);
        if (host.emitEvent) host.emitEvent(event);
      },
      write_file(ptr, len) {
        const request = JSON.parse(readString(exports.memory, ptr, len));
        try {
          if (host.writeFile) {
            const result = host.writeFile(request);
            if (result && typeof result.then === "function") {
              throw new Error("host.writeFile must be synchronous in the Devlish WASM runner v0");
            }
            return result === false ? 1 : 0;
          }
          return 0;
        } catch (error) {
          if (host.emitEvent) {
            host.emitEvent({
              type: "host_error",
              kind: "file_write",
              path: request.path,
              error: error.message
            });
          }
          return 1;
        }
      }
    }
  };

  const instantiated = await WebAssembly.instantiate(wasmBytes, imports);
  instance = instantiated.instance || instantiated;
  exports = instance.exports;

  return {
    instance,
    events,
    async run(input = {}) {
      events.length = 0;
      const bytecode = allocateString(exports, bytecodeText);
      const runInput = allocateString(exports, JSON.stringify(input));
      const status = exports.devlish_run(bytecode.ptr, bytecode.len, runInput.ptr, runInput.len);
      exports.devlish_free(bytecode.ptr, bytecode.len);
      exports.devlish_free(runInput.ptr, runInput.len);

      const resultText = readString(exports.memory, exports.devlish_result_ptr(), exports.devlish_result_len());
      const result = JSON.parse(resultText);
      // Governed runs attach audit records to the envelope under a reserved
      // key; strip them off (output_sha256 covers the result without them),
      // stamp the delivery time, and hand them to the host.
      const audit = result.__devlish_audit__;
      if (audit !== undefined) {
        delete result.__devlish_audit__;
        if (host.onAuditRecord && Array.isArray(audit)) {
          const timestamp = Math.floor(Date.now() / 1000);
          for (const record of audit) {
            if (record === null || typeof record !== "object") continue;
            record.timestamp = timestamp;
            host.onAuditRecord(record);
          }
        }
      }
      result.status = status;
      return result;
    }
  };
}

async function buildHost(host) {
  if (host.writeFile) return host;

  if (isNode()) {
    const fs = await import("node:fs");
    const path = await import("node:path");
    return {
      ...host,
      writeFile(request) {
        fs.mkdirSync(path.dirname(request.path), { recursive: true });
        fs.writeFileSync(request.path, request.content);
        return true;
      }
    };
  }

  return host;
}

async function loadBytes(source) {
  if (source instanceof Uint8Array) return source;
  if (source instanceof ArrayBuffer) return new Uint8Array(source);
  if (!source) throw new Error("A wasmPath, wasmUrl, or wasmBytes option is required");

  if (isNode() && !isWebUrl(source)) {
    const fs = await import("node:fs/promises");
    return fs.readFile(source);
  }

  const response = await fetch(source);
  if (!response.ok) throw new Error(`Failed to load ${source}: ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

async function loadText(source) {
  if (typeof source === "object" && !(source instanceof Uint8Array) && !(source instanceof ArrayBuffer)) {
    return JSON.stringify(source);
  }
  if (source instanceof Uint8Array) return decoder.decode(source);
  if (source instanceof ArrayBuffer) return decoder.decode(source);
  if (!source) throw new Error("A bytecodePath, bytecodeUrl, or bytecode option is required");

  if (isNode() && !isWebUrl(source)) {
    const fs = await import("node:fs/promises");
    return fs.readFile(source, "utf8");
  }

  const response = await fetch(source);
  if (!response.ok) throw new Error(`Failed to load ${source}: ${response.status}`);
  return response.text();
}

function allocateString(exports, value) {
  const bytes = encoder.encode(value);
  const ptr = exports.devlish_alloc(bytes.length);
  new Uint8Array(exports.memory.buffer, ptr, bytes.length).set(bytes);
  return { ptr, len: bytes.length };
}

function readString(memory, ptr, len) {
  return decoder.decode(new Uint8Array(memory.buffer, ptr, len));
}

function isNode() {
  return typeof process !== "undefined" && Boolean(process.versions?.node);
}

function isWebUrl(value) {
  return typeof value === "string" && /^(https?:)?\/\//.test(value);
}
