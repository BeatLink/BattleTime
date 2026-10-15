{
  description = "BattleTime: a Rust firmware for the PineTime, and its companion app";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # InfiniTime's own simulator, with the InfiniTime and LVGL versions it is known to build with.
    infinisim = {
      url = "git+https://github.com/InfiniTimeOrg/InfiniSim?submodules=1";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, infinisim }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        # The companion's Android SDK is unfree and asks for its licence to be accepted.
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };
        lib = pkgs.lib;

        # The toolchain file adds the watch's ARM target, which nixpkgs rustc lacks.
        rust = pkgs.rust-bin.fromRustupToolchainFile ./battletime/rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };

        # Only the Cargo files and the crates, so editing docs or the flake does not rebuild. The
        # workspace is battletime/, and the docs the harness reads sit beside it.
        source = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./battletime/Cargo.toml
            ./battletime/Cargo.lock
            ./battletime/app-host
            ./battletime/boot
            ./battletime/compiler
            ./battletime/drivers
            ./battletime/harness
            ./battletime/runtime
            ./battletime/services
            ./battletime/simulator
            ./battletime/ui
            # The compiler draws the shared library icons from these fonts and pictures.
            ./battletime/third_party
            # The compiler inlines the shared components and functions, and its tests compile every app and face.
            ./battletime/sdk/battletime/src/components.tsx
            ./battletime/sdk/battletime/src/lib.ts
            (lib.fileset.fileFilter (file: file.hasExt "tsx") ./battletime/apps)
            (lib.fileset.fileFilter (file: file.hasExt "tsx") ./battletime/faces)
            # The harness's quality checks look for every app and face in these listings.
            ./docs/apps.md
            ./docs/faces.md
            # The harness plays every example script in this folder.
            ./battletime/tests/scripts
          ];
        };

        # The simulator window is SDL2, which the sdl2 crate finds through pkg-config.
        simulatorLibraries = [ pkgs.SDL2 ];
        simulatorBuildTools = [ pkgs.pkg-config ];

        # Runs one cargo command over the workspace, with dependencies vendored from Cargo.lock.
        cargoCheck = name: command: rustPlatform.buildRustPackage {
          pname = "battletime-${name}";
          version = "0.1.0";
          src = source;
          cargoLock.lockFile = ./battletime/Cargo.lock;
          cargoRoot = "battletime";
          nativeBuildInputs = simulatorBuildTools;
          buildInputs = simulatorLibraries;
          buildPhase = ''
            runHook preBuild
            cd battletime
            ${command}
            runHook postBuild
          '';
          doCheck = false;
          installPhase = "touch $out";
        };

        simulator = rustPlatform.buildRustPackage {
          pname = "battletime-simulator";
          version = "0.1.0";
          src = source;
          cargoLock.lockFile = ./battletime/Cargo.lock;
          cargoRoot = "battletime";
          buildAndTestSubdir = "battletime";
          cargoBuildFlags = [ "--package" "battletime-simulator" ];
          cargoTestFlags = [ "--package" "battletime-simulator" ];
          nativeBuildInputs = simulatorBuildTools;
          buildInputs = simulatorLibraries;
          meta.mainProgram = "battletime-simulator";
        };

        # InfiniTime's fonts and images are generated at build time with lv_font_conv and lv_img_conv.
        infinisimPackage = pkgs.stdenv.mkDerivation {
          pname = "infinisim";
          version = "unstable";
          src = infinisim;
          nativeBuildInputs = [
            pkgs.cmake
            pkgs.git
            pkgs.pkg-config
            pkgs.lv_font_conv
            (pkgs.python3.withPackages (python: [ python.pillow ]))
          ];
          buildInputs = [ pkgs.SDL2 pkgs.libpng pkgs.zlib ];
          postPatch = ''
            patchShebangs InfiniTime/src/resources InfiniTime/src/displayapp/fonts
            substituteInPlace InfiniTime/src/displayapp/fonts/generate.py \
              --replace-fail "'/usr/bin/env', 'patch'" "'patch'"
            substituteInPlace InfiniTime/src/resources/generate-fonts.py \
              --replace-fail "'/usr/bin/env', 'patch'" "'patch'"
          '';
          # Point at the patched copy in the build tree, not the read-only source.
          preConfigure = ''
            cmakeFlagsArray+=("-DInfiniTime_DIR=$PWD/InfiniTime")
          '';
          cmakeFlags = [ "-DWITH_PNG=ON" ];
          installPhase = ''
            runHook preInstall
            install -Dm755 infinisim $out/bin/infinisim
            runHook postInstall
          '';
          meta.mainProgram = "infinisim";
        };

        # ====================================================================
        # The companion app
        # ====================================================================

        # The Android versions the Tauri app's Gradle project builds against.
        companionAndroid = {
          platform = "36";
          buildTools = "36.0.0";
          ndk = "27.1.12297006";
          cmake = "3.22.1";
        };

        companionAndroidSdk = (pkgs.androidenv.composeAndroidPackages {
          platformVersions = [ companionAndroid.platform ];
          # Build tools 35 stay alongside 36 for Gradle plugins that ask for them.
          buildToolsVersions = [ companionAndroid.buildTools "35.0.0" ];
          includeNDK = true;
          ndkVersions = [ companionAndroid.ndk ];
          cmakeVersions = [ companionAndroid.cmake ];
          includeEmulator = false;
          includeSystemImages = false;
        }).androidsdk;
        companionAndroidSdkRoot = "${companionAndroidSdk}/libexec/android-sdk";
        companionAndroidNdkRoot = "${companionAndroidSdkRoot}/ndk/${companionAndroid.ndk}";

        # The Tauri Android app's Rust, which needs the phone targets nixpkgs' rustc lacks.
        companionAndroidRust = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "clippy" "rustfmt" ];
          targets = [ "aarch64-linux-android" "armv7-linux-androideabi" "i686-linux-android" "x86_64-linux-android" ];
        };

        # The command line tool, bundled with esbuild so it needs nothing from node_modules at run time.
        companionCli = pkgs.buildNpmPackage {
          pname = "battletime-companion-cli";
          version = (lib.importJSON ./companion/package.json).version;
          src = lib.cleanSource ./companion;

          nodejs = pkgs.nodejs_22;
          npmDepsHash = "sha256-y3T88lDElyTCOBvyg7qp4WMtGMBzpXWZh8M6qnmklaM=";
          makeCacheWritable = true;

          buildPhase = ''
            runHook preBuild
            npx esbuild cli/battletime.ts --bundle --platform=node --target=node20 \
              --format=cjs --alias:@=./src --external:x11 --external:usocket \
              --outfile=build/battletime.cjs
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            install -Dm644 build/battletime.cjs $out/share/battletime-companion/battletime.cjs
            makeWrapper ${pkgs.nodejs_22}/bin/node $out/bin/battletime \
              --add-flags $out/share/battletime-companion/battletime.cjs
            runHook postInstall
          '';

          nativeBuildInputs = [ pkgs.makeWrapper ];

          meta = {
            description = "Command line tool for watches running BattleTime";
            homepage = "https://github.com/BeatLink/BattleTime";
            license = lib.licenses.gpl3Plus;
            mainProgram = "battletime";
            platforms = lib.platforms.linux;
          };
        };

        # The companion's window, built by Vite, which the Tauri desktop app embeds.
        companionWeb = pkgs.buildNpmPackage {
          pname = "battletime-companion-web";
          version = (lib.importJSON ./companion/package.json).version;
          src = lib.cleanSource ./companion;

          nodejs = pkgs.nodejs_22;
          npmDepsHash = companionCli.npmDepsHash;
          makeCacheWritable = true;

          buildPhase = ''
            runHook preBuild
            npx vite build
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            cp -r dist $out
            runHook postInstall
          '';
        };

        # The desktop app and the command line tool together, as one installable package.
        companion = pkgs.symlinkJoin {
          name = "battletime-companion-${(lib.importJSON ./companion/package.json).version}";
          paths = [ companionTauri companionCli ];
          meta = companionTauri.meta // {
            description = "The BattleTime Companion desktop app and command line tool";
          };
        };

        # The Tauri desktop app, with the Vite build built in and its Bluetooth, D-Bus and plugins in Rust.
        companionTauri = pkgs.rustPlatform.buildRustPackage {
          pname = "battletime-companion-tauri";
          version = (lib.importJSON ./companion/package.json).version;
          src = lib.cleanSource ./companion;
          cargoLock.lockFile = ./companion/Cargo.lock;

          cargoBuildFlags = [ "--package" "battletime-companion" "--features" "custom-protocol" ];
          # The Rust tests run in CI's companion workflow.
          doCheck = false;

          nativeBuildInputs = [ pkgs.pkg-config pkgs.wrapGAppsHook3 pkgs.copyDesktopItems ];
          buildInputs = with pkgs; [ webkitgtk_4_1 gtk3 libsoup_3 glib openssl dbus ];

          # Tauri embeds the interface from dist/ while it compiles.
          preBuild = ''
            cp -r ${companionWeb} dist
            chmod -R u+w dist
          '';

          postInstall = ''
            install -Dm644 assets/images/icon.png \
              $out/share/icons/hicolor/512x512/apps/battletime-companion.png
          '';

          desktopItems = [
            (pkgs.makeDesktopItem {
              name = "battletime-companion";
              desktopName = "BattleTime Companion";
              comment = "Companion for watches running BattleTime";
              exec = "battletime-companion";
              icon = "battletime-companion";
              categories = [ "Utility" ];
            })
          ];

          meta = {
            description = "The BattleTime Companion desktop app, built with Tauri";
            homepage = "https://github.com/BeatLink/BattleTime";
            license = lib.licenses.gpl3Plus;
            mainProgram = "battletime-companion";
            platforms = lib.platforms.linux;
          };
        };

        # The Tauri desktop app: Rust, the Tauri CLI, and the WebKitGTK window it draws in.
        companionTauriPackages = with pkgs; [
          cargo
          rustc
          clippy
          rustfmt
        ] ++ companionTauriTools;
        companionTauriTools = with pkgs; [
          cargo-tauri
          pkg-config
          webkitgtk_4_1
          gtk3
          libsoup_3
          glib
          openssl
        ];

        # Chromium loads the window under `npm run dev`, where the mock watch stands in for Bluetooth.
        companionPackages = with pkgs; [
          nodejs_22
          bluez
          dbus
          git
        ] ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux ([ pkgs.chromium ] ++ companionTauriPackages);

        # The App Builder, a VS Code extension; the editor module names its folder from the passthru values.
        appbuilderVersion = (lib.importJSON ./tools/appbuilder/package.json).version;
        appbuilder = pkgs.buildNpmPackage {
          pname = "battletime-appbuilder";
          version = appbuilderVersion;
          src = lib.cleanSource ./tools/appbuilder;

          nodejs = pkgs.nodejs_22;
          npmDepsHash = "sha256-x6bAc/LkBmPnP/fdRDy/YyHwhvRNUbUa6vNohv+P8CU=";

          installPhase = ''
            runHook preInstall

            # TypeScript reads the SDK for the palette at run time, so it is the one dependency kept.
            npm prune --omit=dev
            target=$out/share/vscode/extensions/battletime.battletime-appbuilder
            mkdir -p $target
            cp -r package.json README.md out media node_modules $target/

            runHook postInstall
          '';

          passthru = {
            vscodeExtUniqueId = "battletime.battletime-appbuilder";
            vscodeExtPublisher = "battletime";
            vscodeExtName = "battletime-appbuilder";
            vscodeExtVersion = appbuilderVersion;
          };
        };
      in
      {
        packages = {
          default = simulator;
          simulator = simulator;
          infinisim = infinisimPackage;
          companion = companion;
          companion-cli = companionCli;
          companion-tauri = companionTauri;
          companion-android-sdk = companionAndroidSdk;
          appbuilder = appbuilder;
        };

        apps = {
          default = flake-utils.lib.mkApp { drv = simulator; };
          simulator = flake-utils.lib.mkApp { drv = simulator; };
          infinisim = flake-utils.lib.mkApp { drv = infinisimPackage; };
          companion = flake-utils.lib.mkApp { drv = companion; };
          companion-cli = flake-utils.lib.mkApp { drv = companionCli; };
          companion-tauri = flake-utils.lib.mkApp { drv = companionTauri; };
        };

        checks = {
          tests = cargoCheck "tests" "cargo test --workspace --offline";
          watch-build = cargoCheck "watch-build"
            "cargo build --release --offline --target thumbv7em-none-eabihf";
          clippy = cargoCheck "clippy" "cargo clippy --workspace --offline -- --deny warnings";
          simulator-screenshot = pkgs.runCommand "battletime-simulator-screenshot" { } ''
            ${simulator}/bin/battletime-simulator --screenshot $out.png --page 2
            test -s $out.png
            mv $out.png $out
          '';
          # The built-in stopwatch app, after a click on its start button.
          simulator-app-screenshot = pkgs.runCommand "battletime-simulator-app-screenshot" { } ''
            ${simulator}/bin/battletime-simulator --demo --click 120,138 --screenshot $out.png
            test -s $out.png
            mv $out.png $out
          '';
        };

        devShells.default = pkgs.mkShell {
          packages = [
            rust
            pkgs.cargo-binutils
            pkgs.cargo-llvm-cov
            pkgs.nodejs_24
            pkgs.pkg-config
            pkgs.probe-rs-tools
            (pkgs.python3.withPackages (python: [ python.mkdocs-material ]))
            pkgs.SDL2
          ];

          shellHook = ''
            # CI measures docs/memory-map.md now; a clone still pointing git at the removed .githooks gets its default hooks back.
            if [ ! -d .githooks ] && [ "$(git config --get core.hooksPath)" = .githooks ]; then
              git config --unset core.hooksPath
            fi
            echo "BattleTime (run these from battletime/)"
            echo "  cargo test --workspace                                 run every test, simulator included"
            echo "  cargo build --release --target thumbv7em-none-eabihf   build the drivers for the watch"
            echo "  cargo run -p battletime-simulator                         open the simulator"
            echo "  cargo run -p battletime-simulator -- --demo               run the built-in stopwatch app"
            echo "  cargo run -p battletime-simulator -- --script FILE        play a test script, such as tests/scripts/*.battletime"
            echo "  ../tools/build_images.sh                               build the bootloader, firmware and reloaders into dist/"
            echo "  nix run .#infinisim                                    open InfiniTime's simulator to compare"
            echo "  ../tools/check_coverage.sh                             run every test with coverage and check the floors"
            echo "  nix flake check                                        run all of the above as checks"
            echo "  python3 -I ../tools/measure_memory.py                  measure the firmware and file system into the memory map pages"
            echo "  ../tools/build_site.sh                                 build the GitHub Pages site into target/site/out"
          '';
        };

        devShells.companion = pkgs.mkShell {
          packages = companionPackages;

          shellHook = ''
            echo "BattleTime Companion (run these from companion/)"
            echo "  npm ci               install dependencies"
            echo "  npm run tauri        build the window with Vite and open the desktop app"
            echo "  npm run dev          the window alone in a browser, with a mock watch"
            echo "  npm test             the TypeScript tests"
            echo "  npm run battletime -- <command>   the command line tool"
            echo "  cargo test --workspace   the Rust core and the Tauri app"
            echo "  nix develop .#companion-android   the shell with the Android SDK"
          '';
        };

        # The Android SDK is several gigabytes, so it has a shell of its own.
        devShells.companion-android = pkgs.mkShell {
          packages = [ companionAndroidRust ]
            ++ lib.subtractLists [ pkgs.cargo pkgs.rustc pkgs.clippy pkgs.rustfmt ] companionPackages
            ++ [ companionAndroidSdk pkgs.jdk17 ];

          ANDROID_HOME = companionAndroidSdkRoot;
          ANDROID_SDK_ROOT = companionAndroidSdkRoot;
          ANDROID_NDK_ROOT = companionAndroidNdkRoot;
          # The Tauri CLI finds the NDK here.
          NDK_HOME = companionAndroidNdkRoot;
          # QuickJS's bindings are generated for Android, against the NDK's headers.
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          BINDGEN_EXTRA_CLANG_ARGS = "--sysroot=${companionAndroidNdkRoot}/toolchains/llvm/prebuilt/linux-x86_64/sysroot";
          JAVA_HOME = pkgs.jdk17.home;
          # Gradle's own aapt2 is built for other Linux systems and does not run on NixOS.
          GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${companionAndroidSdkRoot}/build-tools/${companionAndroid.buildTools}/aapt2";

          shellHook = ''
            echo "BattleTime Companion, with the Android SDK (run these from companion/)"
            echo "  npm run tauri-android           build the app's debug APK for arm64"
            echo "  npm run tauri-android:release   build the release APK for every ABI, signed when keystore.properties exists"
          '';
        };
      });
}
