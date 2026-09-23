{ pkgs, lib, config, ... }:

# Moduł devenv: opcja `plkokoro.phonemisLanguages` -> pakiet Phonemis (nix/pkgs/phonemis.nix) + PHONEMIS_RUNNER.
let
  cfg = config.plkokoro;
  supportedLanguages = lib.attrNames (import ../pkgs/phonemis-files.nix);
in
{
  options.plkokoro = {
    phonemisLanguages = lib.mkOption {
      type = lib.types.nonEmptyListOf (lib.types.enum supportedLanguages);
      # Frontendy tekstowe języków wybranych głosów (z katalogu, patrz nix/modules/kokoro-model.nix) + pl.
      default = lib.intersectLists (cfg.kokoroModel.passthru.phonemisLanguages ++ [ "pl" ]) supportedLanguages;
      defaultText = lib.literalMD "języki frontendu tekstowego głosów z `plkokoro.voices` (z katalogu) + `pl`";
      example = [ "pl" "de" "en-us" ];
      description = "Języki Phonemis, których wagi są instalowane obok phonemis_runner (kody jak plkokoro::PHONEMIS_LANGS).";
    };
    phonemis = lib.mkOption {
      type = lib.types.package;
      readOnly = true;
      default = pkgs.callPackage ../pkgs/phonemis.nix { languages = cfg.phonemisLanguages; };
      defaultText = lib.literalMD "phonemis_runner + wagi `plkokoro.phonemisLanguages`";
      description = "Zbudowany pakiet Phonemis (passthru: languages, supportedLanguages, rev).";
    };
  };

  config = {
    packages = [ cfg.phonemis ];
    # Wagi biblioteka znajduje sama: bin/phonemis_runner to symlink do build/, a <pakiet>/data/<język>/… leży obok.
    env.PHONEMIS_RUNNER = lib.mkDefault "${cfg.phonemis}/bin/phonemis_runner";
  };
}
