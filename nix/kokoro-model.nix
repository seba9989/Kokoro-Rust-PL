{ pkgs, lib, config, ... }:

# Moduł devenv: model Kokoro jako pakiet Nix — głosy z `plkokoro.voices`, ich modele ONNX i tokenizery, pobrane przez
# `fetchurl` z sumami SHA-256 z nix/catalog.json i ułożone jak plkokoro::Store (`<out>/v2.1.1/…`). Ustawia
# KOKORO_MODEL_DIR. Wczytywany przez `imports` w devenv.nix.
let
  cfg = config.plkokoro;

  # Musi być zgodne z plkokoro::REVISION (plkokoro/src/store.rs) — biblioteka szuka plików w <dir>/<rewizja>/.
  revision = "v2.1.1";

  # Kopia prawdziwego catalog.json tej rewizji (sprawdzona sha256 z plikiem na HF). Czytana przy ewaluacji, więc
  # ścieżki i sumy SHA-256 artefaktów pochodzą z katalogu, bez import-from-derivation.
  catalog = lib.importJSON ./catalog.json;

  allVoices = lib.concatMap (l: map (v: v // { lang = l.id; }) l.voices) catalog.languages;
  byId = lib.listToAttrs (map (v: lib.nameValuePair v.id v) allVoices);
  models = lib.listToAttrs (map (m: lib.nameValuePair m.id m) catalog.models);
  tokenizers = lib.listToAttrs (map (t: lib.nameValuePair t.id t) catalog.tokenizers);

  mkKokoroModel = voices:
    let
      unknown = lib.filter (id: !(byId ? ${id})) voices;
      chosen = map (id: byId.${id}) voices;
      modelIds = lib.unique (map (v: v.modelId) chosen);
      tokenizerIds = lib.unique (map (m: models.${m}.tokenizerId) modelIds);
      artifacts = lib.unique (
        map (v: v.artifact) chosen
        ++ map (m: models.${m}.artifact) modelIds
        ++ map (t: tokenizers.${t}.artifact) tokenizerIds
      );
      fetch = a: {
        inherit (a) path;
        src = pkgs.fetchurl {
          url = "https://huggingface.co/${catalog.distribution.repository}/resolve/${revision}/${a.path}";
          inherit (a) sha256;
        };
      };
      langs = lib.unique (map (v: v.lang) chosen);
    in
    assert lib.assertMsg (catalog.catalogVersion == lib.removePrefix "v" revision)
      "nix/kokoro-model.nix: nix/catalog.json ma wersję ${catalog.catalogVersion}, a revision = ${revision}";
    assert lib.assertMsg (unknown == [ ])
      "plkokoro.voices: nieznane głosy ${toString unknown}; lista: cargo run -p plkokoro-cli -- --list-voices";
    pkgs.runCommand "kokoro-model-${revision}"
      {
        passthru = {
          inherit revision voices;
          languages = langs;
          # Kody Phonemis potrzebne do fonemizacji tekstu w językach wybranych głosów (bez ja/zh: external-required).
          phonemisLanguages = lib.unique (lib.concatMap
            (l:
              let tf = l.textFrontend or { }; in
              lib.optional (lib.elem l.id langs && (tf.status or "") == "bundled") tf.language)
            catalog.languages);
        };
        meta = {
          description = "Kokoro-82M (ONNX) + głosy ${toString voices} + tokenizery, układ katalogu plkokoro::Store";
          homepage = "https://huggingface.co/${catalog.distribution.repository}";
          license = lib.licenses.asl20; # catalog.license = "Apache-2.0"
          platforms = lib.platforms.all;
        };
      }
      ''
        base="$out/${revision}"
        install -Dm644 ${./catalog.json} "$base/catalog.json"
        ${lib.concatMapStrings (f: ''
          install -Dm644 ${f.src} "$base/${f.path}"
        '') (map fetch artifacts)}
      '';
in
{
  options.plkokoro = {
    voices = lib.mkOption {
      type = lib.types.nonEmptyListOf lib.types.str;
      default = [ "pm_mateusz" ];
      example = [ "pm_mateusz" "df_anna" "af_heart" ];
      description = ''
        Głosy Kokoro (id z nix/catalog.json) spakowane do KOKORO_MODEL_DIR razem z modelami i tokenizerami, których
        wymagają (każdy nowy model to ~325 MB). Katalog jest w /nix/store (tylko do odczytu): głos spoza listy daje
        błąd z podpowiedzią.
      '';
    };
    kokoroModel = lib.mkOption {
      type = lib.types.package;
      readOnly = true;
      default = mkKokoroModel cfg.voices;
      defaultText = lib.literalMD "pakiet z głosami `plkokoro.voices`";
      description = "Zbudowany pakiet modelu Kokoro (passthru: voices, languages, phonemisLanguages, revision).";
    };
  };

  config = {
    packages = [ cfg.kokoroModel ];
    # Niski priorytet: nadpisanie w devenv.local.nix, np. env.KOKORO_MODEL_DIR = "/home/…/models" — katalog
    # z prawem zapisu, do którego biblioteka sama pobierze dowolny głos.
    env.KOKORO_MODEL_DIR = lib.mkDefault "${cfg.kokoroModel}";
  };
}
