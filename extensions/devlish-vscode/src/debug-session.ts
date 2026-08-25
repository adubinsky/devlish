import * as vscode from "vscode";
import { execFile } from "child_process";

// ── DAP protocol types (minimal subset) ──────────────────────────

interface DapMessage {
  seq: number;
  type: string;
}

interface DapRequest extends DapMessage {
  type: "request";
  command: string;
  arguments?: Record<string, unknown>;
}

interface DapResponse extends DapMessage {
  type: "response";
  request_seq: number;
  command: string;
  success: boolean;
  body?: Record<string, unknown>;
  message?: string;
}

interface DapEvent extends DapMessage {
  type: "event";
  event: string;
  body?: Record<string, unknown>;
}

// ── Timeline snapshot ────────────────────────────────────────────

interface TimelineEntry {
  line: number;
  variables: Map<string, string>;
  output: string | undefined;
}

// ── VM event types ───────────────────────────────────────────────

interface VmEvent {
  type: string;
  address?: number;
  pc?: number;
  op?: string;
  symbol?: string;
  value?: unknown;
}

// ── Debug session ────────────────────────────────────────────────

export class DevlishDebugSession implements vscode.DebugAdapter {
  private sendMessageEmitter = new vscode.EventEmitter<DapResponse | DapEvent>();
  readonly onDidSendMessage = this.sendMessageEmitter.event;

  private seq = 1;
  private timeline: TimelineEntry[] = [];
  private timelineIndex = -1;
  private breakpoints = new Map<string, Set<number>>();
  private programPath = "";
  private disposed = false;

  // ── DAP message handling ─────────────────────────────────────

  handleMessage(message: DapMessage): void {
    if (message.type !== "request") {
      return;
    }
    const request = message as DapRequest;

    switch (request.command) {
      case "initialize":
        this.onInitialize(request);
        break;
      case "launch":
        this.onLaunch(request);
        break;
      case "setBreakpoints":
        this.onSetBreakpoints(request);
        break;
      case "configurationDone":
        this.onConfigurationDone(request);
        break;
      case "threads":
        this.onThreads(request);
        break;
      case "stackTrace":
        this.onStackTrace(request);
        break;
      case "scopes":
        this.onScopes(request);
        break;
      case "variables":
        this.onVariables(request);
        break;
      case "continue":
        this.onContinue(request);
        break;
      case "next":
      case "stepIn":
        this.onNext(request);
        break;
      case "pause":
        this.sendResponse(request, {});
        break;
      case "disconnect":
        this.onDisconnect(request);
        break;
      default:
        this.sendResponse(request, {});
        break;
    }
  }

  dispose(): void {
    this.disposed = true;
    this.sendMessageEmitter.dispose();
  }

  // ── DAP handlers ────────────────────────────────────────────

  private onInitialize(request: DapRequest): void {
    this.sendResponse(request, {
      supportsConfigurationDoneRequest: true,
      supportsFunctionBreakpoints: false,
      supportsConditionalBreakpoints: false,
      supportsEvaluateForHovers: false,
      supportsStepBack: false,
      supportsRestartRequest: false,
    });
    this.sendEvent("initialized");
  }

  private onLaunch(request: DapRequest): void {
    const args = request.arguments ?? {};
    this.programPath = args.program as string ?? "";
    const input = args.input as Record<string, unknown> | undefined;

    if (!this.programPath) {
      this.sendErrorResponse(request, "No program specified in launch configuration.");
      return;
    }

    this.sendResponse(request, {});

    this.runProgram(this.programPath, input).then(
      () => {
        if (this.disposed) {
          return;
        }
        if (this.timeline.length === 0) {
          this.sendEvent("terminated");
          return;
        }
        // Stop at first breakpoint or first line
        const firstStop = this.findNextBreakpoint(0);
        if (firstStop >= 0) {
          this.timelineIndex = firstStop;
        } else {
          this.timelineIndex = 0;
        }
        this.sendEvent("stopped", { reason: "entry", threadId: 1 });
      },
      (error) => {
        if (this.disposed) {
          return;
        }
        this.sendEvent("output", {
          category: "stderr",
          output: `Error: ${error}\n`,
        });
        this.sendEvent("terminated");
      }
    );
  }

  private onSetBreakpoints(request: DapRequest): void {
    const args = request.arguments ?? {};
    const source = args.source as { path?: string } | undefined;
    const sourceBreakpoints = args.breakpoints as Array<{ line: number }> | undefined;

    const path = source?.path ?? "";
    const lines = new Set<number>();
    const verifiedBreakpoints: Array<{ verified: boolean; line: number }> = [];

    if (sourceBreakpoints) {
      for (const bp of sourceBreakpoints) {
        lines.add(bp.line);
        // A breakpoint is verified if the line appears in our timeline
        const exists = this.timeline.some((entry) => entry.line === bp.line);
        verifiedBreakpoints.push({ verified: exists || this.timeline.length === 0, line: bp.line });
      }
    }

    this.breakpoints.set(path, lines);
    this.sendResponse(request, { breakpoints: verifiedBreakpoints });
  }

