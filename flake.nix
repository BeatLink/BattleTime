{
  description = "NeoTime: a Rust firmware for the PineTime";

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
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        lib = pkgs.lib;

        # The toolchain file adds the watch's ARM target, which nixpkgs rustc lacks.
        rust = pkgs.rust-bin.fromRustupToolchainFile ./neotime/rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };

        # Only the Cargo files and the crates, so editing docs or the flake does not rebuild.
        source = lib.fileset.toSource {
          root = ./neotime;
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
            # The compiler inlines the shared components, and its tests compile every app and face.
            ./neotime/sdk/neotime/src/components.tsx
            (lib.fileset.fileFilter (file: file.hasExt "tsx") ./neotime/apps)
            (lib.fileset.fileFilter (file: file.hasExt "tsx") ./neotime/faces)
            # The harness's quality checks look for every app and face in these listings.
            ./neotime/docs/apps.md
            ./neotime/docs/faces.md
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
          nativeBuildInputs = simulatorBuildTools;
          buildInputs = simulatorLibraries;
          buildPhase = ''
            runHook preBuild
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
      in
      {
        packages = {
          default = simulator;
          simulator = simulator;
          infinisim = infinisimPackage;
        };

        apps = {
          default = flake-utils.lib.mkApp { drv = simulator; };
          simulator = flake-utils.lib.mkApp { drv = simulator; };
          infinisim = flake-utils.lib.mkApp { drv = infinisimPackage; };
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
            echo "NeoTime"
            echo "  cargo test --workspace                                 run every test, simulator included"
            echo "  cargo build --release --target thumbv7em-none-eabihf   build the drivers for the watch"
            echo "  cargo run -p neotime-simulator                         open the simulator"
            echo "  cargo run -p neotime-simulator -- --demo               run the built-in stopwatch app"
            echo "  cargo run -p neotime-simulator -- --script FILE        play a test script, such as tests/scripts/*.neotime"
            echo "  tools/build_images.sh                                  build the bootloader, firmware and reloaders into dist/"
            echo "  nix run .#infinisim                                    open InfiniTime's simulator to compare"
            echo "  tools/check_coverage.sh                                run every test with coverage and check the floors"
            echo "  nix flake check                                        run all of the above as checks"
            echo "  python3 -I tools/measure_memory.py                     measure the firmware and file system into the memory map pages"
            echo "  tools/build_site.sh                                    build the GitHub Pages site into target/site/out"
          '';
        };
      });
}
