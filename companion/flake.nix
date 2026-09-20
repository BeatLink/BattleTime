{
  description = "NeoTime Companion App: phone, web and desktop builds";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        # The desktop app runs the Electron from nixpkgs rather than the
        # prebuilt binary npm downloads, which does not run on NixOS.
        electron = pkgs.electron;

        # Chromium is the browser with Web Bluetooth, for testing the web
        # build against a real watch without the desktop shell.
        browsers = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.chromium ];
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            nodejs_22
            electron
            bluez
            dbus
            git
          ] ++ browsers;

          # electron-forge and the npm electron package both look here before
          # trying to download a binary of their own.
          ELECTRON_OVERRIDE_DIST_PATH = "${electron}/libexec/electron";
          ELECTRON_SKIP_BINARY_DOWNLOAD = "1";

          shellHook = ''
            # The VSCode terminal exports this, which makes Electron start as
            # a bare Node process and exit immediately.
            unset ELECTRON_RUN_AS_NODE

            echo "NeoTime Companion"
            echo "  npm install          install dependencies"
            echo "  npm start            phone and web, via Expo"
            echo "  npm run desktop      build the web export and open the desktop app"
            echo "  npm test             protocol tests"
            echo
            echo "node    $(node --version)"
            echo "electron ${electron.version}"
          '';
        };
      });
}
