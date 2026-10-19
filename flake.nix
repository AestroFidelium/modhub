{
  description = "modhub: a registry for Stormlight WASM mods. Publish, version, validate and serve mods.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # TDD + fuzzing + coverage + mutation testing. nextest is the only blessed runner.
        testTools = with pkgs; [
          cargo-nextest
          cargo-bolero
          cargo-llvm-cov
          cargo-mutants
        ];

        # Everything the gate runs (tools/gate).
        lintTools = with pkgs; [
          cargo-deny
          cargo-machete
          typos
          nixfmt
          statix
          deadnix
        ];

        nativeTools = with pkgs; [
          mold
          clang
        ];

        # Local database (tools/pg) and the sqlx CLI: migrations and the offline query
        # cache in .sqlx/. The CLI's minor version must match the sqlx crate's.
        dbTools = with pkgs; [
          postgresql_18
          sqlx-cli
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [ rust ] ++ testTools ++ lintTools ++ nativeTools ++ dbTools;

          # Versioned hooks in .githooks/: pre-commit runs `tools/gate quick`,
          # commit-msg enforces Conventional Commits. Fix hooks there, never in .git/hooks.
          # DATABASE_URL: the unix-socket cluster of tools/pg. sqlx macros check queries
          # against it at compile time; tests create their own databases on it.
          shellHook = ''
            git config core.hooksPath .githooks 2>/dev/null || true
            export DATABASE_URL="postgres://modhub@localhost/modhub?host=$PWD/.pg"
          '';
        };

        formatter = pkgs.nixfmt;
      }
    );
}
