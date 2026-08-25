import type { RuleInfo, ToolInfo } from "./types.js";

/**
 * Bytecode format versions this runtime can execute. When the compiler's
 * format changes incompatibly it bumps `format_version`; older runtimes
 * reject the artifact at load instead of failing mid-execution.
 */
export const SUPPORTED_FORMAT_VERSIONS = [0];

export class ArtifactError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ArtifactError";
  }
}

/**
 * Validates the artifact contract of a compiled Devlish bytecode package
 * and extracts its metadata. Throws ArtifactError on any violation so
 * bad artifacts fail at load, not mid-run.
 */
export function validateArtifact(bytecodeJson: string): ToolInfo {
  let pkg: Record<string, unknown>;
  try {
    pkg = JSON.parse(bytecodeJson);
  } catch (err) {
    throw new ArtifactError(
      `Bytecode is not valid JSON: ${err instanceof Error ? err.message : String(err)}`
    );
  }
  if (typeof pkg !== "object" || pkg === null || Array.isArray(pkg)) {
    throw new ArtifactError("Bytecode must be a JSON object");
  }

  if (pkg.format !== "devlish-bytecode") {
    throw new ArtifactError(
      `Not a Devlish bytecode package (format: ${JSON.stringify(pkg.format)})`
    );
  }
  const formatVersion = pkg.format_version;
  if (typeof formatVersion !== "number" || !SUPPORTED_FORMAT_VERSIONS.includes(formatVersion)) {
    throw new ArtifactError(
      `Unsupported bytecode format_version ${JSON.stringify(formatVersion)}; ` +
        `this runtime supports: ${SUPPORTED_FORMAT_VERSIONS.join(", ")}. ` +
        "Recompile the tool with a matching devlish compiler."
    );
  }
  if (!Array.isArray(pkg.instructions)) {
    throw new ArtifactError("Bytecode is missing its 'instructions' array");
  }
  if (!Array.isArray(pkg.constant_pool)) {
    throw new ArtifactError("Bytecode is missing its 'constant_pool' array");
  }

  const manifest = pkg.manifest as
    | { permissions?: Array<{ kind?: string }>; rule?: Record<string, unknown> }
    | undefined;
  const permissions = Array.isArray(manifest?.permissions)
    ? manifest.permissions
        .map((p) => (typeof p?.kind === "string" ? p.kind : null))
        .filter((k): k is string => k !== null)
    : [];

  const str = (value: unknown): string | null =>
    typeof value === "string" ? value : null;

  let rule: RuleInfo | null = null;
  const rawRule = manifest?.rule;
  if (rawRule && typeof rawRule === "object") {
    const id = str(rawRule.id);
    const version = str(rawRule.version);
    // id and version are required by the compiler for any governed rule.
    if (id !== null && version !== null) {
      rule = {
        id,
        version,
        author: str(rawRule.author),
        effectiveFrom: str(rawRule.effective_from),
        effectiveUntil: str(rawRule.effective_until),
      };
    }
  }

  return {
    formatVersion,
    compilerVersion: str(pkg.compiler_version),
    sourceHash: str(pkg.source_hash),
    sourcePath: str(pkg.source_path),
    permissions,
    rule,
  };
}

/**
 * Verifies the artifact bytes against a caller-supplied SHA-256 hex digest.
 * Uses WebCrypto, which is available in browsers and Node 19+.
 */
export async function verifySha256(
  bytecodeJson: string,
  expectedSha256: string
): Promise<void> {
  if (!globalThis.crypto?.subtle) {
    throw new ArtifactError(
      "SHA-256 verification requires WebCrypto (a secure context, or Node 19+). " +
        "Load without expectedSha256, or verify the artifact upstream."
    );
  }
  const digest = await globalThis.crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(bytecodeJson)
  );
  const actual = Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
  const expected = expectedSha256.toLowerCase();
  if (actual !== expected) {
    throw new ArtifactError(
      `Bytecode integrity check failed: expected sha256 ${expected}, got ${actual}. ` +
        "The artifact was modified or the wrong file was loaded."
    );
  }
}

/** A candidate rule version returned by {@link selectVersion}. */
export interface RuleVersion {
  /** The artifact as passed in (JSON string or object). */
  bytecode: unknown;
  /** Governance info parsed from the artifact. `info.rule` is always present. */
  info: ToolInfo;
}

/**
 * True when `value` is a real `YYYY-MM-DD` calendar date. Mirrors the Rust
 * compiler's `parse_iso_date` (month lengths + leap years) so the runtime and
 * the compiler agree on what a valid effective date is.
 */
export function isValidIsoDate(value: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return false;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  if (month < 1 || month > 12) return false;
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const lengths = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return day >= 1 && day <= lengths[month - 1];
}

/**
 * Selects the single rule version in force on `asOfDate` from a set of
 * artifacts that must all be versions of the same governed rule id. This is
 * how a compliance recomputation runs the rule that was legally in force on a
 * transaction date rather than the latest version.
 *
 * ISO dates are fixed-width, so effective windows compare lexically. A missing
 * `effectiveFrom`/`effectiveUntil` means open-ended in that direction.
 *
 * Throws `ArtifactError` if `asOfDate` is not a real date, if any artifact is
 * ungoverned or names a different rule id, if no version is in force, or if
 * more than one version's window covers the date (overlapping windows are a
 * publish-time error, but they are rejected here too).
 */
export function selectVersion(artifacts: unknown[], asOfDate: string): RuleVersion {
  if (!isValidIsoDate(asOfDate)) {
    throw new ArtifactError(`as-of date '${asOfDate}' must be a real YYYY-MM-DD date`);
  }
  if (artifacts.length === 0) {
    throw new ArtifactError("selectVersion requires at least one artifact");
  }

  const candidates: RuleVersion[] = artifacts.map((bytecode) => {
    const bytecodeJson = typeof bytecode === "string" ? bytecode : JSON.stringify(bytecode);
    const info = validateArtifact(bytecodeJson);
    if (!info.rule) {
      throw new ArtifactError(
        "selectVersion needs governed artifacts (a Rule: section); found one with no rule metadata"
      );
    }
    return { bytecode, info };
  });

  const ruleId = candidates[0].info.rule!.id;
  for (const candidate of candidates) {
    if (candidate.info.rule!.id !== ruleId) {
      throw new ArtifactError(
        `selectVersion needs one rule id; got '${ruleId}' and '${candidate.info.rule!.id}'`
      );
    }
  }

  const inForce = candidates.filter(({ info }) => {
    const { effectiveFrom, effectiveUntil } = info.rule!;
    return (
      (effectiveFrom === null || asOfDate >= effectiveFrom) &&
      (effectiveUntil === null || asOfDate <= effectiveUntil)
    );
  });

  if (inForce.length === 0) {
    throw new ArtifactError(`no version of ${ruleId} is in force on ${asOfDate}`);
  }
  if (inForce.length > 1) {
    const versions = inForce.map((c) => c.info.rule!.version).join(", ");
    throw new ArtifactError(
      `multiple versions of ${ruleId} are in force on ${asOfDate} (overlapping windows): ${versions}`
    );
  }
  return inForce[0];
}
