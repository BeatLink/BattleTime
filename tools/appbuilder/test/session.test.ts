import type { ChildProcess } from "node:child_process";
import { EventEmitter } from "node:events";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { PassThrough } from "node:stream";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  AppSession,
  decode_frame,
  find_battletime_folder,
  parse_compile_errors,
  read_simulator_line,
  split_lines,
  OLD_SIMULATOR_MESSAGE,
  type CompileError,
  type Spawn,
} from "../src/session";

// ============================================================================
// A pretend set of programs
// ============================================================================

interface FakeProcess {
  command: string;
  arguments_: string[];
  child: ChildProcess;
  stdout: PassThrough;
  stdin: PassThrough;
  written: string[];
  killed: boolean;
}

/** Spawns pretend programs; `behave` decides each one's output and exit code, or leaves it running. */
function fake_spawn(behave: (process: FakeProcess) => { code: number; output?: string } | "keep running") {
  const started: FakeProcess[] = [];
  const spawn: Spawn = (command, arguments_) => {
    const emitter = new EventEmitter();
    const stdout = new PassThrough();
    const stderr = new PassThrough();
    const stdin = new PassThrough();
    const fake: FakeProcess = { command, arguments_, child: emitter as ChildProcess, stdout, stdin, written: [], killed: false };
    stdin.on("data", (chunk) => fake.written.push(chunk.toString()));
    Object.assign(emitter, {
      stdout,
      stderr,
      stdin,
      kill: () => {
        fake.killed = true;
        emitter.emit("close", null);
        return true;
      },
    });
    started.push(fake);
    setImmediate(() => {
      const result = behave(fake);
      if (result === "keep running") return;
      if (result.output) stdout.write(result.output);
      setImmediate(() => emitter.emit("close", result.code));
    });
    return fake.child;
  };
  return { spawn, started };
}

