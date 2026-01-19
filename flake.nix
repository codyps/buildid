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
        inherit (pkgs) lib stdenv mkShell mkShellNoCC;

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

            cargo-deny
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
