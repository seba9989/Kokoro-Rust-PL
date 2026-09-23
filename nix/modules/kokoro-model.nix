{ pkgs, lib, config, ... }:

# Moduł devenv: opcja `plkokoro.voices` -> pakiet modelu Kokoro (nix/pkgs/kokoro-model.nix) + KOKORO_MODEL_DIR.
let
  cfg = config.plkokoro;
in
{
  options.plkokoro = {
    voices = lib.mkOption {
      type = lib.types.nonEmptyListOf lib.types.str;
      default = [ "pm_mateusz" ];
      example = [ "pm_mateusz" "df_anna" "af_heart" ];
      description = ''
        Głosy Kokoro (id z nix/catalog.json, lista: `plkokoro --list-voices`) spakowane do KOKORO_MODEL_DIR razem
        z modelami i tokenizerami, których wymagają (każdy nowy model to ~325 MB). Katalog jest w /nix/store (tylko
        do odczytu): głos spoza listy daje błąd z podpowiedzią.
      '';
    };
    kokoroModel = lib.mkOption {
      type = lib.types.package;
      readOnly = true;
      default = pkgs.callPackage ../pkgs/kokoro-model.nix { inherit (cfg) voices; };
      defaultText = lib.literalMD "pakiet z głosami `plkokoro.voices`";
      description = "Zbudowany pakiet modelu Kokoro (passthru: voices, languages, phonemisLanguages, revision).";
    };
  };

  config = {
    packages = [ cfg.kokoroModel ];
    # Niski priorytet: nadpisanie np. env.KOKORO_MODEL_DIR = "/home/…/models" — katalog z prawem zapisu, do którego
    # biblioteka sama pobierze dowolny głos.
    env.KOKORO_MODEL_DIR = lib.mkDefault "${cfg.kokoroModel}";
  };
}
