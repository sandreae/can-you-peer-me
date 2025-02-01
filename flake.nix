{
  description = "Creates a promise based map of locks that can be used as a semaphore";
  outputs = {
    nixpkgs,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem
    (system: let
      pkgs = import nixpkgs {
        inherit system;
      };

      nativeBuildInputs = with pkgs; [
        cargo-tauri.hook
        rustc
        # npmHooks.npmConfigHook
        pkg-config
      ];

      buildInputs = with pkgs; [
        # Make sure we can find our libraries
        openssl
        glib-networking
        webkitgtk_4_1
        atk
        gst_all_1.gst-plugins-bad
        gst_all_1.gst-plugins-base
        gst_all_1.gst-plugins-good
        gst_all_1.gst-plugins-ugly
      ];
    in {
      devShells.default = pkgs.mkShell {
        inherit nativeBuildInputs;
        inherit buildInputs;
      };
      packages.default = pkgs.rustPlatform.buildRustPackage rec {
        inherit nativeBuildInputs;
        inherit buildInputs;
        pname = "canyoupeerme";
        version = "0.0.1";
        src = ./.;
        cargoLock = {
          lockFile = ./src-tauri/Cargo.lock;
          allowBuiltinFetchGit = true;
        };
        cargoRoot = "src-tauri";
        buildAndTestSubdir = cargoRoot;
        doCheck = false; # no tests

        meta = {
          homepage = "https://github.com/sandreae/can-you-peer-me";
          description = "Minimalistic Pomodoro Timer Desktop App";
          mainProgram = "minipom";
        };
      };
    });
}
