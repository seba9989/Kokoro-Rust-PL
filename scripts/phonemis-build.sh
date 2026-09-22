#!/usr/bin/env bash
# Buduje phonemis_runner (Phonemis, CMake) dla plkokoro.
#
# Domyślnie: klonuje repo do KATALOGU TYMCZASOWEGO, buduje tam, a na stałe instaluje tylko to, co potrzebne
# w czasie działania, do folderu biblioteki (domyślnie <projekt>/phonemis):
#     <prefix>/build/phonemis_runner            runner
#     <prefix>/data/pl/phonemizer_pl.bin        wagi PL (~7 MB)
#     <prefix>/LICENSE, <prefix>/BUILDINFO      licencja MIT upstreamu i skąd/jak zbudowano
# Katalog tymczasowy jest usuwany po instalacji. Układ jest taki, że biblioteka (Go i Python) sama znajduje
# wagi na podstawie ścieżki runnera — wystarczy PHONEMIS_RUNNER=<prefix>/build/phonemis_runner.
#
# Wymaga: git, git-lfs (chyba że --no-lfs), cmake, kompilator C++20.
set -euo pipefail

REPO_URL="${PHONEMIS_REPO_URL:-https://github.com/IgorSwat/Phonemis.git}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  cat <<'USAGE'
Użycie: phonemis-build.sh [opcje]

Miejsce instalacji i katalog roboczy:
  --prefix DIR          gdzie zainstalować pliki końcowe (domyślnie: <projekt>/phonemis)
  --no-install          nie instaluj; wynik zostaje w katalogu roboczym (wymaga --dir)
  --dir DIR             trwały katalog roboczy (klon + build) zamiast tymczasowego; kolejne uruchomienia
                        są przyrostowe (bez rekompilacji). Wymagany dla --update i --clean.
  --keep-tmp            nie usuwaj katalogu tymczasowego po budowaniu (wypisze jego ścieżkę)
  --tmp-base DIR        gdzie utworzyć katalog tymczasowy (domyślnie $TMPDIR albo /tmp)

Wersja i budowanie:
  --ref REF             gałąź lub tag, np. v0.1.0 (domyślnie: domyślna gałąź)
  --update              git pull --ff-only w katalogu roboczym (tylko z --dir; nie łącz z --ref)
  --clean               usuń build/ w katalogu roboczym przed konfiguracją (tylko z --dir)
  -j, --jobs N          liczba równoległych zadań (domyślnie: nproc)
  --type TYP            Release (domyślnie) | RelWithDebInfo | MinSizeRel | Debug
  --tests               zbuduj też phonemis_test (tylko z --dir albo --keep-tmp)
  --no-lfs              nie pobieraj wag PL z Git LFS (runner zostanie zainstalowany bez wag)
  --no-verify           pomiń test dymny zainstalowanego runnera
  --no-force-includes   nie dodawaj wymuszonych #include (obejście braków w nagłówkach upstreamu)
  --cmake-arg ARG       dodatkowy argument dla cmake (można powtarzać), np. --cmake-arg -DET_ON=ON
  -h, --help            ta pomoc

Zmienne: PHONEMIS_REPO_URL, CXX, CXXFLAGS (doklejane po wymuszonych nagłówkach), TMPDIR.
USAGE
}

die()  { printf 'błąd: %s\n' "$*" >&2; exit 1; }
warn() { printf 'uwaga: %s\n' "$*" >&2; }
say()  { printf '==> %s\n' "$*"; }

prefix="" work="" ref="" update=0 clean=0 jobs="" type="Release" tests=0 lfs=1 verify=1 force=1
install=1 keep_tmp=0 tmp_base="${TMPDIR:-/tmp}"
cmake_args=()

