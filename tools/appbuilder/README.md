# NeoTime App Builder

A VS Code extension for building NeoTime watch apps, in two panes. The palette of widgets, events,
effects and values sits in the NeoTime sidebar, to the left of the editor; clicking an entry puts it at
the cursor in the app's source and adds the import it needs. The simulated watch opens in its own
pane to the right of the code, and saving the app compiles it and restarts it there. Each pane can be
moved, resized or closed on its own.

The app stays ordinary TypeScript in `neotime/apps/<id>/app.tsx`, so it can be edited by hand at any
time, and the tests, tours and build tools treat it like any other app.

## How it runs an app

The extension uses the same path the watch does: `neotime-compiler` turns the app into an image and
`neotime-simulator --stream` runs it with the shared library (see
[docs/simulator.md](../../docs/simulator.md)). The simulator sends each changed
frame to the watch pane and takes its touches, so the pane shows exactly what the watch would, including
compile errors, which also appear in the editor.

The first run builds the compiler and simulator with cargo, which takes a while; later runs take a
moment. Effects the app asks for, such as `effect: Vibrate(30)`, appear under the screen and in the
"NeoTime App Builder" output channel.

The compiler and simulator are built from the checkout the app is in, so an app opened in a checkout
from before the App Builder landed gets a simulator without `--stream`. The watch pane then says the
checkout is too old; update it to NeoTime master.

| On the watch pane | Does |
|---|---|
| Click, hold or drag on the screen | A tap, a long press, a drag or a swipe |
| Side button | Goes back a screen |
| Theme | Switches between the default and a light theme |
| Restart | Compiles the app again and restarts it |

The palette is read from the SDK's source (`neotime/sdk/neotime/src`) of the app's checkout: widgets
from `widgets.tsx` and `components.tsx`, events from `EventMap`, effects from the functions that call
`emit`, and values from the exported signals. It always matches what apps can use.

## Commands

- **NeoTime: Open App Builder** shows the palette and runs the app open in the editor in the watch
  pane; the device button on a `.tsx` file's title bar does the same. The palette also fills on its
  own whenever an app's source is in focus, and inserts into the last one that was.
- **NeoTime: New App** asks for an id, writes a starting app to `neotime/apps/<id>/app.tsx`, and
  opens the builder on it. A finished app also needs its tour and docs page, as
  [CLAUDE.md](../../CLAUDE.md) says.

The setting `neotime.appBuilder.toolchainCommand` (default `nix develop --command`) goes in front of
cargo; set it empty if cargo is already on the path.

## Developing it

```sh
npm install
npm run build      # compile to out/
npm test           # or npm run coverage, which checks the floors in vitest.config.mts
```

To try it, open this folder in VS Code and run the extension with F5, or start VS Code with
`code --extensionDevelopmentPath=$PWD path/to/NeoTime`. `src/extension.ts`, `src/palette_view.ts` and `src/simulator_panel.ts` need
a running editor, so they are tried by hand; everything they call is tested.
