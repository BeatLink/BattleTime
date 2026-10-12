{
  description = "NeoTime Companion: phone, web and desktop builds";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        # The Android SDK is unfree and asks for its licence to be accepted.
        pkgs = import nixpkgs {
          inherit system;
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };
        lib = pkgs.lib;

        # The desktop app runs the Electron from nixpkgs rather than the
        # prebuilt binary npm downloads, which does not run on NixOS.
        electron = pkgs.electron;

        # Chromium is the browser with Web Bluetooth, for testing the web
        # build against a real watch without the desktop shell.
        browsers = lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.chromium ];

        # ====================================================================
        # Android SDK
        # ====================================================================

        # The versions React Native 0.86 and Expo SDK 57 build against.
        android = {
          platform = "36";
          buildTools = "36.0.0";
          ndk = "27.1.12297006";
          cmake = "3.22.1";
        };

        android_sdk_packages = pkgs.androidenv.composeAndroidPackages {
          platformVersions = [ android.platform ];
          # Some Expo modules still ask for build tools 35 as well.
          buildToolsVersions = [ android.buildTools "35.0.0" ];
          includeNDK = true;
          ndkVersions = [ android.ndk ];
          cmakeVersions = [ android.cmake ];
          includeEmulator = false;
          includeSystemImages = false;
        };
        android_sdk = android_sdk_packages.androidsdk;
        android_sdk_root = "${android_sdk}/libexec/android-sdk";

        # ====================================================================
        # Installable desktop app and command line tool
        # ====================================================================

        neotime_companion = pkgs.buildNpmPackage {
          pname = "neotime-companion";
          version = (lib.importJSON ./package.json).version;
          src = lib.cleanSource self;

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

            makeWrapper ${electron}/bin/electron $out/bin/neotime-companion \
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
            homepage = "https://github.com/BeatLink/neotime-companion";
            license = lib.licenses.lgpl3Plus;
            mainProgram = "neotime-companion";
            platforms = lib.platforms.linux;
          };
        };

        # ====================================================================
        # Development shells
        # ====================================================================

        common_packages = with pkgs; [
          nodejs_22
          electron
          bluez
          dbus
          git
        ] ++ browsers;

        # Nothing here should download an Electron of its own; the one on
        # PATH is the wrapper that works. Pointing anything at the unwrapped
        # binary under libexec makes it die with SIGILL.
        common_environment = {
          ELECTRON_SKIP_BINARY_DOWNLOAD = "1";
        };

        common_shell_hook = ''
          # The VSCode terminal exports this, which makes Electron start as
          # a bare Node process and exit immediately.
          unset ELECTRON_RUN_AS_NODE
        '';
      in
      {
        packages = {
          default = neotime_companion;
          neotime-companion = neotime_companion;
          android-sdk = android_sdk;
        };

        apps = {
          default = flake-utils.lib.mkApp { drv = neotime_companion; };
          neotime = flake-utils.lib.mkApp { drv = neotime_companion; name = "neotime"; };
        };

        devShells.default = pkgs.mkShell (common_environment // {
          packages = common_packages;

          shellHook = common_shell_hook + ''
            echo "NeoTime Companion"
            echo "  npm install          install dependencies"
            echo "  npm start            phone and web, via Expo"
            echo "  npm run desktop      build the web export and open the desktop app"
            echo "  npm test             protocol tests"
            echo "  nix develop .#android  the shell with the Android SDK"
            echo
            echo "node    $(node --version)"
            echo "electron ${electron.version}"
          '';
        });

        # The Android SDK is several gigabytes, so it has a shell of its own.
        devShells.android = pkgs.mkShell (common_environment // {
          packages = common_packages ++ [ android_sdk pkgs.jdk17 ];

          ANDROID_HOME = android_sdk_root;
          ANDROID_SDK_ROOT = android_sdk_root;
          ANDROID_NDK_ROOT = "${android_sdk_root}/ndk/${android.ndk}";
          JAVA_HOME = pkgs.jdk17.home;
          # Gradle's own aapt2 is built for other Linux systems and does not run on NixOS.
          GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${android_sdk_root}/build-tools/${android.buildTools}/aapt2";

          shellHook = common_shell_hook + ''
            echo "NeoTime Companion, with the Android SDK"
            echo "  npm run android      build, install and start the app on a phone over adb"
            echo "  npx expo prebuild -p android && (cd android && ./gradlew assembleRelease)"
            echo "                       build an APK without a phone"
            echo
            echo "Android SDK ${android.platform}, build tools ${android.buildTools}, NDK ${android.ndk}"
          '';
        });
      });
}
