# Compiling app bytecode to native code

Apps run as bytecode in the interpreter ([app-runtime.md](app-runtime.md)). For the few that
compute hard, such as Game of Life's generation step, which visits every cell in one loop, the
watch can translate the app's verified bytecode into Thumb-2 machine code once, at install, and run
that instead. This page is the design; [external-flash-architecture.md](external-flash-architecture.md)
places it as stage 8 of the load-and-go plan.

## What it must keep

Native code must behave exactly as the interpreter does, so an app never sees a difference but
speed:

- **Values** stay 64-bit integers, with the interpreter's overflow, division and shift checks, and
  `NONE` as `i64::MIN`. Text stays a reference into the interpreter's 256-byte text buffer, in the
  same encoding, since the app host reads results and effect texts from there after a run.
- **Failures** are the interpreter's `Failure` values, raised at the same instruction. A failed
  handler leaves the state as it was: writes go to the interpreter's working copies, which only a
  successful run puts back.
- **Steps**: a run still stops with `TooManySteps` after 200,000 bytecode instructions. Native code
  counts each basic block's instruction count on entry, so the count is exact, not estimated.
- **Calls** go only to lower-numbered functions, as the verifier proves, at most 8 deep.
- **Library functions** stay bytecode: the shared library is small and pure, and an app may open
  before it is attached.

## Where it runs and what it trusts

The translator runs on the watch, as the native module `aot`, never on the phone: native code a
phone sent could not be checked, while bytecode the verifier passed can only translate into code
that keeps the rules above. Like the verifier it runs once per image with the open app closed, after
the install's check, and writes `/apps/<id>/native`. The record of verified images gains the native
file's CRC, so the host runs native code only when the file is the one the watch wrote for that
image. A phone that rewrote it could make the watch run code of its choosing, but an unsigned
firmware update already lets a connected phone do that; signed updates would close both.

The module is 11.2 KB. It reads no image: the firmware, which has parsed the image already, gives
it only what translating needs, from `native_file::translator_input`: the image's CRC field and
length, each function's stack effect, and each looping unit's kind, number, locals and code. The
first call measures the native file and the second writes it into room the firmware lends.

## What gets compiled

Native code is about four times the size of its bytecode and must sit in RAM, where the app's whole
budget is about 25 KB, so only code that repeats is compiled: every handler, expression and function
with a loop, and the functions they call. Straight-line code runs once per event and gains little.
The memory need counts the native code an image's looping units take, so an app that would no
longer fit stays interpreted.

## The code it writes

- **One native function per code unit**, taking a context in r0 and giving back an exit in r0.
  r4 holds the context, r5 the unit's first local in the interpreter's pool and r6 the bottom of
  the unit's part of the interpreter's operand stack. The stack's depth is known at every
  instruction, as the verifier proves, so each stack slot is a fixed place above r6 and no depth
  is tracked.
- **Handing over.** Native code runs on the interpreter's own stack and locals, so the interpreter
  can take over at the start of any instruction. Every way out that native code does not handle
  itself, such as an overflow, an instruction it does not translate or a block that would pass the
  step limit, goes through a small stub that stores the instruction's offset, the stack's depth
  and the steps to give back, and exits. The interpreter then runs that instruction itself, so a
  failure is always the interpreter's own, raised at the same instruction.
- **Steps** are counted per block, a run of instructions entered only at its start. A block takes
  its instruction count as it starts, or hands over before it when fewer steps are left, so the
  interpreter fails at exactly the instruction it would have failed at; a handover inside a block
  gives back the steps the block took but did not run.
- **Simple instructions inline**: constants, locals, the stack moves, 64-bit add and subtract with
  the overflow flag checked, minimum, maximum, negation, absolute value, logic, comparisons, jumps
  and loops.
- **The rest through the interpreter**: multiply, divide and remainder, state and array reads and
  writes, text, time, system values, events, effects and the array work call the execute helper,
  through one small routine per unit, with the instruction's offset and the stack's depth. The
  helper runs that one instruction with the interpreter's own code, so its behaviour cannot drift,
  and a failure there ends the run with the interpreter's own failure.
- **Calls** are handed over: the interpreter makes the call, counting its step, which the handover
  gives back. When the callee returns, the interpreter runs the caller natively again from the
  instruction after the call, which the unit's start dispatches to.
- **Position-independent**: branches are PC-relative, and the helpers are reached through a table
  in the context, so the code needs no relocation and runs from the RAM's code alias wherever the
  heap puts it.

## The native file

`battletime_aot::native_file` writes what the module keeps at `/apps/<id>/native`: a header naming
the image it was made from by its header's CRC field and its length, a table giving each unit's
kind, number and code, and the code, each unit's on a 4-byte boundary. Only units with a loop are
in it. Game of Life's two looping handlers, 346 bytes of bytecode, take 3,276 bytes of native code,
about nine and a half times as much, since every instruction loads its operands from their stack
slots and stores its result; keeping the top of the stack in registers would cut that.

## The runtime's side

`battletime_runtime::native` holds the `Context` the code reads and the `NativeCode` trait. An
interpreter given native code with `set_native_code` asks it to run each app unit it enters,
handing it the unit's part of the operand stack, its locals, the steps left and the offset to
start at, and a closure that runs one instruction. Whatever runs the code builds the context from
those: the watch calls the code through the RAM's code alias, and the tests run it in the
simulator, on the interpreter's own memory.

## How it is checked

The translator is a `no_std` crate, `battletime-aot`, built for the module and for the computer. Its
`simulator` feature runs the Thumb-2 instructions the translator writes, and stops at any other.
The tests run an interpreter with native code beside one without: every arithmetic and comparison
instruction on edge values, loops counted step for step, each kind of handover, thousands of
random programs with branches and nested loops, and every handler, expression and function of
every app, face and shell screen in the repository, three rounds over, comparing results, texts,
state, effects and failures. The emulator check then runs a compiled app on the emulated chip, and
the watch measures the speed.

## Order of work

1. The crate: the Thumb-2 encoder, the simulator, and the simple instructions, checked against the
   interpreter. Done.
2. Every instruction, through helpers, with calls and step counting. Done.
3. The runtime's side: the context, the helper table, and running a native unit in place of its
   bytecode. The interpreter's side is done; the watch's `NativeCode`, which calls the code and
   gives it the helper table, comes with the module.
4. The `aot` module, the install step, the native file and its record, loading at open and the
   memory need.
5. Measurements on the watch, starting with Game of Life's generation.