function recording_listener() {
  const heard = { statuses: [] as string[], frames: [] as Uint8ClampedArray[], motor: [] as boolean[], logs: [] as string[], errors: [] as CompileError[][] };
  const listener = {
    status: (text: string) => heard.statuses.push(text),
    frame: (pixels: Uint8ClampedArray) => heard.frames.push(pixels),
    motor: (running: boolean) => heard.motor.push(running),
    log: (text: string) => heard.logs.push(text),
    errors: (errors: CompileError[]) => heard.errors.push(errors),
  };
  return { heard, listener };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

let battletime_folder: string;

beforeEach(() => {
  battletime_folder = fs.mkdtempSync(path.join(os.tmpdir(), "appbuilder-"));
});

afterEach(() => {
  fs.rmSync(battletime_folder, { recursive: true, force: true });
});

// ============================================================================
// Pieces
// ============================================================================

describe("finding the watch code", () => {
  it("finds battletime/ from a file inside it or from the repository above it", () => {
    const repository = fs.mkdtempSync(path.join(os.tmpdir(), "appbuilder-repo-"));
    const battletime = path.join(repository, "battletime");
    for (const crate of ["compiler", "simulator"]) {
      fs.mkdirSync(path.join(battletime, crate), { recursive: true });
      fs.writeFileSync(path.join(battletime, crate, "Cargo.toml"), "");
    }
    fs.mkdirSync(path.join(battletime, "apps", "hello"), { recursive: true });

    expect(find_battletime_folder(path.join(battletime, "apps", "hello"))).toBe(battletime);
    expect(find_battletime_folder(repository)).toBe(battletime);
    expect(find_battletime_folder(os.tmpdir())).toBeUndefined();
    fs.rmSync(repository, { recursive: true });
  });
});

describe("compiler messages", () => {
  it("read their file, line and column", () => {
    const output = "/a/app.tsx:20:1: `Bogus` is not defined; import it from battletime/widgets\n";
    expect(parse_compile_errors(output, "/a/app.tsx")).toEqual([
      { file: "/a/app.tsx", line: 20, column: 1, message: "`Bogus` is not defined; import it from battletime/widgets" },
    ]);
  });

  it("put a message with no place at the top of the app", () => {
    expect(parse_compile_errors("could not read it\n\n", "/a/app.tsx")).toEqual([{ file: "/a/app.tsx", line: 1, column: 1, message: "could not read it" }]);
  });
});

describe("simulator output", () => {
  it("decodes RGB565 into RGBA", () => {
    const red_then_white = Buffer.from([0x00, 0xf8, 0xff, 0xff]).toString("base64");
    expect([...decode_frame(red_then_white)]).toEqual([255, 0, 0, 255, 255, 255, 255, 255]);
  });

  it("tells frames, the motor and log text apart", () => {
    const frame = read_simulator_line(`frame ${Buffer.from([0x1f, 0x00]).toString("base64")}`);
    expect(frame.kind === "frame" && [...frame.pixels]).toEqual([0, 0, 255, 255]);
    expect(read_simulator_line("motor on")).toEqual({ kind: "motor", running: true });
    expect(read_simulator_line("motor off")).toEqual({ kind: "motor", running: false });
    expect(read_simulator_line("effect: Vibrate(30)")).toEqual({ kind: "log", text: "effect: Vibrate(30)" });
  });

  it("is split into whole lines however the chunks fall", () => {
    const lines: string[] = [];
    const take = split_lines((line) => lines.push(line));
    take("fir");
    take("st\nsec");
    take(Buffer.from("ond\nthird"));
    expect(lines).toEqual(["first", "second"]);
  });
});

// ============================================================================
// The session
// ============================================================================

describe("a session", () => {
  const app = "/apps/hello/app.tsx";

  it("builds the tools once, compiles the app and runs it in the simulator", async () => {
    const pixel = Buffer.from([0x00, 0xf8]).toString("base64");
    const { spawn, started } = fake_spawn((process) => {
      if (process.command.endsWith("battletime-simulator")) {
        process.stdout.write(`frame ${pixel}\nmotor on\neffect: Vibrate(30)\n`);
        return "keep running";
      }
      return { code: 0 };
    });
    const { heard, listener } = recording_listener();
    const session = new AppSession(battletime_folder, "nix develop --command", spawn, listener);

    await session.run(app);
    await session.run(app);
    await settle();

    const commands = started.map((process) => [path.basename(process.command), ...process.arguments_.slice(0, 3)]);
    expect(commands[0]).toEqual(["nix", "develop", "--command", "cargo"]);
    expect(started[0].arguments_).toEqual(["develop", "--command", "cargo", "build", "-q", "-p", "battletime-compiler", "-p", "battletime-simulator"]);
    expect(commands[1]).toEqual(["battletime-compiler", "library", "-o", path.join(battletime_folder, "target/appbuilder/shared.ntlib")]);
    expect(commands[2]).toEqual(["battletime-compiler", app, "-o", path.join(battletime_folder, "target/appbuilder/app.ntapp")]);
    expect(commands[3][0]).toBe("battletime-simulator");
    expect(started[3].arguments_).toContain("--stream");
    expect(commands.slice(4).map((command) => command[0])).toEqual(["battletime-compiler", "battletime-simulator"]);
    expect(started[3].killed).toBe(true);
    expect(started[3].written.join("")).toBe("quit\n");

    expect(heard.frames.map((frame) => [...frame])).toEqual([[255, 0, 0, 255], [255, 0, 0, 255]]);
    expect(heard.motor).toEqual([true, true]);
    expect(heard.logs).toContain("effect: Vibrate(30)");
    expect(heard.errors).toEqual([[], []]);
    expect(heard.statuses.at(-1)).toBe("Running");

    session.send("press 1 2");
    expect(started[5].written.join("")).toBe("press 1 2\n");
    session.stop();
    expect(started[5].killed).toBe(true);
    session.send("button");
  });

  it("runs cargo as it is with no toolchain command", async () => {
    const { spawn, started } = fake_spawn(() => ({ code: 0 }));
    const session = new AppSession(battletime_folder, "  ", spawn, recording_listener().listener);
    await session.prepare();
    expect(started[0].command).toBe("cargo");
  });

  it("reports compile errors and leaves the simulator as it was", async () => {
    const { spawn, started } = fake_spawn((process) =>
      process.arguments_[0] === app ? { code: 1, output: `${app}:3:5: \`Bogus\` is not defined\n` } : { code: 0 },
    );
    const { heard, listener } = recording_listener();
    await new AppSession(battletime_folder, "", spawn, listener).run(app);

    expect(heard.errors).toEqual([[{ file: app, line: 3, column: 5, message: "`Bogus` is not defined" }]]);
    expect(heard.statuses.at(-1)).toMatch(/^1 compile error;/);
    expect(started.some((process) => process.command.endsWith("battletime-simulator"))).toBe(false);
  });

  it("reports a failed build and tries again next time", async () => {
    let builds = 0;
    const { spawn } = fake_spawn((process) => (process.command === "cargo" && builds++ === 0 ? { code: 101, output: "error[E0308]" } : { code: 0 }));
    const { heard, listener } = recording_listener();
    const session = new AppSession(battletime_folder, "", spawn, listener);

    expect(await session.prepare()).toBe(false);
    expect(heard.statuses).toContain("The tools did not build");
    expect(heard.logs).toContain("error[E0308]");
    expect(await session.prepare()).toBe(true);
  });

  it("reports a shared library that does not build", async () => {
    const { spawn } = fake_spawn((process) => (process.arguments_[0] === "library" ? { code: 1, output: "no fonts" } : { code: 0 }));
    const { heard, listener } = recording_listener();
    expect(await new AppSession(battletime_folder, "", spawn, listener).prepare()).toBe(false);
    expect(heard.statuses).toContain("The shared library did not build");
  });

  it("reports a program that cannot start", async () => {
    const spawn: Spawn = () => {
      const emitter = new EventEmitter();
      setImmediate(() => emitter.emit("error", new Error("spawn cargo ENOENT")));
      return emitter as ChildProcess;
    };
    const { heard, listener } = recording_listener();
    expect(await new AppSession(battletime_folder, "", spawn, listener).prepare()).toBe(false);
    expect(heard.logs).toContain("spawn cargo ENOENT");
  });

  it("drops a run that a newer one overtook", async () => {
    const { spawn, started } = fake_spawn((process) => (process.command.endsWith("battletime-simulator") ? "keep running" : { code: 0 }));
    const session = new AppSession(battletime_folder, "", spawn, recording_listener().listener);
    await Promise.all([session.run("/first.tsx"), session.run("/second.tsx")]);

    const compiled = started.filter((process) => process.command.endsWith("battletime-compiler") && process.arguments_[0] !== "library");
    expect(compiled.map((process) => process.arguments_[0])).toEqual(["/second.tsx"]);
    session.stop();
  });

  it("says when the simulator stops by itself", async () => {
    const { spawn } = fake_spawn((process) => (process.command.endsWith("battletime-simulator") ? { code: 3 } : { code: 0 }));
    const { heard, listener } = recording_listener();
    await new AppSession(battletime_folder, "", spawn, listener).run(app);
    await settle();
    expect(heard.statuses.at(-1)).toBe("The simulator stopped with code 3");
  });

  it("says plainly when the checkout's simulator is too old to stream", async () => {
    const { spawn } = fake_spawn((process) => {
      if (!process.command.endsWith("battletime-simulator")) return { code: 0 };
      (process.child.stderr as PassThrough).write('Error: "unknown option --stream; use --screenshot PATH"\n');
      return { code: 1 };
    });
    const { heard, listener } = recording_listener();
    await new AppSession(battletime_folder, "", spawn, listener).run(app);
    await settle();
    expect(heard.statuses.at(-1)).toBe(OLD_SIMULATOR_MESSAGE);
    expect(heard.logs.some((line) => line.includes("unknown option --stream"))).toBe(true);
  });
});
