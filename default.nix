(import (
  let
    lock = builtins.fromJSON (builtins.readFile ./flake.lock);
  in
  fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/${lock.nodes.nixpkgs.locked.rev}.tar.gz";
    sha256 = lock.nodes.nixpkgs.locked.narHash;
  }
) { }).callPackage
  ./nix/package.nix
  {
    src = builtins.path {
      path = ./.;
      name = "topmatic-source";
      filter =
        path: type:
        !builtins.elem (builtins.baseNameOf path) [
          ".git"
          "result"
          "target"
        ];
    };
    version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
  }
