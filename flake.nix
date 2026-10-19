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

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rust;
          rustc = rust;
        };

        # The server binary. Built without a database: sqlx checks queries against the
        # committed .sqlx/ cache. Tests need PostgreSQL and run in tools/gate instead.
        modhub = rustPlatform.buildRustPackage {
          pname = "modhub";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
          src = pkgs.lib.cleanSource ./.;
          # `cargoHash`, not `cargoLock.lockFile`: the latter fetches from crates.io's API
          # with a curl User-Agent, which crates.io refuses. Update after Cargo.lock changes.
          cargoHash = "sha256-A00rm09Hm3oeF0kGooxow7b1u3nZ4kuvNc4IkYJ2/Ec=";
          cargoBuildFlags = [
            "--package"
            "modhub-server"
            "--bin"
            "modhub"
          ];
          SQLX_OFFLINE = "true";
          doCheck = false;
          # .cargo/config.toml links through clang + mold.
          nativeBuildInputs = with pkgs; [
            clang
            mold
          ];
          meta = {
            description = "A registry for Stormlight WASM mods";
            license = with pkgs.lib.licenses; [
              mit
              asl20
            ];
            mainProgram = "modhub";
          };
        };

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

        cargo-codspeed = pkgs.cargo-codspeed.overrideAttrs (_: {
          doCheck = false; # its own tests fail to build on current nightly (as in atelier)
        });

        # No optimisation without a number: divan under CodSpeed + a profiler.
        perfTools = with pkgs; [
          cargo-codspeed
          samply
          oha # HTTP load generator for the numbers in README.md
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
        packages.default = modhub;

        devShells.default = pkgs.mkShell {
          packages = [ rust ] ++ testTools ++ lintTools ++ perfTools ++ nativeTools ++ dbTools;

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
