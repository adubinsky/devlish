export interface LoadToolOptions {
  /** Compiled Devlish bytecode (JSON object or string). */
  bytecode: unknown;
  /** Maximum instructions before termination. Default: 10_000_000. */
  instructionLimit?: number;
  /** Run on the main thread instead of a Web Worker. Default: false. */
  mainThread?: boolean;
  /**
   * SHA-256 hex digest of the bytecode JSON string. When set, loadTool
   * verifies the artifact bytes before instantiating and throws
   * ArtifactError on mismatch (tamper/corruption detection).
   */
  expectedSha256?: string;
  /**
   * Called once per run of a governed rule (a `Rule:` manifest section)
   * with an execution provenance record, so embedding apps persist audit
   * trails in their own store. Ungoverned tools never invoke it. A run
   * that dies in a WASM trap never reaches the audit path, so trapped
   * results carry no record.
   */
  onAuditRecord?: (record: AuditRecord) => void;
}

/**
 * Execution provenance record emitted once per governed run, binding the
 * output to the exact rule version and artifact that produced it. Matches
 * the record shape the native runner appends via `--audit-log`, except
 * `prev_sha256` (hash chaining), which is added by whichever store persists
 * the log.
 */
export interface AuditRecord {
  /** SHA-256 of the canonical (sorted-keys pretty) bytecode -- the same form evidence bundles hash. */
  artifact_sha256: string;
  /** SHA-256 of the canonical JSON serialization of the run input. */
  input_sha256: string;
  /** Instructions executed by the VM during this run. */
  instruction_count: number;
  /** SHA-256 of the canonical JSON serialization of the result the caller observes. */
  output_sha256: string;
  /** Dotted rule identifier from the `Rule:` section. */
  rule_id: string;
  /** Semantic version of the rule that ran. */
  rule_version: string;
  /** Runtime that executed the rule. */
  runtime: { kind: "wasm" | "native"; version: string };
  /** Whether the run completed successfully. */
  success: boolean;
  /**
   * Present (true) when the run paused at a checkpoint rather than
   * completing; the resumed run emits its own record.
   */
  paused?: boolean;
  /** Unix timestamp (seconds) stamped at record delivery. */
  timestamp: number;
}

/** Metadata extracted from a compiled bytecode artifact at load time. */
export interface ToolInfo {
  /** Bytecode format version the artifact was compiled to. */
  formatVersion: number;
  /** Version of the devlish compiler that produced the artifact. */
  compilerVersion: string | null;
  /** SHA-256 of the original .dvl source, as recorded by the compiler. */
  sourceHash: string | null;
  /** Source path recorded by the compiler, if any. */
  sourcePath: string | null;
  /** Permission kinds declared in the artifact's manifest. */
  permissions: string[];
  /** Rule governance identity, present only for governed rules (a `Rule:` section). */
  rule: RuleInfo | null;
}

/** Governance identity of a rule, from its `Rule:` manifest section. */
export interface RuleInfo {
  /** Dotted rule identifier, e.g. `credit_verification.dti_check`. */
  id: string;
  /** Semantic version of this rule. */
  version: string;
  /** Declared author, if any. */
  author: string | null;
  /** Inclusive first day this version is in force (`YYYY-MM-DD`), if declared. */
  effectiveFrom: string | null;
  /** Inclusive last day this version is in force (`YYYY-MM-DD`), if declared. */
  effectiveUntil: string | null;
}

export interface RunResult {
  success: boolean;
  error?: string;
  context?: Record<string, unknown>;
  results?: Record<string, unknown>;
  responded?: boolean;
  response?: unknown;
  /**
   * Set when the run died in a WASM trap (Rust panic). The runtime replaces
   * the WASM instance before the next run, so callers only need to retry.
   */
  trapped?: boolean;
}

export interface DevlishTool {
  /** Metadata extracted from the artifact at load time. */
  info: ToolInfo;
  /** Execute the tool with the given input context. */
  run(input?: Record<string, unknown>): Promise<RunResult>;
  /** Release resources (terminates the worker if applicable). */
  dispose(): void;
}