need_val() { [ "$2" -ge 2 ] || die "opcja $1 wymaga wartości"; }
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix)   need_val "$1" $#; prefix="$2"; shift 2 ;;
    --dir|--work-dir) need_val "$1" $#; work="$2"; shift 2 ;;
    --tmp-base) need_val "$1" $#; tmp_base="$2"; shift 2 ;;
    --ref)      need_val "$1" $#; ref="$2"; shift 2 ;;
    -j|--jobs)  need_val "$1" $#; jobs="$2"; shift 2 ;;
    --type)     need_val "$1" $#; type="$2"; shift 2 ;;
    --cmake-arg) need_val "$1" $#; cmake_args+=("$2"); shift 2 ;;
    --no-install) install=0; shift ;;
    --keep-tmp) keep_tmp=1; shift ;;
    --update)   update=1; shift ;;
    --clean)    clean=1; shift ;;
    --tests)    tests=1; shift ;;
    --no-lfs)   lfs=0; shift ;;
    --no-verify) verify=0; shift ;;
    --no-force-includes) force=0; shift ;;
    -h|--help)  usage; exit 0 ;;
    *)          printf 'błąd: nieznana opcja: %s\n\n' "$1" >&2; usage >&2; exit 1 ;;
  esac
done

case "$type" in Release|RelWithDebInfo|MinSizeRel|Debug) ;; *) die "nieznany typ budowania: $type" ;; esac
case "$jobs" in ""|*[!0-9]*) [ -z "$jobs" ] || die "--jobs wymaga liczby całkowitej" ;; esac
[ -z "$jobs" ] && jobs="$(nproc 2>/dev/null || echo 2)"
[ "$update" = 1 ] && [ -n "$ref" ] && die "--update i --ref wykluczają się (--ref przełącza na wskazaną wersję)"
if [ -z "$work" ]; then
  [ "$update" = 1 ] && die "--update ma sens tylko z --dir (katalog tymczasowy jest za każdym razem nowy)"
  [ "$clean" = 1 ]  && die "--clean ma sens tylko z --dir (katalog tymczasowy jest za każdym razem nowy)"
  [ "$install" = 0 ] && die "--no-install wymaga --dir (inaczej wynik zniknąłby razem z katalogiem tymczasowym)"
  [ "$tests" = 1 ] && [ "$keep_tmp" = 0 ] && die "--tests ma sens tylko z --dir albo --keep-tmp (inaczej testy zniknęłyby z katalogiem tymczasowym)"
fi
[ "$install" = 0 ] && [ -n "$prefix" ] && die "--prefix i --no-install wykluczają się"

# --- narzędzia -----------------------------------------------------------------------------------
command -v git   >/dev/null 2>&1 || die "brak git"
command -v cmake >/dev/null 2>&1 || die "brak cmake (w devenv: pkgs.cmake)"
cxx="${CXX:-c++}"
command -v "$cxx" >/dev/null 2>&1 || die "brak kompilatora C++ ($cxx)"
if [ "$lfs" = 1 ] && ! command -v git-lfs >/dev/null 2>&1; then
  die "brak git-lfs — wagi PL leżą w Git LFS (zainstaluj albo użyj --no-lfs)"
fi

# --- katalogi ------------------------------------------------------------------------------------
[ -z "$prefix" ] && prefix="$SCRIPT_DIR/../phonemis"
prefix="$(realpath -m "$prefix")"

tmp=""
cleanup_tmp() {
  [ -n "$tmp" ] || return 0
  case "$tmp" in */phonemis-build.*) ;; *) return 0 ;; esac   # bezpiecznik przed rm -rf złej ścieżki
  if [ "$keep_tmp" = 1 ]; then
    echo "katalog tymczasowy zachowany: $tmp"
  else
    rm -rf "$tmp"
  fi
  tmp=""
}
trap cleanup_tmp EXIT

if [ -z "$work" ]; then
  mkdir -p "$tmp_base"
  tmp="$(mktemp -d "${tmp_base%/}/phonemis-build.XXXXXX")"
  dir="$tmp/Phonemis"
  say "katalog tymczasowy: $tmp"
else
  dir="$(realpath -m "$work")"
fi
build="$dir/build"
weights="$dir/data/pl/phonemizer_pl.bin"

# Wskaźnik LFS (~130 B) zamiast wag: runner bez poprawnych wag kończy się kodem 0 i zwraca same spacje.
is_pointer() { [ ! -s "$1" ] || head -c 24 "$1" | grep -q '^version https://git-lfs'; }

