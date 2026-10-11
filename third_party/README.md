# Third-party artwork

The compiler draws the shared library's icons (`neotime-compiler library`) from these files. Each is
copied unchanged from a local checkout of InfiniTime (github.com/InfiniTimeOrg/InfiniTime, commit
6c119eb).

| File | From InfiniTime | Licence |
|---|---|---|
| `font-awesome/FontAwesome5-Solid+Brands+Regular.woff` | `src/displayapp/fonts/` | Font Awesome Free 5.9.0: font under SIL OFL 1.1 (`LICENSE-OFL-1.1.txt`), icons under CC BY 4.0 (`LICENSE-CC-BY-4.0.txt`) |
| `material-icons/MaterialIcons-Regular.ttf` | `src/displayapp/fonts/material-design-icons/` | Google Material Icons, Apache 2.0 (`LICENSE`) |
| `infinitime/pine_logo.png` | `src/resources/images/` | InfiniTime, GPL-3.0-or-later (`LICENSE`) |

Despite its name, the Font Awesome file is a plain TrueType font.

## Fonts

`tools/generate_fonts.py` makes the app host's fonts (`app-host/fonts/*.ntfont`) from these, copied
unchanged from InfiniTime's `src/displayapp/fonts/` at the same commit.

| File | Licence |
|---|---|
| `fonts/JetBrainsMono-Bold.ttf`, `-ExtraBold.ttf`, `-Light.ttf` | SIL OFL 1.1 (`fonts/JetBrainsMono-OFL.txt`, from the JetBrains Mono source package) |
| `fonts/OpenSans-Light.ttf` | Apache 2.0 (`fonts/OpenSans-LICENSE-2.0.txt`) |
