{buildRustPackage, ...}:
buildRustPackage {
  src = ./.;
  extraArgs.doCheck = true;
}
