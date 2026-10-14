// Runs an app the way the watch would: the NeoTime compiler turns its source into an image, and the
// simulator runs that image with `--stream`, sending frames back and taking touches.

import type { ChildProcess, SpawnOptions } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

// ============================================================================
// Settings
// ============================================================================

export const SCREEN_SIZE = 240;
const OUTPUT_FOLDER = path.join("target", "appbuilder");
const BUILT_TOOLS = ["neotime-compiler", "neotime-simulator"];

// ============================================================================
// Finding the watch code
// ============================================================================

/** The `neotime/` folder holding `start` or found below one of its parents, or nothing. */
export function find_neotime_folder(start: string): string | undefined {
  const is_neotime = (folder: string) =>
    fs.existsSync(path.join(folder, "compiler", "Cargo.toml")) && fs.existsSync(path.join(folder, "simulator", "Cargo.toml"));
  let folder = path.resolve(start);
  for (;;) {
    if (is_neotime(folder)) return folder;
    if (is_neotime(path.join(folder, "neotime"))) return path.join(folder, "neotime");
    const parent = path.dirname(folder);
    if (parent === folder) return undefined;
    folder = parent;
  }
}

// ============================================================================
// Compiler messages
// ============================================================================

export interface CompileError {
  file: string;
  /** Counted from 1, as the compiler prints them. */
  line: number;
  column: number;
  message: string;
}

/** Reads the compiler's `path:line:column: message` lines; any other line becomes an error with no place. */
export function parse_compile_errors(output: string, app_file: string): CompileError[] {
  return output
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => {
      const placed = /^(.*?):(\d+):(\d+): (.*)$/.exec(line);
      if (!placed) return { file: app_file, line: 1, column: 1, message: line };
      return { file: placed[1], line: Number(placed[2]), column: Number(placed[3]), message: placed[4] };
    });
}

// ============================================================================
// Simulator output
// ============================================================================

export type SimulatorLine = { kind: "frame"; pixels: Uint8ClampedArray } | { kind: "motor"; running: boolean } | { kind: "log"; text: string };

export function read_simulator_line(line: string): SimulatorLine {
  if (line.startsWith("frame ")) return { kind: "frame", pixels: decode_frame(line.slice("frame ".length)) };
  if (line === "motor on" || line === "motor off") return { kind: "motor", running: line === "motor on" };
  return { kind: "log", text: line };
}

/** Turns the simulator's little-endian RGB565 frame into RGBA for a canvas. */
export function decode_frame(base64: string): Uint8ClampedArray {
  const bytes = Buffer.from(base64, "base64");
  const pixels = new Uint8ClampedArray((bytes.length / 2) * 4);
  for (let place = 0; place < bytes.length / 2; place++) {
    const colour = bytes[2 * place] | (bytes[2 * place + 1] << 8);
    pixels[4 * place] = Math.round(((colour >> 11) * 255) / 31);
    pixels[4 * place + 1] = Math.round((((colour >> 5) & 63) * 255) / 63);
    pixels[4 * place + 2] = Math.round(((colour & 31) * 255) / 31);
    pixels[4 * place + 3] = 255;
  }
  return pixels;
}

/** Calls `on_line` with each whole line a stream writes, however its chunks are cut. */
export function split_lines(on_line: (line: string) => void): (chunk: Buffer | string) => void {
  let waiting = "";
  return (chunk) => {
    waiting += chunk.toString();
    const lines = waiting.split("\n");
    waiting = lines.pop()!;
    for (const line of lines) on_line(line);
  };
}

// ============================================================================
// The session
// ============================================================================

export type Spawn = (command: string, arguments_: string[], options: SpawnOptions) => ChildProcess;

export interface SessionListener {
  status(text: string): void;
  frame(pixels: Uint8ClampedArray): void;
  motor(running: boolean): void;
  log(text: string): void;
  /** The app's compile errors, or an empty list once it compiles again. */
  errors(errors: CompileError[]): void;
}

export class AppSession {
  private tools_ready: Promise<boolean> | undefined;
  private simulator: ChildProcess | undefined;
  private run_number = 0;

  constructor(
    private readonly neotime_folder: string,
    /** Put in front of cargo, such as `nix develop --command`; empty runs cargo as it is. */
    private readonly toolchain_command: string,
    private readonly spawn: Spawn,
    private readonly listener: SessionListener,
  ) {}

