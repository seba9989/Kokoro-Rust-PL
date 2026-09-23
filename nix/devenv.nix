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

  options.plkokoro = {
    onnxruntime = lib.mkOption {
      type = lib.types.package;
      default = pkgs.onnxruntime;
      defaultText = lib.literalExpression "pkgs.onnxruntime";
      description = ''
        ONNX Runtime (>= 1.21), którego libonnxruntime.so wskazuje ORT_LIBRARY_PATH. Crate `ort` ładuje ją dynamicznie,
        więc nic nie jest linkowane w czasie budowania. Testowane z 1.21.0, 1.22.0, 1.23.2, 1.27.1 i 1.30.0.
      '';
    };
    doctor = {
      title = lib.mkOption {
        type = lib.types.str;
        default = "plkokoro — diagnostyka środowiska";
        description = "Pierwsza linia wypisywana przez `pk-doctor`.";
      };
      extraChecks = lib.mkOption {
        type = lib.types.lines;
        default = "";
        example = lib.literalExpression ''
          '''
            command -v cargo-tauri >/dev/null && ok "cargo-tauri: $(cargo tauri --version)" || warn "brak cargo-tauri"
          '''
        '';
        description = ''
          Dodatkowe testy projektu dopisywane do `pk-doctor` (bash) przed sprawdzeniem zależności plkokoro. Dostępne są
          funkcje `ok`, `warn` (oznacza porażkę — kod wyjścia 1) i `info`. Definiuj własne testy tutaj, a nie przez
          `scripts.pk-doctor` — ta komenda należy do modułu.
        '';
      };
    };
  };

  config = {
    # Niski priorytet jak w modułach: nadpisywalne w devenv.nix / devenv.local.nix.
    env.ORT_LIBRARY_PATH = lib.mkDefault "${lib.getLib cfg.onnxruntime}/lib/libonnxruntime.so";

    # Sprawdza to, co najczęściej psuje uruchomienie: brak ORT, runnera, wag języka, modelu albo wybranego głosu.
    scripts.pk-doctor = {
      description = "Diagnostyka zależności plkokoro (ORT, Phonemis, model Kokoro) i testów projektu";
      exec = ''
        bad=0
        ok()   { printf '  \033[32mOK\033[0m    %s\n' "$*"; }
        warn() { printf '  \033[33mBRAK\033[0m  %s\n' "$*"; bad=1; }
        info() { printf '  ....  %s\n' "$*"; }

        echo ${lib.escapeShellArg cfg.doctor.title}
        ${cfg.doctor.extraChecks}

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
          for v in ${toString cfg.voices}; do
            f="$(find "$KOKORO_MODEL_DIR" -path "*/voices/*/$v.bin" 2>/dev/null | head -n 1)"
            if [ -n "$f" ]; then ok "głos $v: $(basename "$(dirname "$f")")/$v"
            else warn "brak głosu $v w $KOKORO_MODEL_DIR (plkokoro.voices)"; fi
          done
        else
          warn "brak modelu Kokoro w $KOKORO_MODEL_DIR"
        fi
        exit $bad
      '';
    };

    # Czytelny błąd zamiast tajemniczego "unsupported API version" przy zbyt starym nixpkgs.
    assertions = [
      {
        assertion = lib.versionAtLeast cfg.onnxruntime.version "1.21";
        message = "plkokoro wymaga libonnxruntime >= 1.21, a wybrany nixpkgs ma ${cfg.onnxruntime.version}. Zaktualizuj input nixpkgs albo ustaw plkokoro.onnxruntime.";
      }
    ];
  };
}
