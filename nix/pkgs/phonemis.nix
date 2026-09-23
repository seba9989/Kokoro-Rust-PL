{ lib
, stdenv
, fetchFromGitHub
, fetchurl
, cmake
  # Języki fonemizera do zainstalowania (kody profili Phonemis, jak plkokoro::PHONEMIS_LANGS).
, languages ? [ "pl" ]
}:

# Phonemis jako pakiet Nix: phonemis_runner zbudowany z CMake + wagi wybranych języków (Git LFS, `fetchurl`).
# Używany przez moduł nix/modules/phonemis.nix.
let
  # Podbijaj ręcznie razem z sumami niżej: `git ls-remote https://github.com/IgorSwat/Phonemis.git HEAD`, potem
  # `nix-prefetch-url --unpack https://github.com/IgorSwat/Phonemis/archive/<rev>.tar.gz` (źródła) oraz nowe `oid`
  # z plików-wskaźników LFS: `curl -sL https://raw.githubusercontent.com/IgorSwat/Phonemis/<rev>/data/<lang>/<plik>`.
  rev = "71eb1ce33bd586d38cbac037843b8539d7829c3b"; # HEAD gałęzi main, 2026-09-22

  # Wagi (i dla angielskiego leksykon + tagger) leżą w Git LFS; fetchFromGitHub ściąga tylko wskaźniki, więc każdy
  # plik to osobny fixed-output `fetchurl`. SHA-256 to dokładnie `oid` z pliku-wskaźnika (specyfikacja Git LFS).
  # Nazwa wag: phonemizer_<kod z `-` -> `_`>.bin (plkokoro::phonemis_weights_file).
  files = import ./phonemis-files.nix;
  supportedLanguages = lib.attrNames files;

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
  "phonemis: nieznane języki ${toString unknown}; dostępne: ${toString supportedLanguages}";
assert lib.assertMsg (languages != [ ]) "phonemis: lista languages jest pusta";

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
    # bin/ dla PATH (nix shell) i wrapperów; biblioteka rozwiązuje symlink (canonicalize) i szuka wag obok build/.
    mkdir -p "$out/bin" && ln -s ../build/phonemis_runner "$out/bin/phonemis_runner"
    ${lib.concatMapStrings (f: ''
      install -Dm644 ${f.src} "$out/data/${f.lang}/${f.name}"
    '') dataFiles}
    install -Dm644 "$src/LICENSE" "$out/LICENSE"
    printf 'phonemis %s\njęzyki: %s\nzbudowane przez Nix (nix/pkgs/phonemis.nix)\n' ${rev} "${toString languages}" > "$out/BUILDINFO"
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

  passthru = { inherit languages rev supportedLanguages; };

  meta = {
    description = "Phonemis — G2P dla Kokoro: phonemis_runner + wagi wybranych języków (${toString languages})";
    homepage = "https://github.com/IgorSwat/Phonemis";
    license = lib.licenses.mit;
    platforms = lib.platforms.unix;
    mainProgram = "phonemis_runner";
  };
}
