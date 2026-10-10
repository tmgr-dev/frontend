{
  description = "TMGR desktop app";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        config.allowUnfree = true;
      };
      tmgr = pkgs.callPackage ./nix/package.nix { };
    in
    {
      packages.${system} = {
        inherit tmgr;
        default = tmgr;
      };

      apps.${system}.default = {
        type = "app";
        program = "${tmgr}/bin/tmgr";
      };

      overlays.default = final: _prev: {
        tmgr = final.callPackage ./nix/package.nix { };
      };
    };
}
