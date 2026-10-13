---
hide:
  - navigation
  - toc
---

# A Rust firmware for the PineTime

NeoTime combines the best of the two PineTime firmwares: the polish, battery life and speed of
InfiniTime, with the easy, loadable apps and desktop simulator of wasp-os.

Apps are written in **Preact and TypeScript**, the way you would build a web page. No Rust or
embedded knowledge is needed: the compiler turns an app into bytecode the watch runs.

[Read the docs](introduction.md){ .md-button .md-button--primary }
[Source on GitHub](https://github.com/BeatLink/NeoTime){ .md-button }

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

    [How apps are written](app-runtime.md)

-   :material-download-network: **Loadable apps**

    Apps and watch faces install over Bluetooth from the phone, with no reflash.

-   :material-puzzle: **Apps from shared parts**

    Widgets, fonts and images live in the firmware and a shared library, so each app is small and
    light on RAM.

-   :material-language-rust: **Pure Rust**

    The whole firmware is Rust, down to the Bluetooth stack: TrouBLE on NeoTime's own radio
    controller, built on Embassy.

-   :material-shield-check: **Safe updates**

    A bad firmware rolls back by itself, and holding the button starts a recovery firmware.

-   :material-battery-high: **Battery life**

    The watch sleeps whenever it can, radio and crystal included.

-   :material-cellphone-link: **Companion apps**

    It works with Gadgetbridge and the other apps InfiniTime supports.

</div>

## Watch faces

Every face from InfiniTime, wasp-os and PinetimeLite, ported to NeoTime's app language.
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
<figure markdown>![](screens/shell-quick-settings.png){ .screen loading=lazy }<figcaption>Quick settings</figcaption></figure>
<figure markdown>![](screens/app-settings.png){ .screen loading=lazy }<figcaption>Settings</figcaption></figure>
</div>

Screenshots are drawn by the test suite, so they show the apps as they are now. NeoTime is free
software under the GNU General Public License, version 3 or later.
