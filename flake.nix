{
  description = "User-level scheduled updates powered by topgrade";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
        in
        {
          topmatic = pkgs.callPackage ./nix/package.nix {
            src = self;
            inherit version;
          };
          default = self.packages.${system}.topmatic;
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.topmatic}/bin/topmatic";
        };
      });
    };
}
