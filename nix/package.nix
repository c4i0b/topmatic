{
  lib,
  rustPlatform,
  makeWrapper,
  systemd,
  topgrade,
  src,
  version,
}:

rustPlatform.buildRustPackage {
  pname = "topmatic";
  inherit version src;

  cargoDepsName = "topmatic";
  cargoHash = "sha256-ts1dKsfmHenfyzVObblUNMQiuzHvj+0qvrstydjKUj8=";

  nativeBuildInputs = [ makeWrapper ];

  nativeCheckInputs = [ systemd ];

  postInstall = ''
    wrapProgram $out/bin/topmatic \
      --prefix PATH : "${lib.makeBinPath [ topgrade ]}"
  '';

  meta = {
    description = "User-level scheduled updates powered by topgrade";
    homepage = "https://github.com/c4i0b/topmatic";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "topmatic";
  };
}
