{ pkgs, lib, config, ... }:

# Moduł devenv plkokoro dla INNYCH projektów (i dla devenv.nix tego repo): ONNX Runtime, Phonemis i model Kokoro
# jako pakiety Nix + zmienne ORT_LIBRARY_PATH, PHONEMIS_RUNNER, KOKORO_MODEL_DIR, które czyta biblioteka plkokoro.
#
# Użycie w innym projekcie — devenv.yaml:
#   inputs:
#     plkokoro:
#       url: github:seba9989/Kokoro-Rust-PL
#       flake: false
#   imports:
#     - plkokoro/nix            # = ten plik (katalog z devenv.nix)
# i w devenv.nix (opcjonalnie):
#   plkokoro.voices = [ "pm_mateusz" "df_anna" ];
#   plkokoro.phonemisLanguages = [ "pl" "de" "en-us" ];
let
  cfg = config.plkokoro;
in
{
  imports = [
    ./modules/phonemis.nix
    ./modules/kokoro-model.nix
  ];

  options.plkokoro.onnxruntime = lib.mkOption {
    type = lib.types.package;
    default = pkgs.onnxruntime;
    defaultText = lib.literalExpression "pkgs.onnxruntime";
    description = ''
      ONNX Runtime (>= 1.21), którego libonnxruntime.so wskazuje ORT_LIBRARY_PATH. Crate `ort` ładuje ją dynamicznie,
      więc nic nie jest linkowane w czasie budowania. Testowane z 1.21.0, 1.22.0, 1.23.2, 1.27.1 i 1.30.0.
    '';
  };

  config = {
    # Niski priorytet jak w modułach: nadpisywalne w devenv.nix / devenv.local.nix.
    env.ORT_LIBRARY_PATH = lib.mkDefault "${lib.getLib cfg.onnxruntime}/lib/libonnxruntime.so";

    # Czytelny błąd zamiast tajemniczego "unsupported API version" przy zbyt starym nixpkgs.
    assertions = [
      {
        assertion = lib.versionAtLeast cfg.onnxruntime.version "1.21";
        message = "plkokoro wymaga libonnxruntime >= 1.21, a wybrany nixpkgs ma ${cfg.onnxruntime.version}. Zaktualizuj input nixpkgs albo ustaw plkokoro.onnxruntime.";
      }
    ];
  };
}