# --- 1. kod --------------------------------------------------------------------------------------
if [ ! -d "$dir/.git" ]; then
  if [ -e "$dir" ] && [ -n "$(ls -A "$dir" 2>/dev/null)" ]; then
    die "$dir istnieje i nie jest repozytorium git (podaj inny --dir)"
  fi
  say "klonowanie $REPO_URL${ref:+ (ref: $ref)}"
  mkdir -p "$(dirname "$dir")"
  clone=(clone --depth 1)
  [ -n "$ref" ] && clone+=(--branch "$ref")
  # SKIP_SMUDGE: bez pobierania wag wszystkich języków; polskie dociągamy niżej.
  GIT_LFS_SKIP_SMUDGE=1 git "${clone[@]}" "$REPO_URL" "$dir"
else
  say "repozytorium: $dir"
  if [ -n "$ref" ]; then
    say "przełączanie na $ref"
    GIT_LFS_SKIP_SMUDGE=1 git -C "$dir" fetch --depth 1 origin "$ref"
    GIT_LFS_SKIP_SMUDGE=1 git -C "$dir" -c advice.detachedHead=false checkout FETCH_HEAD
  elif [ "$update" = 1 ]; then
    # Na odłączonym HEAD (np. po --ref TAG) pull niczego nie aktualizuje, a kończy się sukcesem.
    git -C "$dir" symbolic-ref -q HEAD >/dev/null \
      || die "repo jest przypięte do wersji ($(git -C "$dir" describe --tags --always)), a nie do gałęzi — --update nic by nie zrobiło. Wróć na gałąź: git -C $dir checkout <gałąź>"
    say "git pull --ff-only"
    GIT_LFS_SKIP_SMUDGE=1 git -C "$dir" pull --ff-only \
      || die "pull nie powiódł się (lokalne zmiany?)"
  fi
fi
version="$(git -C "$dir" describe --tags --always 2>/dev/null || git -C "$dir" rev-parse --short HEAD)"
say "wersja: $version"

# --- 2. wagi (Git LFS) ---------------------------------------------------------------------------
if [ "$lfs" = 1 ]; then
  if is_pointer "$weights"; then
    say "pobieranie wag PL z Git LFS"
    git -C "$dir" lfs install --local >/dev/null
    git -C "$dir" lfs pull --include='data/pl/*'
    is_pointer "$weights" && die "$weights to nadal wskaźnik LFS po lfs pull (sieć? limit LFS?)"
  fi
  say "wagi PL OK ($(wc -c < "$weights") B)"
else
  is_pointer "$weights" && warn "wagi PL nie są pobrane (--no-lfs); runner nie zadziała bez nich"
fi

# --- 3. konfiguracja i budowanie -----------------------------------------------------------------
# Upstream nie dołącza części nagłówków standardowych (np. <optional> w num2word/layer.h, <string> w
# tokenizer/types.h); nowsze libstdc++ nie dociągają ich przechodnio. Wymuszamy je zamiast łatać cudzy kod.
cxxflags=""
if [ "$force" = 1 ]; then
  for h in optional string string_view cstdint cstddef vector unordered_map unordered_set map memory \
           algorithm stdexcept functional variant array cmath; do
    cxxflags="$cxxflags -include $h"
  done
fi
cxxflags="$cxxflags ${CXXFLAGS:-}"

[ "$clean" = 1 ] && { say "usuwanie $build"; rm -rf "$build"; }

# Zmiana kompilatora w istniejącym build/ (np. po aktualizacji nixpkgs ścieżka w /nix/store się zmienia):
# CMake wyrzuca wtedy cache i konfiguruje od nowa, GUBIĄC opcje z linii poleceń (BUILD_RUNNER) —
# skutek to "No rule to make target 'phonemis_runner'". Dlatego w takim przypadku czyścimy build/ sami.
if [ -f "$build/CMakeCache.txt" ]; then
  old_cxx="$(sed -n 's/^CMAKE_CXX_COMPILER:[A-Z]*=//p' "$build/CMakeCache.txt" | head -n 1)"
  new_cxx="$(command -v "$cxx")"
  if [ -n "$old_cxx" ] && [ "$old_cxx" != "$new_cxx" ]; then
    say "zmiana kompilatora ($old_cxx -> $new_cxx) — czyszczenie $build"
    rm -rf "$build"
  fi
fi

