{ lib
, stdenv
, fetchFromGitHub
, fetchurl
, cmake
  # Języki fonemizera do zainstalowania (kody profili Phonemis, jak plkokoro::PHONEMIS_LANGS).
, languages ? [ "pl" ]
}:

let
  # Podbijaj ręcznie razem z sumami niżej: `git ls-remote https://github.com/IgorSwat/Phonemis.git HEAD`, potem
  # `nix-prefetch-url --unpack https://github.com/IgorSwat/Phonemis/archive/<rev>.tar.gz` (źródła) oraz nowe `oid`
  # z plików-wskaźników LFS: `curl -sL https://raw.githubusercontent.com/IgorSwat/Phonemis/<rev>/data/<lang>/<plik>`.
  rev = "71eb1ce33bd586d38cbac037843b8539d7829c3b"; # HEAD gałęzi main, 2026-09-22

  # Wagi (i dla angielskiego leksykon + tagger) leżą w Git LFS; fetchFromGitHub ściąga tylko wskaźniki, więc każdy
  # plik to osobny fixed-output `fetchurl`. SHA-256 to dokładnie `oid` z pliku-wskaźnika (specyfikacja Git LFS).
  # Nazwa wag: phonemizer_<kod z `-` -> `_`>.bin (plkokoro::phonemis_weights_file).
  files = {
    pl = { "phonemizer_pl.bin" = "ec85f4dc2c4ac7a72ff88b98b0664a4ed887bd15c0d5add2eb1d6a6ee05b73f2"; };
    de = { "phonemizer_de.bin" = "4888dc7e54dc66098551555096562063364091fc246da0d057b629587bedaa0b"; };
    fr = { "phonemizer_fr.bin" = "fa0018b750a3670328026107b44e02eafe3223b6878585ca0927786b7812d96a"; };
    es = { "phonemizer_es.bin" = "8dc68946e12c1a233ac9153fd369f6418b930a3d731587ff64dc789f44d75a52"; };
    it = { "phonemizer_it.bin" = "dca8d068d76134a40856cd874ea6e5e05c988914369ab643f52ab77a512067e3"; };
    pt = { "phonemizer_pt.bin" = "89049ea03c52ffa7233a343d44a35059aae3b1231d772c8c498fbc4427756ecf"; };
    hi = { "phonemizer_hi.bin" = "dcee3272f96d7f1b7cc40c5df23060b502a5b9066f7cb20092413dff82a487f5"; };
    en-us = {
      "phonemizer_en_us.bin" = "e059561fb8d51eadfd2000965be30e31f0152e7e8c8b4fcf7859dbcce8557576";
      "lexicon_full.json" = "ef0b19a0126455e4216fb08083c8b50f7e85f98f6055738129a89d2095e635d7";
      "tagger.json" = "af2fe9831e8560fa78ebf7d96da715ce5ecb43a3363bd701c952a6db206f169c";
    };
    en-gb = {
      "phonemizer_en_gb.bin" = "3d4fe5a541c02de30879a5b84b88f5229f5b31f8fb2dcf7e89a1ce3a6c330334";
      "lexicon_full.json" = "52167ca536a93d56a02e8b1db29f572438cb7103737ef2c3f8dd9dbfc0b35e0a";
      "tagger.json" = "af2fe9831e8560fa78ebf7d96da715ce5ecb43a3363bd701c952a6db206f169c";
    };
  };

  unknown = lib.filter (l: !(files ? ${l})) languages;

  # [ { lang; name; src; } ] dla wszystkich plików wybranych języków
  dataFiles = lib.concatMap
    (lang: lib.mapAttrsToList
      (name: sha256: {
        inherit lang name;
        src = fetchurl {
          url = "https://media.githubusercontent.com/media/IgorSwat/Phonemis/${rev}/data/${lang}/${name}";
          inherit sha256;
        };
      })
      files.${lang})
    languages;
in
assert lib.assertMsg (unknown == [ ])
  "nix/phonemis.nix: nieznane języki ${toString unknown}; dostępne: ${toString (lib.attrNames files)}";
assert lib.assertMsg (languages != [ ]) "nix/phonemis.nix: lista languages jest pusta";

stdenv.mkDerivation {
  pname = "phonemis";
  version = "unstable-2026-09-22";

  src = fetchFromGitHub {
    owner = "IgorSwat";
    repo = "Phonemis";
    inherit rev;
    sha256 = "0y9bnmxlr1zmvany2qsgszgpi3j129cwf9hf3y1bpymxkspc4npc";
  };

  nativeBuildInputs = [ cmake ];

  # Phonemis nie #include'uje jawnie kilku nagłówków biblioteki standardowej, na których polega (błąd upstream);
  # nowsze libstdc++ nie dociągają ich przechodnio. `NIX_CFLAGS_COMPILE` trafia do każdego wywołania kompilatora.
  env.NIX_CFLAGS_COMPILE = toString (map (h: "-include ${h}") [
    "optional"
    "string"
    "string_view"
    "cstdint"
    "cstddef"
    "vector"
    "unordered_map"
    "unordered_set"
    "map"
    "memory"
    "algorithm"
    "stdexcept"
    "functional"
    "variant"
    "array"
    "cmath"
  ]);

  cmakeFlags = [
    "-DBUILD_RUNNER=ON"
    "-DBUILD_TESTS=OFF"
    "-DCMAKE_BUILD_TYPE=Release"
  ];

  # CMakeLists.txt Phonemis nie ma reguł install(). Układ <out>/build/phonemis_runner + <out>/data/<lang>/… jest
  # wymagany: RunnerG2p::new wyprowadza ścieżkę wag z położenia runnera (<repo>/data/<lang>/phonemizer_<lang>.bin),
  # a leksykon i tagger angielskiego szuka obok wag.
  installPhase = ''
    runHook preInstall
    install -Dm755 phonemis_runner "$out/build/phonemis_runner"
    ${lib.concatMapStrings (f: ''
      install -Dm644 ${f.src} "$out/data/${f.lang}/${f.name}"
    '') dataFiles}
    install -Dm644 "$src/LICENSE" "$out/LICENSE"
    printf 'phonemis %s\njęzyki: %s\nzbudowane przez Nix (nix/phonemis.nix)\n' ${rev} "${toString languages}" > "$out/BUILDINFO"
    runHook postInstall
  '';

  # Test dymny na zainstalowanych plikach: runner działa z wagami każdego języka i zwraca niepuste IPA.
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    ${lib.concatMapStrings (lang: ''
      out_ipa="$("$out/build/phonemis_runner" --lang ${lang} --model "$out/data/${lang}/phonemizer_${lib.replaceStrings [ "-" ] [ "_" ] lang}.bin" "Test 123." | sed "s/\x1b\[[0-9;]*m//g" | sed -n "s/^Output: //p")"
      echo "phonemis ${lang}: $out_ipa"
      [ -n "$(echo "$out_ipa" | tr -d ' ')" ] || { echo "phonemis ${lang}: puste IPA" >&2; exit 1; }
    '') languages}
    runHook postInstallCheck
  '';

  passthru = { inherit languages rev; supportedLanguages = lib.attrNames files; };

  meta = {
    description = "Phonemis — G2P dla Kokoro: phonemis_runner + wagi wybranych języków (${toString languages})";
    homepage = "https://github.com/IgorSwat/Phonemis";
    license = lib.licenses.mit;
    platforms = lib.platforms.unix;
    mainProgram = "phonemis_runner";
  };
}
