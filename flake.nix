{
  description = "TMGR desktop app and command line tool";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      desktopSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      desktopFor =
        s:
        (import nixpkgs {
          system = s;
          config.allowUnfree = true;
        }).callPackage
          ./nix/package.nix
          { };

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
      packages = nixpkgs.lib.recursiveUpdate cliPackages (
        nixpkgs.lib.genAttrs desktopSystems (s: rec {
          tmgr = desktopFor s;
          default = tmgr;
        })
      );

      apps = nixpkgs.lib.genAttrs desktopSystems (s: {
        default = {
          type = "app";
          program = "${self.packages.${s}.tmgr}/bin/tmgr";
        };
      });

      overlays.default = final: _prev: {
        tmgr = final.callPackage ./nix/package.nix { };
        tmgr-cli = final.callPackage ./nix/tmgr-cli.nix { };
      };
    };
}
