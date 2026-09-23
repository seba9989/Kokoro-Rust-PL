{ lib
, rustPlatform
, makeWrapper
, onnxruntime
, callPackage
  # Głosy i języki Phonemis zapakowane razem z CLI (jak opcje plkokoro.voices / plkokoro.phonemisLanguages).
, voices ? [ "pm_mateusz" ]
, phonemisLanguages ? null # null = frontendy języków wybranych głosów + pl
, phonemis ? null
, kokoroModel ? null
}:

# CLI `plkokoro` gotowe do uruchomienia: binarka z plkokoro-cli owinięta tak, że domyślnie (--set-default, więc
# zmienne z otoczenia wygrywają) korzysta z ONNX Runtime, Phonemis i modelu Kokoro z /nix/store.
#   nix run github:seba9989/Kokoro-Rust-PL -- "Cześć." -o out.wav
let
  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions [
      ../../Cargo.toml
      ../../Cargo.lock
      ../../rustfmt.toml
      ../../plkokoro/Cargo.toml
      ../../plkokoro/src
      ../../plkokoro/examples
      ../../plkokoro-cli/Cargo.toml
      ../../plkokoro-cli/src
    ];
  };
  cargoToml = lib.importTOML ../../Cargo.toml;

  model = if kokoroModel != null then kokoroModel else callPackage ./kokoro-model.nix { inherit voices; };
  langs =
    if phonemisLanguages != null then phonemisLanguages
    else lib.intersectLists (model.passthru.phonemisLanguages ++ [ "pl" ]) (lib.attrNames (import ./phonemis-files.nix));
  runner = if phonemis != null then phonemis else callPackage ./phonemis.nix { languages = langs; };
in
rustPlatform.buildRustPackage {
  pname = "plkokoro";
  inherit (cargoToml.workspace.package) version;
  inherit src;
  cargoLock.lockFile = ../../Cargo.lock;

  cargoBuildFlags = [ "-p" "plkokoro-cli" ];
  # Testy potrzebują fałszywego serwera HF, ORT i sieci lokalnej — uruchamiaj je w devenv (`pk-test`).
  doCheck = false;

  nativeBuildInputs = [ makeWrapper ];

  # PHONEMIS_RUNNER wskazuje symlink bin/ (ścieżka z „/build/” w wrapperze nie przeszłaby kontroli stdenv).
  postInstall = ''
    wrapProgram "$out/bin/plkokoro" \
      --set-default ORT_LIBRARY_PATH "${lib.getLib onnxruntime}/lib/libonnxruntime.so" \
      --set-default PHONEMIS_RUNNER "${runner}/bin/phonemis_runner" \
      --set-default KOKORO_MODEL_DIR "${model}"
  '';

  passthru = { phonemis = runner; kokoroModel = model; inherit onnxruntime; };

  meta = {
    description = "TTS: Kokoro (ONNX) + Phonemis — CLI z modelem (${toString model.passthru.voices}) i Phonemis (${toString langs})";
    homepage = "https://github.com/seba9989/Kokoro-Rust-PL";
    license = lib.licenses.mit;
    mainProgram = "plkokoro";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}
