{
  lib,
  rustPlatform,
}:

let
  cargoToml = fromTOML (builtins.readFile ../Cargo.toml);
in
rustPlatform.buildRustPackage {
  pname = cargoToml.package.name;
  version = cargoToml.package.version;
  src = ../.;
  cargoLock.lockFile = ../Cargo.lock;

  meta = {
    description = "A sticky windows manager CLI tool for Niri";
    homepage = "https://github.com/lonerOrz/nsticky";
    mainProgram = "nsticky";
    license = lib.licenses.bsd3;
    maintainers = with lib.maintainers; [ lonerOrz ];
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
  };
}
