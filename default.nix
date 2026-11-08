let
  lock = builtins.fromJSON (builtins.readFile ./flake.lock);
  pkgs = import (fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/${lock.nodes.nixpkgs.locked.rev}.tar.gz";
    sha256 = lock.nodes.nixpkgs.locked.narHash;
  }) { };
in
pkgs.callPackage ./nix/package.nix { }
