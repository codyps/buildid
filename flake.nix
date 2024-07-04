{
  inputs = {
    nixpkgs.url = "nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, flake-utils, nixpkgs }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = ((import nixpkgs) {
          inherit system;
        });
        lib = pkgs.lib;
        stdenv = if pkgs.stdenv.isDarwin then apple_sdk.stdenv else pkgs.stdenv;

        # Not sure we need the newer sdk, but let's avoid weird issues with
        # having different apple_sdk versions on x86_64 vs aarch64.
        apple_sdk = pkgs.darwin.apple_sdk_11_0;
        mkShell = pkgs.mkShell.override {
          inherit stdenv;
        };
        mkShellNoCC = pkgs.mkShellNoCC.override {
          inherit stdenv;
        };

       run-lint = pkgs.writeScriptBin "run-lint" ''
          echo "Checking Rust formatting..."
          cargo fmt --check

          echo "Auditing Rust dependencies..."
          cargo-deny check

          echo "Auditing editorconfig conformance..."
          eclint -exclude "Cargo.lock"

          echo "Checking spelling..."
          codespell \
            --skip target,.git \
            --ignore-words-list crate
        '';
      in
      {
        formatter = pkgs.nixpkgs-fmt;

        devShells.fmt = mkShellNoCC {
          nativeBuildInputs = with pkgs; [cargo rustfmt];
        };

        devShells.clippy = mkShell {
          nativeBuildInputs = with pkgs; [
            rustc
            cargo
            sccache
            clippy
          ];
        };
        

        devShells.default = mkShell {
          nativeBuildInputs = [ run-lint ] ++ (with pkgs; [
            rustc
            cargo
            rustfmt
            sccache
            clippy
            rust-analyzer
            cargo-outdated
            cargo-udeps

            eclint
            codespell
          ] ++ lib.optional stdenv.isDarwin [
            iconv
          ]);

          RUSTC_WRAPPER = "sccache";
          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        };
      }
    );
}
