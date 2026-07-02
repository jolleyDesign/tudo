{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      flake-parts,
      rust-overlay,
      crane,
      ...
    }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      perSystem =
        { pkgs, system, ... }:
        let
          lib = pkgs.lib;

          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          overlays = [ (import rust-overlay) ];
          craneLib = (crane.mkLib pkgs).overrideToolchain rust;

          commonArgs = {
            src = lib.cleanSourceWith {
              name = "source";
              src = ./.;
              filter = craneLib.filterCargoSources;
            };
            strictDeps = true;
          };
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system overlays;
          };

          packages = rec {
            ratic = craneLib.buildPackage (
              commonArgs
              // {
                inherit cargoArtifacts;
              }
            );

            default = ratic;
          };
        };
    };
}
