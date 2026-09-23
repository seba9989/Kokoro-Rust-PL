{ pkgs, lib, config, ... }:

# Środowisko deweloperskie dla plkokoro-rs (port Rust: Kokoro + Phonemis).
#
# Wszystkie zależności czasu działania to deklaratywne pakiety Nix: przypięta wersja/rewizja, sumy SHA-256,
# budowane/pobierane przez `devenv shell` hermetycznie i cache'owane jak każdy pakiet. Nic nie jest klonowane,
# kompilowane ani pobierane w enterShell.
#
#   ort          — pkgs.onnxruntime (z nixpkgs)
#   phonemis     — nix/phonemis.nix: phonemis_runner (CMake) + wagi języków z `plkokoro.phonemisLanguages`
#   kokoro-model — nix/kokoro-model.nix: głosy z `plkokoro.voices` + ich modele ONNX i tokenizery (sumy SHA-256
#                  z nix/catalog.json — kopii prawdziwego katalogu)
#
# Wybór głosów i języków (i inne nadpisania) wpisuj do devenv.local.nix — devenv wczytuje go automatycznie, jest
# w .gitignore. Przykład:
#   { lib, ... }: {
#     plkokoro.voices = [ "pm_mateusz" "df_anna" "af_heart" ];   # + niemiecki i amerykański angielski
#     plkokoro.phonemisLanguages = [ "pl" "de" "en-us" "fr" ];    # opcjonalnie; domyślnie wynika z głosów
#   }
# Lista głosów: `plkokoro --list-voices` (albo nix/catalog.json).
let
  cfg = config.plkokoro;

  # libonnxruntime.so z nixpkgs. Crate `ort` ładuje ją dynamicznie (cecha load-dynamic), więc nic nie jest
  # linkowane w czasie budowania. Testowane z 1.21.0, 1.22.0, 1.23.2, 1.27.1 i 1.30.0.
  ort = pkgs.onnxruntime;
  kokoroModel = pkgs.callPackage ./nix/kokoro-model.nix { inherit (cfg) voices; };
  phonemis = pkgs.callPackage ./nix/phonemis.nix { languages = cfg.phonemisLanguages; };
in
{
  options.plkokoro = {
    voices = lib.mkOption {
      type = lib.types.nonEmptyListOf lib.types.str;
      default = [ "pm_mateusz" ];
      example = [ "pm_mateusz" "df_anna" "af_heart" ];
      description = ''
        Głosy Kokoro (id z nix/catalog.json) spakowane do KOKORO_MODEL_DIR razem z modelami i tokenizerami, których
        wymagają. Katalog jest w /nix/store (tylko do odczytu): głos spoza listy daje błąd z podpowiedzią.
      '';
    };
    phonemisLanguages = lib.mkOption {
      type = lib.types.nonEmptyListOf lib.types.str;
      default = lib.intersectLists (kokoroModel.passthru.phonemisLanguages ++ [ "pl" ])
        (pkgs.callPackage ./nix/phonemis.nix { }).passthru.supportedLanguages;
      defaultText = lib.literalMD "języki frontendu tekstowego wybranych głosów (z katalogu) + `pl`";
      example = [ "pl" "de" "en-us" ];
      description = "Języki Phonemis, których wagi są instalowane obok phonemis_runner (kody jak plkokoro::PHONEMIS_LANGS).";
    };
  };

  config = {
    # Rust z nixpkgs (rustc, cargo, clippy, rustfmt, rust-analyzer). Żeby użyć nowszego toolchaina niż w nixpkgs:
    #   languages.rust.channel = "stable";   # wymaga inputu rust-overlay w devenv.yaml (devenv podpowie)
    languages.rust.enable = true;

    # Kompilator C dla zależności budowanych z C (ring w ureq/rustls) dostarcza stdenv devenv.
    packages = [
      pkgs.git # devenv nie dodaje gita do powłoki
      phonemis
      kokoroModel
    ];

    env = {
      # Czytane przez bibliotekę (Config::* ma pierwszeństwo). Ścieżki w /nix/store; nadpisanie w devenv.local.nix
      # (lib.mkDefault niżej), np. env.KOKORO_MODEL_DIR = "/home/…/models" — katalog z prawem zapisu, do którego
      # biblioteka sama pobierze dowolny głos.
      ORT_LIBRARY_PATH = "${lib.getLib ort}/lib/libonnxruntime.so";
      # Wagi biblioteka znajduje sama: <pakiet>/data/<język>/phonemizer_<język>.bin obok <pakiet>/build/phonemis_runner.
      PHONEMIS_RUNNER = lib.mkDefault "${phonemis}/build/phonemis_runner";
      KOKORO_MODEL_DIR = lib.mkDefault "${kokoroModel}";
      # KOKORO_LANG / KOKORO_VOICE celowo nieustawione: zmieniałyby domyślne zachowanie testów (`pk-test`).
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

    # Sprawdza to, co najczęściej psuje uruchomienie (brak runnera, brak wag języka, brak ORT, brak głosu).
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
          warn "phonemis_runner nie uruchamia się (kod $rc)"
        fi
        data="$(dirname "$(dirname "$(readlink -f "$PHONEMIS_RUNNER")")")/data"
        for lang in ${toString cfg.phonemisLanguages}; do
          w="$data/$lang/phonemizer_''${lang//-/_}.bin"
          if [ ! -f "$w" ]; then
            warn "brak wag Phonemis ($lang): $w"
          elif head -c 24 "$w" | grep -q '^version https://git-lfs'; then
            warn "wagi Phonemis ($lang) to wskaźnik Git LFS: $w"
          else
            ok "wagi Phonemis ($lang): $w"
          fi
        done
      else
        warn "phonemis_runner nie istnieje albo nie jest wykonywalny: $PHONEMIS_RUNNER"
      fi

      if [ -n "$(find "$KOKORO_MODEL_DIR" -name '*.onnx' 2>/dev/null | head -n 1)" ]; then
        ok "model Kokoro: $KOKORO_MODEL_DIR"
        for v in $(find "$KOKORO_MODEL_DIR" -path '*/voices/*.bin' 2>/dev/null | sort); do
          info "głos: $(basename "$(dirname "$v")")/$(basename "$v" .bin)"
        done
      else
        warn "brak modelu Kokoro w $KOKORO_MODEL_DIR"
      fi
      exit $bad
    '';

    enterShell = ''
      echo "plkokoro-rs: $(rustc --version | cut -d' ' -f1,2) | głosy: ${toString cfg.voices} | Phonemis: ${toString cfg.phonemisLanguages}"
      echo "polecenia: pk-doctor pk-build pk-test"
    '';
  };
}