  private onConfigurationDone(request: DapRequest): void {
    this.sendResponse(request, {});
  }

  private onThreads(request: DapRequest): void {
    this.sendResponse(request, {
      threads: [{ id: 1, name: "Main Thread" }],
    });
  }

  private onStackTrace(request: DapRequest): void {
    const entry = this.currentEntry();
    if (!entry) {
      this.sendResponse(request, { stackFrames: [], totalFrames: 0 });
      return;
    }

    this.sendResponse(request, {
      stackFrames: [
        {
          id: 1,
          name: "main",
          source: { name: this.baseName(this.programPath), path: this.programPath },
          line: entry.line,
          column: 1,
        },
      ],
      totalFrames: 1,
    });
  }

  private onScopes(request: DapRequest): void {
    this.sendResponse(request, {
      scopes: [
        {
          name: "Locals",
          variablesReference: 1,
          expensive: false,
        },
      ],
    });
  }

  private onVariables(request: DapRequest): void {
    const entry = this.currentEntry();
    const vars: Array<{ name: string; value: string; variablesReference: number }> = [];

    if (entry) {
      for (const [name, value] of entry.variables) {
        vars.push({ name, value, variablesReference: 0 });
      }
    }

    this.sendResponse(request, { variables: vars });
  }

  private onContinue(request: DapRequest): void {
    this.sendResponse(request, { allThreadsContinued: true });

    const next = this.findNextBreakpoint(this.timelineIndex + 1);
    if (next >= 0) {
      this.timelineIndex = next;
      this.sendEvent("stopped", { reason: "breakpoint", threadId: 1 });
    } else {
      this.timelineIndex = this.timeline.length;
      this.sendEvent("terminated");
    }
  }

  private onNext(request: DapRequest): void {
    this.sendResponse(request, {});

    const nextIndex = this.timelineIndex + 1;
    if (nextIndex < this.timeline.length) {
      this.timelineIndex = nextIndex;
      // Emit output if this step produced any
      const entry = this.timeline[nextIndex];
      if (entry.output !== undefined) {
        this.sendEvent("output", {
          category: "stdout",
          output: entry.output + "\n",
        });
      }
      this.sendEvent("stopped", { reason: "step", threadId: 1 });
    } else {
      this.timelineIndex = this.timeline.length;
      this.sendEvent("terminated");
    }
  }

  private onDisconnect(request: DapRequest): void {
    this.sendResponse(request, {});
  }

  // ── Program execution and timeline building ──────────────────

  private async runProgram(
    programPath: string,
    input: Record<string, unknown> | undefined
  ): Promise<void> {
    const cliPath = this.getCliPath();

    // Step 1: Compile to get source_map
    const sourceMap = await this.compileForSourceMap(cliPath, programPath);

    // Step 2: Run and collect events
    const events = await this.executeAndCollectEvents(cliPath, programPath, input);

    // Step 3: Build timeline from events using source_map
    this.buildTimeline(events, sourceMap);
  }

  private compileForSourceMap(
    cliPath: string,
    programPath: string
  ): Promise<Map<number, number>> {
    return new Promise((resolve, reject) => {
      execFile(
        cliPath,
        ["compile", programPath],
        { maxBuffer: 10 * 1024 * 1024, timeout: 30_000 },
        (error, stdout) => {
          if (error) {
            reject(`Compilation failed: ${error.message}`);
            return;
          }
          const addressToLine = new Map<number, number>();
          try {
            const pkg = JSON.parse(stdout);
            const sm = pkg.source_map as Array<{ address: number; line?: number }> | undefined;
            if (sm) {
              for (const entry of sm) {
                if (entry.line !== undefined) {
                  addressToLine.set(entry.address, entry.line);
                }
              }
            }
          } catch {
            // If parsing fails, we proceed without line mapping
          }
          resolve(addressToLine);
        }
      );
    });
  }

  private executeAndCollectEvents(
    cliPath: string,
    programPath: string,
    input: Record<string, unknown> | undefined
  ): Promise<VmEvent[]> {
    return new Promise((resolve, reject) => {
      const args = ["run", programPath];
      if (input && Object.keys(input).length > 0) {
        args.push("--input", JSON.stringify(input));
      }

      execFile(
        cliPath,
        args,
        { maxBuffer: 10 * 1024 * 1024, timeout: 60_000 },
        (error, stdout, stderr) => {
          // Parse events from stderr even if there was a non-zero exit
          const events: VmEvent[] = [];
          if (stderr) {
            for (const line of stderr.split("\n")) {
              const trimmed = line.trim();
              if (!trimmed) {
                continue;
              }
              try {
                events.push(JSON.parse(trimmed) as VmEvent);
              } catch {
                // skip non-JSON lines
              }
            }
          }

          // Parse stdout for any output/error info
          if (stdout && stdout.trim()) {
            try {
              const result = JSON.parse(stdout);
              if (result.success === false && result.error) {
                // Still resolve with events collected so far, but emit the error
                events.push({
                  type: "run_failed",
                  value: result.error,
                });
              }
            } catch {
              // ignore
            }
          }

          if (error && events.length === 0) {
            if ((error as NodeJS.ErrnoException).code === "ENOENT") {
              reject(`devlish-core binary not found at "${cliPath}".`);
            } else {
              reject(error.message);
            }
            return;
          }

          resolve(events);
        }
      );
    });
  }

