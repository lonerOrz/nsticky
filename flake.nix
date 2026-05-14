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

      flake.homeModules = rec {
        default = nsticky;
        nsticky = ./nix/module.nix;
      };

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
            nsticky = pkgs.callPackage ./nix/package.nix { };
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
