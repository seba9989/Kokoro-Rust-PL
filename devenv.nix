{ pkgs, lib, config, ... }:

# Środowisko deweloperskie dla plkokoro-rs (port Rust: Kokoro + Phonemis).
#
# Lokalne nadpisania (np. inna ścieżka do Phonemis) wpisuj do devenv.local.nix (devenv wczytuje go
# automatycznie; jest w .gitignore), np.:
#   { lib, ... }: { env.PHONEMIS_RUNNER = "/home/seba9989/Dokumenty/AI/Phonemis/build/phonemis_runner"; }
let
  # libonnxruntime.so z nixpkgs. Crate `ort` ładuje ją dynamicznie (cecha load-dynamic), więc nic nie jest
  # linkowane w czasie budowania. Testowane z 1.21.0, 1.22.0, 1.23.2 i 1.30.0 (nixpkgs-unstable miało 1.27.1).
  ort = pkgs.onnxruntime;
in
{
  # Rust z nixpkgs (rustc, cargo, clippy, rustfmt, rust-analyzer). Żeby użyć nowszego toolchaina niż w nixpkgs:
  #   languages.rust.channel = "stable";   # wymaga inputu rust-overlay w devenv.yaml (devenv podpowie)
  languages.rust.enable = true;

  # Kompilator C dla zależności budowanych z C (ring w ureq/rustls) dostarcza stdenv devenv.
  packages = [
    pkgs.git # devenv nie dodaje gita do powłoki; potrzebny do budowania Phonemis (pk-phonemis-build)
    pkgs.git-lfs # wagi Phonemis (data/pl/phonemizer_pl.bin) leżą w Git LFS
    pkgs.cmake # budowanie phonemis_runner (kompilator C++ dostarcza stdenv)
  ];

  env = {
    # Czytane przez bibliotekę (Config::ort_library ma pierwszeństwo). Ścieżka bezwzględna, więc
    # LD_LIBRARY_PATH nie jest potrzebne.
    ORT_LIBRARY_PATH = "${lib.getLib ort}/lib/libonnxruntime.so";

    # Lokalna instalacja Phonemis w folderze projektu (tworzy ją pk-phonemis-build): <projekt>/phonemis/build/
    # phonemis_runner + <projekt>/phonemis/data/pl/phonemizer_pl.bin (wagi biblioteka znajduje sama na podstawie
    # położenia runnera). Inny runner wskaż w devenv.local.nix.
    PHONEMIS_RUNNER = lib.mkDefault "${config.devenv.root}/phonemis/build/phonemis_runner";

    # Lokalna kopia modelu Kokoro. Układ plików jest wspólny z wersjami Pythona i Go, więc żeby nie pobierać
    # drugi raz, wskaż ich katalog w devenv.local.nix, np.:
    #   env.KOKORO_MODEL_DIR = "/home/seba9989/Dokumenty/AI/Kokoro/tests/models/kokoro-kmp-models";
    KOKORO_MODEL_DIR = lib.mkDefault "${config.devenv.root}/models/kokoro-kmp-models";
  };

  # Czytelny błąd zamiast tajemniczego "unsupported API version" / "no matching package" przy zbyt starym nixpkgs.
  assertions = [
    {
      assertion = lib.versionAtLeast ort.version "1.21";
      message = "plkokoro-rs wymaga libonnxruntime >= 1.21, a wybrany nixpkgs ma ${ort.version}. Zaktualizuj input nixpkgs w devenv.yaml.";
    }
    {
      assertion = lib.versionAtLeast config.languages.rust.toolchain.rustc.version "1.88";
      message = "plkokoro-rs wymaga Rusta >= 1.88 (MSRV crate'a ort), a wybrany nixpkgs ma ${config.languages.rust.toolchain.rustc.version}. Zaktualizuj nixpkgs albo ustaw languages.rust.channel = \"stable\".";
    }
  ];

  # Prefiks "pk-", żeby nie przesłaniać poleceń powłoki (np. `test`).
  scripts.pk-build.exec = ''
    cargo build --release -p plkokoro-cli || exit $?
    mkdir -p "$DEVENV_ROOT/bin"
    cp "$DEVENV_ROOT/target/release/plkokoro" "$DEVENV_ROOT/bin/plkokoro" && echo "zbudowano: $DEVENV_ROOT/bin/plkokoro"
  '';

  # Pełny zestaw z prawdziwym ORT (ORT_LIBRARY_PATH jest ustawione). Testy wymagające ORT bez niego same się
  # pomijają (wypisują SKIP), więc tu widać je w komplecie.
  scripts.pk-test.exec = ''
    cargo test --workspace "$@"
  '';

  # Pobiera model Kokoro do $KOKORO_MODEL_DIR (jednorazowo; potem biblioteka nie używa sieci).
  scripts.pk-fetch-model.exec = ''
    cargo run --release -p plkokoro-cli -- --fetch-only "$@"
  '';

  # Buduje phonemis_runner (CMake, domyślnie Release) w katalogu TYMCZASOWYM, pobiera wagi PL z Git LFS i instaluje
  # na stałe tylko runner + wagi (+ licencję i BUILDINFO) do <projekt>/phonemis. Opcje: pk-phonemis-build --help
  scripts.pk-phonemis-build.exec = ''
    exec bash "$DEVENV_ROOT/scripts/phonemis-build.sh" "$@"
  '';

  # Testy skryptu budującego Phonemis: pk-test-phonemis --src <checkout Phonemis> [--quick].
  # Nie potrzebują sieci LFS ani prawdziwych wag; pełny przebieg = 3 kompilacje Phonemis.
  scripts.pk-test-phonemis.exec = ''
    exec bash "$DEVENV_ROOT/scripts/test-phonemis-build.sh" "$@"
  '';

  # Sprawdza to, co najczęściej psuje uruchomienie (brak runnera, wagi jako wskaźnik LFS, brak ORT).
  scripts.pk-doctor.exec = ''
    bad=0
    ok()   { printf '  \033[32mOK\033[0m    %s\n' "$*"; }
    warn() { printf '  \033[33mBRAK\033[0m  %s\n' "$*"; bad=1; }
    info() { printf '  ....  %s\n' "$*"; }

    echo "plkokoro-rs — diagnostyka środowiska"
    ok "$(rustc --version) / $(cargo --version)"

    if [ -f "$ORT_LIBRARY_PATH" ]; then ok "libonnxruntime: $ORT_LIBRARY_PATH"
    else warn "libonnxruntime nie istnieje: $ORT_LIBRARY_PATH"; fi

    if [ -x "$PHONEMIS_RUNNER" ]; then
      ok "phonemis_runner: $PHONEMIS_RUNNER"
      "$PHONEMIS_RUNNER" >/dev/null 2>&1 </dev/null
      rc=$?
      if [ "$rc" -ge 126 ]; then
        warn "phonemis_runner nie uruchamia się (kod $rc) — np. zniknęła biblioteka po czyszczeniu /nix/store; przebuduj: pk-phonemis-build"
      fi
      weights="$(dirname "$(dirname "$(readlink -f "$PHONEMIS_RUNNER")")")/data/pl/phonemizer_pl.bin"
      if [ ! -f "$weights" ]; then
        warn "brak wag Phonemis: $weights"
      elif head -c 24 "$weights" | grep -q '^version https://git-lfs'; then
        warn "wagi Phonemis to wskaźnik Git LFS: $weights (napraw: pk-phonemis-build albo git -C <repo Phonemis> lfs pull --include='data/pl/*')"
      else
        ok "wagi Phonemis: $weights"
      fi
    else
      warn "phonemis_runner nie istnieje albo nie jest wykonywalny: $PHONEMIS_RUNNER (zbuduj i zainstaluj lokalnie: pk-phonemis-build, albo ustaw env.PHONEMIS_RUNNER w devenv.local.nix)"
    fi

    if [ -d "$KOKORO_MODEL_DIR" ] && [ -n "$(find "$KOKORO_MODEL_DIR" -name '*.onnx' 2>/dev/null | head -n 1)" ]; then
      ok "model Kokoro: $KOKORO_MODEL_DIR"
    else
      info "model Kokoro jeszcze nie pobrany ($KOKORO_MODEL_DIR) — uruchom: pk-fetch-model"
    fi
    exit $bad
  '';

  enterShell = ''
    echo "plkokoro-rs: $(rustc --version | cut -d' ' -f1,2) | polecenia: pk-doctor pk-build pk-test pk-fetch-model pk-phonemis-build"
  '';
}