# Upstream nie ustawia domyślnego CMAKE_BUILD_TYPE: bez -DCMAKE_BUILD_TYPE dostajesz kod BEZ optymalizacji.
say "cmake ($type, C++: $cxx)"
cmake -S "$dir" -B "$build" \
  -DCMAKE_BUILD_TYPE="$type" \
  -DCMAKE_CXX_COMPILER="$(command -v "$cxx")" \
  -DBUILD_RUNNER=ON \
  -DBUILD_TESTS="$([ "$tests" = 1 ] && echo ON || echo OFF)" \
  -DCMAKE_CXX_FLAGS="$cxxflags" \
  ${cmake_args[@]+"${cmake_args[@]}"}

grep -q '^BUILD_RUNNER:BOOL=ON' "$build/CMakeCache.txt" \
  || die "konfiguracja bez BUILD_RUNNER=ON (nadpisane przez --cmake-arg albo CMake wyczyścił cache) — uruchom z --clean"

targets=(phonemis_runner)
[ "$tests" = 1 ] && targets+=(phonemis_test)
say "budowanie: ${targets[*]} (-j$jobs)"
cmake --build "$build" --target "${targets[@]}" -j "$jobs"

built_runner="$build/phonemis_runner"
[ -x "$built_runner" ] || die "po budowaniu brak $built_runner"
say "zbudowano: $built_runner"
[ "$tests" = 1 ] && say "testy: $build/phonemis_test"

# --- 4. instalacja minimalnego zestawu -----------------------------------------------------------
# Każdy plik trafia najpierw do <plik>.new, potem mv: przerwana instalacja nie zostawia połowy pliku,
# a nieudane budowanie (wyżej) w ogóle nie rusza wcześniej zainstalowanej wersji.
put() { # put ŹRÓDŁO CEL TRYB
  mkdir -p "$(dirname "$2")"
  install -m "$3" "$1" "$2.new"
  mv -f "$2.new" "$2"
}

if [ "$install" = 1 ]; then
  say "instalacja do $prefix"
  put "$built_runner" "$prefix/build/phonemis_runner" 0755
  if is_pointer "$weights"; then
    warn "wag PL nie zainstalowano (brak prawdziwych wag); zainstalowano tylko runner"
  else
    put "$weights" "$prefix/data/pl/phonemizer_pl.bin" 0644
  fi
  [ -f "$dir/LICENSE" ] && put "$dir/LICENSE" "$prefix/LICENSE" 0644
  info="$prefix/BUILDINFO"
  {
    echo "źródło:     $REPO_URL"
    echo "wersja:     $version"
    echo "commit:     $(git -C "$dir" rev-parse HEAD)"
    echo "typ:        $type"
    echo "kompilator: $(command -v "$cxx") ($("$cxx" --version 2>/dev/null | head -n 1))"
    echo "zbudowano:  $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } > "$info.new" && mv -f "$info.new" "$info"
  runner="$prefix/build/phonemis_runner"
  final_weights="$prefix/data/pl/phonemizer_pl.bin"
else
  runner="$built_runner"
  final_weights="$weights"
fi

# Katalog tymczasowy znika PRZED testem dymnym: test sprawdza więc wyłącznie zainstalowane pliki.
cleanup_tmp

# --- 5. test dymny -------------------------------------------------------------------------------
if [ "$verify" = 1 ]; then
  if is_pointer "$final_weights"; then
    warn "pomijam test dymny: brak prawdziwych wag ($final_weights)"
  else
    out="$("$runner" --lang pl --model "$final_weights" "Mam 123 zł." 2>&1)" \
      || die "test dymny: runner zakończył się błędem: $(printf '%s' "$out" | sed 's/\x1b\[[0-9;]*m//g')"
    ipa="$(printf '%s\n' "$out" | sed 's/\x1b\[[0-9;]*m//g' | sed -n 's/^Output: //p' | head -n 1)"
    [ -n "$(printf '%s' "$ipa" | tr -d '[:space:][:punct:]')" ] || die "test dymny: runner zwrócił pusty wynik: $out"
    say "test dymny OK: $ipa"
  fi
fi

echo
echo "Użyj:  export PHONEMIS_RUNNER=$runner"
