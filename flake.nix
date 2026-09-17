{
  description = "opake dev shell — Rust + TS monorepo with wasm bindings";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    # per-project rust toolchain via oxalica's overlay; keeps stable/
    # nightly/targets composable without rustup.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, ... }:
    let
      systems = [ "aarch64-darwin" "aarch64-linux" "x86_64-darwin" "x86_64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f (import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      }));
    in
    {
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            # ── rust toolchain ───────────────────────────────────────
            # stable + wasm target for crates/opake-wasm. rust-src and
            # rust-analyzer extensions give IDE-grade tooling.
            (rust-bin.stable.latest.default.override {
              targets = [ "wasm32-unknown-unknown" ];
              extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
            })

            # wasm-pack drives the opake-wasm build pipeline. It pulls
            # in wasm-bindgen-cli at a version matching the crate's
            # wasm-bindgen dep, so we don't pin wasm-bindgen-cli here
            # (nixpkgs' pin frequently drifts from per-project lockfiles).
            wasm-pack

            # cargo meta-tools — live here (alongside the toolchain)
            # rather than globally in home/tools.nix, so they always
            # have a matching cargo on PATH.
            cargo-edit       # cargo add / cargo upgrade
            cargo-watch      # auto-rebuild on file change
            cargo-nextest    # faster test runner

            # ── elixir / phoenix (apps/indexer) ──────────────────────
            # mix.exs pins `elixir ~> 1.15`; 1.18 on OTP 27 is current
            # stable and satisfies that. The beam.packages form locks
            # the elixir↔OTP pair explicitly (vs pkgs.elixir_1_18 which
            # picks whatever default OTP nixpkgs ships).
            beam.packages.erlang_27.elixir_1_18

            # postgres client + libpq — the indexer talks to the
            # dockerized postgres:17 from docker-compose.development.yaml
            # via postgrex (pure-elixir driver), but psql / pg_dump are
            # useful for mix ecto commands and ad-hoc db poking.
            postgresql_17

            # ── ts monorepo ──────────────────────────────────────────
            # package.json#volta pins 24.15.0; nodejs_24 tracks the
            # latest 24.x, which is close enough for bun's needs.
            nodejs_24
            bun

            # ── task runner ──────────────────────────────────────────
            just

            # ── atproto tooling ──────────────────────────────────────
            # goat — Go CLI for atproto (repo inspection, XRPC calls,
            # firehose consumption, identity resolution). Packaged in
            # nixpkgs as `atproto-goat` to disambiguate from the ASCII
            # diagram tool `goat`. Binary name on PATH is still `goat`.
            atproto-goat
          ] ++ nixpkgs.lib.optionals pkgs.stdenv.isLinux [
            # Phoenix LiveReload uses inotify on Linux; macOS uses
            # fsevents natively and doesn't need this.
            inotify-tools
          ];

          # openspec 1.6.0 ships telemetry on by default and routes it
          # through edge.openspec.dev, which its own source says is "to
          # avoid ad blockers". I hope they step on a sharp pebble, but
          # at least they provide this option too.
          DO_NOT_TRACK = "1";

          shellHook = ''
            echo "opake dev shell — $(rustc --version | cut -d' ' -f1,2), elixir $(elixir --version 2>&1 | tail -n1 | cut -d' ' -f2), node $(node --version), bun $(bun --version)"
          '';
        };
      });
    };
}
