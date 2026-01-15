{buildRustPackage, ...}:
buildRustPackage {
  src = ./.;
  # FIXME: re-enable tests once zellij sandbox issues are resolved
  extraArgs.doCheck = false;
}
