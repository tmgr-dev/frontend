{
  description = "TMGR desktop app and command line tool";

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

      cliSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      cliPackages = nixpkgs.lib.genAttrs cliSystems (s: {
        tmgr-cli = nixpkgs.legacyPackages.${s}.callPackage ./nix/tmgr-cli.nix { };
      });
    in
    {
      packages = nixpkgs.lib.recursiveUpdate cliPackages {
        ${system} = {
          inherit tmgr;
          default = tmgr;
        };
      };

      apps.${system}.default = {
        type = "app";
        program = "${tmgr}/bin/tmgr";
      };

      overlays.default = final: _prev: {
        tmgr = final.callPackage ./nix/package.nix { };
        tmgr-cli = final.callPackage ./nix/tmgr-cli.nix { };
      };
    };
}
