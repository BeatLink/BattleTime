{
  description = "NeoTime: a Rust firmware for the PineTime, and its companion app";

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
        rust = pkgs.rust-bin.fromRustupToolchainFile ./neotime/rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };

        # Only the Cargo files and the crates, so editing docs or the flake does not rebuild. The
        # workspace is neotime/, and the docs the harness reads sit beside it.
        source = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./neotime/Cargo.toml
            ./neotime/Cargo.lock
            ./neotime/app-host
            ./neotime/boot
            ./neotime/compiler
            ./neotime/drivers
            ./neotime/harness
            ./neotime/runtime
            ./neotime/services
            ./neotime/simulator
            ./neotime/ui
            # The compiler draws the shared library icons from these fonts and pictures.
            ./neotime/third_party
            # The compiler inlines the shared components and functions, and its tests compile every app and face.
            ./neotime/sdk/neotime/src/components.tsx
            ./neotime/sdk/neotime/src/lib.ts
            (lib.fileset.fileFilter (file: file.hasExt "tsx") ./neotime/apps)
            (lib.fileset.fileFilter (file: file.hasExt "tsx") ./neotime/faces)
            # The harness's quality checks look for every app and face in these listings.
            ./docs/apps.md
            ./docs/faces.md
            # The harness plays every example script in this folder.
            ./neotime/tests/scripts
          ];
        };

        # The simulator window is SDL2, which the sdl2 crate finds through pkg-config.
        simulatorLibraries = [ pkgs.SDL2 ];
        simulatorBuildTools = [ pkgs.pkg-config ];

        # Runs one cargo command over the workspace, with dependencies vendored from Cargo.lock.
        cargoCheck = name: command: rustPlatform.buildRustPackage {
          pname = "neotime-${name}";
          version = "0.1.0";
          src = source;
          cargoLock.lockFile = ./neotime/Cargo.lock;
          cargoRoot = "neotime";
          nativeBuildInputs = simulatorBuildTools;
          buildInputs = simulatorLibraries;
          buildPhase = ''
            runHook preBuild
            cd neotime
            ${command}
            runHook postBuild
          '';
          doCheck = false;
          installPhase = "touch $out";
        };

        simulator = rustPlatform.buildRustPackage {
          pname = "neotime-simulator";
          version = "0.1.0";
          src = source;
          cargoLock.lockFile = ./neotime/Cargo.lock;
          cargoRoot = "neotime";
          buildAndTestSubdir = "neotime";
          cargoBuildFlags = [ "--package" "neotime-simulator" ];
          cargoTestFlags = [ "--package" "neotime-simulator" ];
          nativeBuildInputs = simulatorBuildTools;
          buildInputs = simulatorLibraries;
          meta.mainProgram = "neotime-simulator";
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

        # The versions React Native 0.86 and Expo SDK 57 build against.
        companionAndroid = {
          platform = "36";
          buildTools = "36.0.0";
          ndk = "27.1.12297006";
          cmake = "3.22.1";
        };

        companionAndroidSdk = (pkgs.androidenv.composeAndroidPackages {
          platformVersions = [ companionAndroid.platform ];
          # Some Expo modules still ask for build tools 35 as well.
          buildToolsVersions = [ companionAndroid.buildTools "35.0.0" ];
          includeNDK = true;
          ndkVersions = [ companionAndroid.ndk ];
          cmakeVersions = [ companionAndroid.cmake ];
          includeEmulator = false;
          includeSystemImages = false;
        }).androidsdk;
        companionAndroidSdkRoot = "${companionAndroidSdk}/libexec/android-sdk";

        # The desktop app runs the Electron from nixpkgs rather than the prebuilt binary npm
        # downloads, which does not run on NixOS.
        companionElectron = pkgs.electron;

        # The installable desktop app and command line tool.
        companion = pkgs.buildNpmPackage {
          pname = "neotime-companion";
          version = (lib.importJSON ./companion/package.json).version;
          src = lib.cleanSource ./companion;

          nodejs = pkgs.nodejs_22;
          npmDepsHash = "sha256-eG9/Mu6QRXpRRDTL0LGrW37SXUKINJzwsqeSMnm+J3M=";
          makeCacheWritable = true;

          nativeBuildInputs = [ pkgs.makeWrapper pkgs.copyDesktopItems ];

          env = {
            ELECTRON_SKIP_BINARY_DOWNLOAD = "1";
            # Expo would otherwise try to reach its servers during the build.
            EXPO_OFFLINE = "1";
            EXPO_NO_TELEMETRY = "1";
            CI = "1";
          };

          buildPhase = ''
            runHook preBuild

            npx expo export --platform web --output-dir dist

            # Both bundles carry their Bluetooth and D-Bus code, so nothing
            # from node_modules is needed at run time.
            npx esbuild electron/main.js --bundle --platform=node --target=node20 \
              --format=cjs --external:electron --external:x11 --external:usocket \
              --outfile=build/electron/main.js
            cp electron/preload.js build/electron/preload.js
            # Electron takes the app's name and version from a package.json beside its main script.
            cat > build/electron/package.json <<EOF
            {"name": "neotime-companion", "productName": "NeoTime Companion", "version": "$version", "main": "main.js"}
            EOF
            npx esbuild cli/neotime.ts --bundle --platform=node --target=node20 \
              --format=cjs --alias:@=./src --external:x11 --external:usocket \
              --outfile=build/neotime.cjs

            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall

            share=$out/share/neotime-companion
            mkdir -p $share $out/bin
            cp -r build/electron $share/electron
            cp -r dist $share/dist
            cp build/neotime.cjs $share/neotime.cjs

            makeWrapper ${companionElectron}/bin/electron $out/bin/neotime-companion \
              --unset ELECTRON_RUN_AS_NODE \
              --add-flags $share/electron/main.js

            makeWrapper ${pkgs.nodejs_22}/bin/node $out/bin/neotime \
              --add-flags $share/neotime.cjs

            install -Dm644 assets/images/icon.png \
              $out/share/icons/hicolor/512x512/apps/neotime-companion.png

            runHook postInstall
          '';

          desktopItems = [
            (pkgs.makeDesktopItem {
              name = "neotime-companion";
              desktopName = "NeoTime Companion";
              comment = "Companion for watches running NeoTime";
              exec = "neotime-companion";
              icon = "neotime-companion";
              categories = [ "Utility" ];
              startupWMClass = "NeoTime Companion";
            })
          ];

          meta = {
            description = "Desktop app and command line tool for watches running NeoTime";
            homepage = "https://github.com/BeatLink/NeoTime";
            license = lib.licenses.gpl3Plus;
            mainProgram = "neotime-companion";
            platforms = lib.platforms.linux;
          };
        };

        # The Tauri desktop app: Rust, the Tauri CLI, and the WebKitGTK window it draws in.
        companionTauriPackages = with pkgs; [
          cargo
          rustc
          clippy
          rustfmt
          cargo-tauri
          pkg-config
          webkitgtk_4_1
          gtk3
          libsoup_3
          glib
          openssl
        ];

        # Chromium is the browser with Web Bluetooth, for testing the web build against a real watch.
        companionPackages = with pkgs; [
          nodejs_22
          companionElectron
          bluez
          dbus
          git
        ] ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux ([ pkgs.chromium ] ++ companionTauriPackages);

        # Nothing here should download an Electron of its own; the one on PATH is the wrapper that
        # works, and the unwrapped binary under libexec dies with SIGILL.
        companionEnvironment = {
          ELECTRON_SKIP_BINARY_DOWNLOAD = "1";
        };

        # The VSCode terminal exports ELECTRON_RUN_AS_NODE, which makes Electron start as a bare
        # Node process and exit at once.
        companionShellHook = ''
          unset ELECTRON_RUN_AS_NODE
        '';
        # The App Builder, a VS Code extension; the editor module names its folder from the passthru values.
        appbuilderVersion = (lib.importJSON ./tools/appbuilder/package.json).version;
        appbuilder = pkgs.buildNpmPackage {
          pname = "neotime-appbuilder";
          version = appbuilderVersion;
          src = lib.cleanSource ./tools/appbuilder;

          nodejs = pkgs.nodejs_22;
          npmDepsHash = "sha256-5yp+SYXose9RS5JC7aKowgCi0q7KJlYRboPfkIjeZcc=";

          installPhase = ''
            runHook preInstall

            # TypeScript reads the SDK for the palette at run time, so it is the one dependency kept.
            npm prune --omit=dev
            target=$out/share/vscode/extensions/neotime.neotime-appbuilder
            mkdir -p $target
            cp -r package.json README.md out media node_modules $target/

            runHook postInstall
          '';

          passthru = {
            vscodeExtUniqueId = "neotime.neotime-appbuilder";
            vscodeExtPublisher = "neotime";
            vscodeExtName = "neotime-appbuilder";
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
          companion-android-sdk = companionAndroidSdk;
          appbuilder = appbuilder;
        };

        apps = {
          default = flake-utils.lib.mkApp { drv = simulator; };
          simulator = flake-utils.lib.mkApp { drv = simulator; };
          infinisim = flake-utils.lib.mkApp { drv = infinisimPackage; };
          companion = flake-utils.lib.mkApp { drv = companion; };
          companion-cli = flake-utils.lib.mkApp { drv = companion; name = "neotime"; };
        };

        checks = {
          tests = cargoCheck "tests" "cargo test --workspace --offline";
          watch-build = cargoCheck "watch-build"
            "cargo build --release --offline --target thumbv7em-none-eabihf";
          clippy = cargoCheck "clippy" "cargo clippy --workspace --offline -- --deny warnings";
          simulator-screenshot = pkgs.runCommand "neotime-simulator-screenshot" { } ''
            ${simulator}/bin/neotime-simulator --screenshot $out.png --page 2
            test -s $out.png
            mv $out.png $out
          '';
          # The built-in stopwatch app, after a click on its start button.
          simulator-app-screenshot = pkgs.runCommand "neotime-simulator-app-screenshot" { } ''
            ${simulator}/bin/neotime-simulator --demo --click 120,138 --screenshot $out.png
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
            echo "NeoTime (run these from neotime/)"
            echo "  cargo test --workspace                                 run every test, simulator included"
            echo "  cargo build --release --target thumbv7em-none-eabihf   build the drivers for the watch"
            echo "  cargo run -p neotime-simulator                         open the simulator"
            echo "  cargo run -p neotime-simulator -- --demo               run the built-in stopwatch app"
            echo "  cargo run -p neotime-simulator -- --script FILE        play a test script, such as tests/scripts/*.neotime"
            echo "  ../tools/build_images.sh                               build the bootloader, firmware and reloaders into dist/"
            echo "  nix run .#infinisim                                    open InfiniTime's simulator to compare"
            echo "  ../tools/check_coverage.sh                             run every test with coverage and check the floors"
            echo "  nix flake check                                        run all of the above as checks"
            echo "  python3 -I ../tools/measure_memory.py                  measure the firmware and file system into the memory map pages"
            echo "  ../tools/build_site.sh                                 build the GitHub Pages site into target/site/out"
          '';
        };

        devShells.companion = pkgs.mkShell (companionEnvironment // {
          packages = companionPackages;

          shellHook = companionShellHook + ''
            echo "NeoTime Companion (run these from companion/)"
            echo "  npm install          install dependencies"
            echo "  npm start            phone and web, via Expo"
            echo "  npm run desktop      build the web export and open the Electron desktop app"
            echo "  npm run tauri        build the web export and open the Tauri desktop app"
            echo "  npm test             protocol tests"
            echo "  cargo test --workspace   the Rust core and the Tauri app"
            echo "  nix develop .#companion-android   the shell with the Android SDK"
          '';
        });

        # The Android SDK is several gigabytes, so it has a shell of its own.
        devShells.companion-android = pkgs.mkShell (companionEnvironment // {
          packages = companionPackages ++ [ companionAndroidSdk pkgs.jdk17 ];

          ANDROID_HOME = companionAndroidSdkRoot;
          ANDROID_SDK_ROOT = companionAndroidSdkRoot;
          ANDROID_NDK_ROOT = "${companionAndroidSdkRoot}/ndk/${companionAndroid.ndk}";
          JAVA_HOME = pkgs.jdk17.home;
          # Gradle's own aapt2 is built for other Linux systems and does not run on NixOS.
          GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${companionAndroidSdkRoot}/build-tools/${companionAndroid.buildTools}/aapt2";

          shellHook = companionShellHook + ''
            echo "NeoTime Companion, with the Android SDK (run these from companion/)"
            echo "  npm run android      build, install and start the app on a phone over adb"
            echo "  npx expo prebuild -p android && (cd android && ./gradlew assembleRelease)"
            echo "                       build an APK without a phone"
          '';
        });
      });
}
