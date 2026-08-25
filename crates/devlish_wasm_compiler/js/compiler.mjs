const encoder = new TextEncoder();
const decoder = new TextDecoder();

export async function loadDevlishCompiler(options = {}) {
  const wasmBytes = await loadBytes(options.wasmBytes || options.wasmPath || options.wasmUrl);
  const instantiated = await WebAssembly.instantiate(wasmBytes, {});
  const instance = instantiated.instance || instantiated;
  const exports = instance.exports;

  return {
    instance,
    compile(source) {
      const allocated = allocateString(exports, source);
      const status = exports.devlish_compile(allocated.ptr, allocated.len);
      exports.devlish_free(allocated.ptr, allocated.len);

      const resultText = readString(
        exports.memory,
        exports.devlish_compile_result_ptr(),
        exports.devlish_compile_result_len()
      );

      if (status === 0) {
        return { success: true, bytecode: JSON.parse(resultText) };
      }
      const error = JSON.parse(resultText);
      return { success: false, diagnostics: error.diagnostics || [] };
    }
  };
}

export async function loadDevlishFull(options = {}) {
  const compilerWasm = await loadBytes(
    options.compilerWasmBytes || options.compilerWasmPath || options.compilerWasmUrl
  );
  const runnerWasm = await loadBytes(
    options.runnerWasmBytes || options.runnerWasmPath || options.runnerWasmUrl
  );

  const compilerInstantiated = await WebAssembly.instantiate(compilerWasm, {});
  const compilerInstance = compilerInstantiated.instance || compilerInstantiated;
  const compilerExports = compilerInstance.exports;

  const host = options.host || {};
  const events = [];
  const runnerImports = {
    devlish_host: {
      emit_event(ptr, len) {
        const event = JSON.parse(readString(runnerExports.memory, ptr, len));
        events.push(event);
        if (host.emitEvent) host.emitEvent(event);
      },
      write_file(ptr, len) {
        const request = JSON.parse(readString(runnerExports.memory, ptr, len));
        try {
          if (host.writeFile) {
            const result = host.writeFile(request);
            if (result && typeof result.then === "function") {
              throw new Error("host.writeFile must be synchronous in the Devlish WASM runner v0");
            }
            return result === false ? 1 : 0;
          }
          return 0;
        } catch {
          return 1;
        }
      }
    }
  };

  const runnerInstantiated = await WebAssembly.instantiate(runnerWasm, runnerImports);
  const runnerInstance = runnerInstantiated.instance || runnerInstantiated;
  const runnerExports = runnerInstance.exports;

  function compile(source) {
    const allocated = allocateString(compilerExports, source);
    const status = compilerExports.devlish_compile(allocated.ptr, allocated.len);
    compilerExports.devlish_free(allocated.ptr, allocated.len);

    const resultText = readString(
      compilerExports.memory,
      compilerExports.devlish_compile_result_ptr(),
      compilerExports.devlish_compile_result_len()
    );

    if (status === 0) {
      return { success: true, bytecode: JSON.parse(resultText) };
    }
    const error = JSON.parse(resultText);
    return { success: false, diagnostics: error.diagnostics || [] };
  }

  function run(bytecode, input = {}) {
    events.length = 0;
    const bytecodeStr = allocateString(runnerExports, JSON.stringify(bytecode));
    const inputStr = allocateString(runnerExports, JSON.stringify(input));
    const status = runnerExports.devlish_run(
      bytecodeStr.ptr, bytecodeStr.len,
      inputStr.ptr, inputStr.len
    );
    runnerExports.devlish_free(bytecodeStr.ptr, bytecodeStr.len);
    runnerExports.devlish_free(inputStr.ptr, inputStr.len);

    const resultText = readString(
      runnerExports.memory,
      runnerExports.devlish_result_ptr(),
      runnerExports.devlish_result_len()
    );
    const result = JSON.parse(resultText);
    result.status = status;
    return result;
  }

  return {
    compilerInstance,
    runnerInstance,
    events,
    compile,
    run,
    compileAndRun(source, input = {}) {
      const compiled = compile(source);
      if (!compiled.success) return compiled;
      return run(compiled.bytecode, input);
    }
  };
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
