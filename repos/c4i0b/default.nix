# NUR entrypoint: exposes this repository's packages to the
# Nix User Registry as nur.repos.c4i0b.<name>.
# Registration lives in nix-community/NUR repos.json; this file is the
# interface NUR evaluates (pkgs is passed by NUR, pinned by us in CI).
{
  pkgs ? import <nixpkgs> { },
}:
let
  root = ../..;
  version = (builtins.fromTOML (builtins.readFile (root + "/Cargo.toml"))).package.version;
in
{
  topmatic = pkgs.callPackage (root + "/nix/package.nix") {
    inherit version;
    src = builtins.path {
      path = root;
      name = "topmatic-source";
      filter =
        path: type:
        !builtins.elem (builtins.baseNameOf path) [
          ".git"
          "result"
          "target"
        ];
    };
  };
}
