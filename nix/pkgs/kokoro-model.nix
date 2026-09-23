{ lib
, runCommand
, fetchurl
  # Głosy do spakowania (id z catalog.json, np. "pm_mateusz", "df_anna", "af_heart"). Pakiet zawiera każdy głos,
  # model ONNX i tokenizer, których te głosy wymagają (model zwykle ~325 MB; wspólne modele są pobierane raz).
, voices ? [ "pm_mateusz" ]
}:

# Model Kokoro jako pakiet Nix: głosy + ich modele ONNX i tokenizery, pobrane przez `fetchurl` z sumami SHA-256
# z nix/catalog.json i ułożone jak plkokoro::Store (`<out>/v2.1.1/…`). Używany przez moduł
# nix/modules/kokoro-model.nix.
let
  # Musi być zgodne z plkokoro::REVISION (plkokoro/src/store.rs) — biblioteka szuka plików w <dir>/<rewizja>/.
  revision = "v2.1.1";

  # Kopia prawdziwego catalog.json tej rewizji (sprawdzona sha256 z plikiem na HF). Czytana przy ewaluacji, więc
  # ścieżki i sumy SHA-256 artefaktów pochodzą z katalogu, bez import-from-derivation.
  catalogFile = ../catalog.json;
  catalog = lib.importJSON catalogFile;

  allVoices = lib.concatMap (l: map (v: v // { lang = l.id; }) l.voices) catalog.languages;
  byId = lib.listToAttrs (map (v: lib.nameValuePair v.id v) allVoices);
  models = lib.listToAttrs (map (m: lib.nameValuePair m.id m) catalog.models);
  tokenizers = lib.listToAttrs (map (t: lib.nameValuePair t.id t) catalog.tokenizers);

  unknown = lib.filter (id: !(byId ? ${id})) voices;
  chosen = map (id: byId.${id}) voices;
  langs = lib.unique (map (v: v.lang) chosen);
  modelIds = lib.unique (map (v: v.modelId) chosen);
  tokenizerIds = lib.unique (map (m: models.${m}.tokenizerId) modelIds);
  artifacts = lib.unique (
    map (v: v.artifact) chosen
    ++ map (m: models.${m}.artifact) modelIds
    ++ map (t: tokenizers.${t}.artifact) tokenizerIds
  );

  fetch = a: {
    inherit (a) path;
    src = fetchurl {
      url = "https://huggingface.co/${catalog.distribution.repository}/resolve/${revision}/${a.path}";
      inherit (a) sha256;
    };
  };
in
assert lib.assertMsg (catalog.catalogVersion == lib.removePrefix "v" revision)
  "kokoro-model: nix/catalog.json ma wersję ${catalog.catalogVersion}, a revision = ${revision}";
assert lib.assertMsg (unknown == [ ])
  "kokoro-model: nieznane głosy ${toString unknown}; lista: plkokoro --list-voices";
assert lib.assertMsg (voices != [ ]) "kokoro-model: lista voices jest pusta";

runCommand "kokoro-model-${revision}"
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
    install -Dm644 ${catalogFile} "$base/catalog.json"
    ${lib.concatMapStrings (f: ''
      install -Dm644 ${f.src} "$base/${f.path}"
    '') (map fetch artifacts)}
  ''
