/**
 * Browser-compatible host configuration for the WASM runner.
 * Replaces filesystem-dependent operations with in-memory equivalents.
 */

import { getFixture } from "./fixtures";

export interface WasmHostConfig {
  readFile: (path: string) => string | null;
  writeFile: (path: string, content: string) => void;
  outputs: string[];
}

/**
 * Create a browser host that resolves Load statements from bundled fixtures
 * and captures file writes in memory.
 */
export function createBrowserHost(): WasmHostConfig {
  const outputs: string[] = [];

  return {
    readFile(path: string): string | null {
      return getFixture(path);
    },
    writeFile(_path: string, content: string): void {
      outputs.push(content);
    },
    outputs,
  };
}
