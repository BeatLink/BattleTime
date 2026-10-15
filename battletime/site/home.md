---
hide:
  - navigation
  - toc
---

# A Rust firmware for the PineTime

BattleTime combines the best of the two PineTime firmwares: the polish, battery life and speed of
InfiniTime, with the easy, loadable apps and desktop simulator of wasp-os.

Apps are written in **Preact and TypeScript**, the way you would build a web page. No Rust or
embedded knowledge is needed: the compiler turns an app into bytecode the watch runs.

[Using the watch](guide/index.md){ .md-button .md-button--primary }
[Writing apps](writing-apps.md){ .md-button }
[Source on GitHub](https://github.com/BeatLink/BattleTime){ .md-button }

Early work. The firmware, bootloader and updates run on a real PineTime.

<div class="screens" markdown>
![The digital watch face](screens/face-digital.png){ .screen }
![The launcher as a grid of icons](screens/app-launcher-grid.png){ .screen }
</div>

## What it aims for

<div class="grid cards" markdown>

-   :material-language-typescript: **Apps in Preact, no Rust needed**

    Write an app as Preact components in TypeScript, with Signals for state, and preview it in a
    browser. The compiler turns it into compact bytecode, so web skills are all it takes.

    [How apps are written](writing-apps.md)

-   :material-download-network: **Loadable apps**

    Apps and watch faces install over Bluetooth from the phone, with no reflash.

-   :material-puzzle: **Apps from shared parts**

    Widgets, fonts and images live in the firmware and a shared library, so each app is small and
    light on RAM.

-   :material-language-rust: **Pure Rust**

    The whole firmware is Rust, down to the Bluetooth stack: TrouBLE on BattleTime's own radio
    controller, built on Embassy.

-   :material-shield-check: **Safe updates**

    A bad firmware rolls back by itself, and holding the button starts a recovery firmware.

-   :material-battery-high: **Battery life**

    The watch sleeps whenever it can, radio and crystal included.

-   :material-cellphone-link: **Companion apps**

    It works with Gadgetbridge and the other apps InfiniTime supports, and has a companion of its
    own for computers and Android.

    [The companion app](companion/index.md)

</div>

## Watch faces

Every face from InfiniTime, wasp-os and PinetimeLite, and the faces of ZephyrLabs' Watchfaces,
ported to BattleTime's app language.
[All faces](faces.md)

<div class="gallery" markdown>
<figure markdown>![](screens/face-analog.png){ .screen loading=lazy }<figcaption>Analog</figcaption></figure>
<figure markdown>![](screens/face-pinetime-style.png){ .screen loading=lazy }<figcaption>PineTimeStyle</figcaption></figure>
<figure markdown>![](screens/face-infineat.png){ .screen loading=lazy }<figcaption>Infineat</figcaption></figure>
<figure markdown>![](screens/face-casio.png){ .screen loading=lazy }<figcaption>Casio</figcaption></figure>
<figure markdown>![](screens/face-terminal.png){ .screen loading=lazy }<figcaption>Terminal</figcaption></figure>
<figure markdown>![](screens/face-word-clock.png){ .screen loading=lazy }<figcaption>Word clock</figcaption></figure>
<figure markdown>![](screens/face-lite-rings.png){ .screen loading=lazy }<figcaption>Rings</figcaption></figure>
<figure markdown>![](screens/face-pride-flag.png){ .screen loading=lazy }<figcaption>Pride flag</figcaption></figure>
<figure markdown>![](screens/face-chrono.png){ .screen loading=lazy }<figcaption>Chrono</figcaption></figure>
<figure markdown>![](screens/face-pinealog.png){ .screen loading=lazy }<figcaption>Pinealog</figcaption></figure>
<figure markdown>![](screens/face-waves.png){ .screen loading=lazy }<figcaption>Waves</figcaption></figure>
<figure markdown>![](screens/face-omega-time.png){ .screen loading=lazy }<figcaption>OmegaTime</figcaption></figure>
</div>

## Apps

The apps of both firmwares, merged where they overlap, grouped in a launcher the companion app
arranges. [All apps](apps.md)

<div class="gallery" markdown>
<figure markdown>![](screens/app-steps.png){ .screen loading=lazy }<figcaption>Steps</figcaption></figure>
<figure markdown>![](screens/app-heart.png){ .screen loading=lazy }<figcaption>Heart rate</figcaption></figure>
<figure markdown>![](screens/app-stopwatch-running.png){ .screen loading=lazy }<figcaption>Stopwatch</figcaption></figure>
<figure markdown>![](screens/app-weather.png){ .screen loading=lazy }<figcaption>Weather</figcaption></figure>
<figure markdown>![](screens/app-music.png){ .screen loading=lazy }<figcaption>Music</figcaption></figure>
<figure markdown>![](screens/app-navigation.png){ .screen loading=lazy }<figcaption>Navigation</figcaption></figure>
<figure markdown>![](screens/app-calculator.png){ .screen loading=lazy }<figcaption>Calculator</figcaption></figure>
<figure markdown>![](screens/app-twos.png){ .screen loading=lazy }<figcaption>2048</figcaption></figure>
<figure markdown>![](screens/app-launcher-folders.png){ .screen loading=lazy }<figcaption>Launcher folders</figcaption></figure>
<figure markdown>![](screens/shell-notifications.png){ .screen loading=lazy }<figcaption>Notifications</figcaption></figure>
<figure markdown>![](screens/shell-notifications-preview.png){ .screen loading=lazy }<figcaption>New notification</figcaption></figure>
<figure markdown>![](screens/app-call-ringing.png){ .screen loading=lazy }<figcaption>Incoming call</figcaption></figure>
<figure markdown>![](screens/shell-quick-settings.png){ .screen loading=lazy }<figcaption>Quick settings</figcaption></figure>
<figure markdown>![](screens/app-settings.png){ .screen loading=lazy }<figcaption>Settings</figcaption></figure>
</div>

## Credits

BattleTime stands on the work of these projects and the people behind them.

<div class="grid cards" markdown>

-   :material-watch: **The firmwares it grew from**

    [InfiniTime](https://github.com/InfiniTimeOrg/InfiniTime) for the interface, faces, apps and
    Bluetooth services; [wasp-os](https://github.com/wasp-os/wasp-os) for loadable apps and the
    simulator; [PinetimeLite](https://github.com/joaquimorg/PinetimeLite) and
    [ZephyrLabs' Watchfaces](https://github.com/ZephyrLabs/Watchfaces) for their faces.

-   :material-language-rust: **What the watch is built on**

    [Embassy](https://embassy.dev) and its Bluetooth host
    [TrouBLE](https://github.com/embassy-rs/trouble);
    [rlvgl](https://github.com/softoboros/rlvgl) and [LVGL](https://lvgl.io) for drawing and
    widgets; [embedded-graphics](https://github.com/embedded-graphics/embedded-graphics) for the
    simulator; [Preact](https://preactjs.com) for the app language; and
    [Font Awesome](https://fontawesome.com) and
    [Material Icons](https://github.com/google/material-design-icons) for the icons.

-   :material-cellphone-link: **The companion and tools**

    [Tauri](https://tauri.app), [btleplug](https://github.com/deviceplug/btleplug) and
    [QuickJS](https://bellard.org/quickjs/) for the companion; [Gadgetbridge](https://gadgetbridge.org)
    for the protocol the watch speaks; [InfiniEmu](https://github.com/pipe01/InfiniEmu) for the
    emulator.

-   :material-heart: **And the rest**

    [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/) for this website, and
    [PINE64](https://www.pine64.org/pinetime/) for the PineTime itself.

</div>

Screenshots are drawn by the test suite, so they show the apps as they are now. BattleTime is free
software under the GNU General Public License, version 3 or later.
