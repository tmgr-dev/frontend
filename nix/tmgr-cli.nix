{ lib, rustPlatform }:

rustPlatform.buildRustPackage {
  pname = "tmgr-cli";
  version = (fromTOML (builtins.readFile ../Cargo.toml)).workspace.package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../LICENSE
      ../crates
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = [
    "-p"
    "tmgr-cli"
  ];
  doCheck = false;

  meta = {
    description = "Command line interface for TMGR";
    homepage = "https://tmgr.dev";
    license = lib.licenses.agpl3Only;
    mainProgram = "tmgr";
    platforms = lib.platforms.unix;
  };
}
