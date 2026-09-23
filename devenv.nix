{ pkgs, lib, config, ... }:

# Środowisko deweloperskie dla plkokoro-rs (port Rust: Kokoro + Phonemis).
#
# Zależności czasu działania dostarcza moduł nix/devenv.nix — ten sam, który inne projekty importują
# (`imports: - plkokoro/nix` w devenv.yaml):
#   ORT_LIBRARY_PATH  — pkgs.onnxruntime (opcja plkokoro.onnxruntime)
#   PHONEMIS_RUNNER   — nix/pkgs/phonemis.nix: phonemis_runner (CMake) + wagi `plkokoro.phonemisLanguages`
#   KOKORO_MODEL_DIR  — nix/pkgs/kokoro-model.nix: głosy `plkokoro.voices` + ich modele ONNX i tokenizery
#                       (sumy SHA-256 z nix/catalog.json — kopii prawdziwego katalogu)
# Wszystko to deklaratywne pakiety Nix (przypięte rewizje, sumy SHA-256); nic nie jest pobierane w enterShell.
# Tutaj dochodzi tylko to, co potrzebne do pracy nad samym repo: Rust, asercja MSRV, skrypty pk-*.
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
in
{
  imports = [ ./nix/devenv.nix ];

  config = {
    # Rust z nixpkgs (rustc, cargo, clippy, rustfmt, rust-analyzer). Żeby użyć nowszego toolchaina niż w nixpkgs:
    #   languages.rust.channel = "stable";   # wymaga inputu rust-overlay w devenv.yaml (devenv podpowie)
    languages.rust.enable = true;

    # Kompilator C dla zależności budowanych z C (ring w ureq/rustls) dostarcza stdenv devenv.
    # Pakiety Phonemis i modelu dodaje moduł nix/devenv.nix.
    packages = [
      pkgs.git # devenv nie dodaje gita do powłoki
    ];

    # ORT_LIBRARY_PATH, PHONEMIS_RUNNER i KOKORO_MODEL_DIR ustawia nix/devenv.nix (niski priorytet — nadpisywalne
    # w devenv.local.nix). KOKORO_LANG / KOKORO_VOICE celowo nieustawione: zmieniałyby domyślne zachowanie testów.

    # Czytelny błąd zamiast "no matching package" przy zbyt starym nixpkgs (asercja ORT jest w nix/devenv.nix).
    assertions = [
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
