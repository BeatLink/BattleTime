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
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };

        # Only the Cargo files and the crates, so editing docs or the flake does not rebuild.
        source = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./app-host
            ./compiler
            ./drivers
            ./runtime
            ./simulator
            ./ui
            # The compiler inlines the shared components, and its tests compile every app and face.
            ./sdk/neotime/src/components.tsx
            (lib.fileset.fileFilter (file: file.hasExt "tsx") ./apps)
            (lib.fileset.fileFilter (file: file.hasExt "tsx") ./faces)
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
          cargoLock.lockFile = ./Cargo.lock;
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
          cargoLock.lockFile = ./Cargo.lock;
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
            pkgs.pkg-config
            pkgs.SDL2
          ];

          shellHook = ''
            echo "NeoTime"
            echo "  cargo test --workspace                                 run every test, simulator included"
            echo "  cargo build --release --target thumbv7em-none-eabihf   build the drivers for the watch"
            echo "  cargo run -p neotime-simulator                         open the simulator"
            echo "  cargo run -p neotime-simulator -- --demo               run the built-in stopwatch app"
            echo "  nix run .#infinisim                                    open InfiniTime's simulator to compare"
            echo "  nix flake check                                        run all of the above as checks"
          '';
        };
      });
}