  private buildTimeline(
    events: VmEvent[],
    sourceMap: Map<number, number>
  ): void {
    this.timeline = [];
    const currentVars = new Map<string, string>();
    let lastLine = -1;
    let pendingOutput: string | undefined;

    for (const event of events) {
      switch (event.type) {
        case "instruction_started": {
          const address = event.address ?? event.pc ?? -1;
          const line = sourceMap.get(address);
          if (line !== undefined && line !== lastLine) {
            // New source line reached: snapshot the current state
            this.timeline.push({
              line,
              variables: new Map(currentVars),
              output: pendingOutput,
            });
            pendingOutput = undefined;
            lastLine = line;
          }
          break;
        }
        case "variable_assigned": {
          const symbol = event.symbol;
          const value = event.value;
          if (symbol) {
            currentVars.set(symbol, formatValue(value));
          }
          // Update the most recent timeline entry with the new variable
          if (this.timeline.length > 0) {
            this.timeline[this.timeline.length - 1].variables = new Map(currentVars);
          }
          break;
        }
        case "output_emitted": {
          const output = formatValue(event.value);
          pendingOutput = pendingOutput !== undefined ? pendingOutput + "\n" + output : output;
          // Attach to current timeline entry
          if (this.timeline.length > 0) {
            const entry = this.timeline[this.timeline.length - 1];
            entry.output = entry.output !== undefined ? entry.output + "\n" + output : output;
          }
          break;
        }
        case "run_failed": {
          // Add a final entry with the error
          if (lastLine >= 0) {
            this.timeline.push({
              line: lastLine,
              variables: new Map(currentVars),
              output: `[error] ${formatValue(event.value)}`,
            });
          }
          break;
        }
      }
    }

    // Deduplicate consecutive entries on the same line (keep last snapshot per line visit)
    // Actually, keep all entries so stepping shows each line transition
  }

  // ── Helpers ──────────────────────────────────────────────────

  private currentEntry(): TimelineEntry | undefined {
    if (this.timelineIndex >= 0 && this.timelineIndex < this.timeline.length) {
      return this.timeline[this.timelineIndex];
    }
    return undefined;
  }

  private findNextBreakpoint(fromIndex: number): number {
    // Collect all breakpoint lines across all sources
    const allBreakpointLines = new Set<number>();
    for (const lines of this.breakpoints.values()) {
      for (const line of lines) {
        allBreakpointLines.add(line);
      }
    }

    if (allBreakpointLines.size === 0) {
      return -1;
    }

    for (let i = fromIndex; i < this.timeline.length; i++) {
      if (allBreakpointLines.has(this.timeline[i].line)) {
        return i;
      }
    }
    return -1;
  }

  private getCliPath(): string {
    const configured = vscode.workspace
      .getConfiguration("devlish")
      .get<string>("cliPath");
    if (configured && configured.trim()) {
      return configured.trim();
    }
    return "devlish-core";
  }

  private baseName(filePath: string): string {
    const parts = filePath.replace(/\\/g, "/").split("/");
    return parts[parts.length - 1] ?? filePath;
  }

  // ── DAP message sending ──────────────────────────────────────

  private sendResponse(
    request: DapRequest,
    body: Record<string, unknown>
  ): void {
    const response: DapResponse = {
      seq: this.seq++,
      type: "response",
      request_seq: request.seq,
      command: request.command,
      success: true,
      body,
    };
    this.sendMessageEmitter.fire(response);
  }

  private sendErrorResponse(request: DapRequest, message: string): void {
    const response: DapResponse = {
      seq: this.seq++,
      type: "response",
      request_seq: request.seq,
      command: request.command,
      success: false,
      message,
    };
    this.sendMessageEmitter.fire(response);
  }

  private sendEvent(event: string, body?: Record<string, unknown>): void {
    const msg: DapEvent = {
      seq: this.seq++,
      type: "event",
      event,
      body,
    };
    this.sendMessageEmitter.fire(msg);
  }
}

function formatValue(value: unknown): string {
  if (value === null || value === undefined) {
    return "null";
  }
  if (typeof value === "string") {
    return value;
  }
  return JSON.stringify(value);
}
