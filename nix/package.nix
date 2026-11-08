{
  lib,
  fetchFromGitHub,
  rustPlatform,
  makeWrapper,
  systemd,
  topgrade,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "topmatic";
  version = "1.2.0";

  src = fetchFromGitHub {
    owner = "c4i0b";
    repo = "topmatic";
    tag = "v${finalAttrs.version}";
    hash = "sha256-YNfNY3OK+bvteBiwh/+u+TLfHT/8OmZI1NbgLeIvBFk=";
  };

  cargoDepsName = finalAttrs.pname;
  cargoHash = "sha256-OjlMyxi35kBuenepkE6pBxJWAEOMrMAPMXMOKjRrqxw=";

  # v1.2.0's tests exec /bin/true and friends, which do not exist in
  # the nix build sandbox; fixed upstream after the tag. Drop after
  # the next release.
  checkFlags = [
    "--skip"
    "systemd::tests::run_status_reports_success_and_failure"
    "--skip"
    "systemd::tests::run_output_captures_stdout_and_kills_a_hanging_command"
  ];

  nativeBuildInputs = [ makeWrapper ];

  nativeCheckInputs = [ systemd ];

  postInstall = ''
    wrapProgram $out/bin/topmatic \
      --prefix PATH : "${lib.makeBinPath [ topgrade ]}"
  '';

  meta = {
    description = "User-level scheduled updates powered by topgrade";
    homepage = "https://github.com/c4i0b/topmatic";
    changelog = "https://github.com/c4i0b/topmatic/blob/v${finalAttrs.version}/CHANGELOG.md";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "topmatic";
  };
})