  private output_path(name: string): string {
    return path.join(this.neotime_folder, OUTPUT_FOLDER, name);
  }

  private tool_path(name: string): string {
    return path.join(this.neotime_folder, "target", "debug", name);
  }

  /** Runs a program to its end, giving its exit code and everything it wrote. */
  private run_to_end(command: string, arguments_: string[]): Promise<{ code: number; output: string }> {
    return new Promise((resolve) => {
      const child = this.spawn(command, arguments_, { cwd: this.neotime_folder });
      let output = "";
      child.stdout?.on("data", (chunk) => (output += chunk.toString()));
      child.stderr?.on("data", (chunk) => (output += chunk.toString()));
      child.on("error", (error) => resolve({ code: -1, output: output + error.message }));
      child.on("close", (code) => resolve({ code: code ?? -1, output }));
    });
  }

  /** Builds the compiler and simulator, and the shared library, once a session. */
  prepare(): Promise<boolean> {
    this.tools_ready ??= this.build_tools();
    return this.tools_ready;
  }

  private async build_tools(): Promise<boolean> {
    this.listener.status("Building the compiler and simulator…");
    const prefix = this.toolchain_command.split(/\s+/).filter((word) => word.length > 0);
    const cargo = [...prefix, "cargo", "build", "-q", ...BUILT_TOOLS.flatMap((tool) => ["-p", tool])];
    const build = await this.run_to_end(cargo[0], cargo.slice(1));
    if (build.code !== 0) {
      this.listener.status("The tools did not build");
      this.listener.log(build.output);
      this.tools_ready = undefined;
      return false;
    }
    fs.mkdirSync(path.join(this.neotime_folder, OUTPUT_FOLDER), { recursive: true });
    const library = await this.run_to_end(this.tool_path("neotime-compiler"), ["library", "-o", this.output_path("shared.ntlib")]);
    if (library.code !== 0) {
      this.listener.status("The shared library did not build");
      this.listener.log(library.output);
      this.tools_ready = undefined;
      return false;
    }
    return true;
  }

  /** Compiles the app and, if it compiles, restarts the simulator on it; a newer run supersedes this one. */
  async run(app_file: string): Promise<void> {
    const this_run = ++this.run_number;
    if (!(await this.prepare()) || this_run !== this.run_number) return;

    this.listener.status(`Compiling ${path.basename(app_file)}…`);
    const image = this.output_path("app.ntapp");
    const compile = await this.run_to_end(this.tool_path("neotime-compiler"), [app_file, "-o", image]);
    if (this_run !== this.run_number) return;
    if (compile.code !== 0) {
      const errors = parse_compile_errors(compile.output, app_file);
      this.listener.errors(errors);
      this.listener.status(`${errors.length} compile ${errors.length === 1 ? "error" : "errors"}; the screen shows the last good build`);
      return;
    }
    this.listener.errors([]);
    this.start_simulator(image);
  }

  private start_simulator(image: string) {
    this.stop();
    const simulator = this.spawn(this.tool_path("neotime-simulator"), ["--app", image, "--library", this.output_path("shared.ntlib"), "--stream"], {
      cwd: this.neotime_folder,
    });
    this.simulator = simulator;
    this.listener.status("Running");
    simulator.stdout?.on(
      "data",
      split_lines((line) => {
        const read = read_simulator_line(line);
        if (read.kind === "frame") this.listener.frame(read.pixels);
        else if (read.kind === "motor") this.listener.motor(read.running);
        else this.listener.log(read.text);
      }),
    );
    simulator.stderr?.on("data", split_lines((line) => this.listener.log(line)));
    simulator.on("close", (code) => {
      if (this.simulator !== simulator) return;
      this.simulator = undefined;
      this.listener.status(code === 0 ? "Stopped" : `The simulator stopped with code ${code}`);
    });
  }

  /** Sends a command such as `press 120 80` or `button` to the running simulator. */
  send(command: string) {
    this.simulator?.stdin?.write(`${command}\n`);
  }

  stop() {
    const simulator = this.simulator;
    this.simulator = undefined;
    if (simulator) {
      simulator.stdin?.end("quit\n");
      simulator.kill();
    }
  }
}
