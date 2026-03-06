{
  description = "A sticky windows manager CLI tool for Niri";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    adios-flake.url = "github:Mic92/adios-flake";
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };

  outputs =
    inputs@{
      adios-flake,
      self,
      ...
    }:
    adios-flake.lib.mkFlake {
      inherit inputs self;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      modules = [ ];

      perSystem =
        {
          self',
          pkgs,
          ...
        }:
        let
          lib = pkgs.lib;
          treefmtEval = inputs.treefmt-nix.lib.evalModule pkgs {
            projectRootFile = "flake.nix";
            programs = {
              rustfmt.enable = true;
              nixfmt.enable = true;
            };
          };
        in
        {
          formatter = treefmtEval.config.build.wrapper;

          packages = {
            default = self'.packages.nsticky;

            nsticky = pkgs.rustPlatform.buildRustPackage {
              pname = "nsticky";
              version = "0.1.0";
              src = ./.;
              cargoLock.lockFile = ./Cargo.lock;

              meta = {
                description = "A sticky windows manager CLI tool for Niri";
                homepage = "https://github.com/lonerOrz/nsticky";
                mainProgram = "nsticky";
                license = lib.licenses.bsd3;
                maintainers = with lib.maintainers; [ lonerOrz ];
                platforms = [
                  "x86_64-linux"
                  "aarch64-linux"
                  "x86_64-darwin"
                  "aarch64-darwin"
                ];
              };
            };
          };

          devShells.default = pkgs.mkShell {
            inputsFrom = [ self'.packages.default ];
            packages = with pkgs; [
              cargo
              rustc
              rust-analyzer
              rustfmt
              clippy
              cargo-watch
              cargo-criterion
            ];
          };
        };
    };
}
