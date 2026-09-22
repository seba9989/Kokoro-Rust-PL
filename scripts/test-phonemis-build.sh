#!/usr/bin/env bash
# Testy skryptu phonemis-build.sh. Nie potrzebują sieci LFS ani prawdziwych wag: budują z lokalnych
# "fałszywych upstreamów" (repozytoria git złożone ze źródeł Phonemis), więc działają offline.
#
# Użycie: test-phonemis-build.sh [--src DIR] [--quick] [--keep]
#   --src DIR   checkout Phonemis, z którego wziąć ŹRÓDŁA (domyślnie: płytki klon z $PHONEMIS_REPO_URL, wymaga sieci)
#   --quick     tylko walidacja opcji i szybki scenariusz błędu (bez pełnych kompilacji; ok. sekund)
#   --keep      nie usuwaj katalogu roboczego testu (wypisze ścieżkę)
#
# Pełny przebieg = 3 świeże kompilacje Phonemis (ok. 1 min każda na jednym rdzeniu).
# Kod wyjścia: 0 = wszystkie asercje przeszły, 1 = są porażki.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_SH="$SCRIPT_DIR/phonemis-build.sh"
REPO_URL="${PHONEMIS_REPO_URL:-https://github.com/IgorSwat/Phonemis.git}"

src="" quick=0 keep=0
while [ $# -gt 0 ]; do
  case "$1" in
    --src)   src="$2"; shift 2 ;;
    --quick) quick=1; shift ;;
    --keep)  keep=1; shift ;;
    -h|--help) sed -n '2,12p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "nieznana opcja: $1" >&2; exit 2 ;;
  esac
done

for tool in git cmake; do command -v "$tool" >/dev/null || { echo "brak $tool" >&2; exit 2; }; done
[ -x "$BUILD_SH" ] || [ -f "$BUILD_SH" ] || { echo "brak $BUILD_SH" >&2; exit 2; }

ws="$(mktemp -d "${TMPDIR:-/tmp}/pk-phonemis-test.XXXXXX")"
cleanup() {
  if [ "$keep" = 1 ]; then echo "katalog roboczy testu zachowany: $ws"; else rm -rf "$ws"; fi
}
trap cleanup EXIT
mkdir -p "$ws/tmp" "$ws/proj/scripts"
# Testujemy KOPIĘ skryptu: domyślny prefix (<skrypt>/../phonemis) trafia wtedy do katalogu testu, a nie do projektu.
cp "$BUILD_SH" "$ws/proj/scripts/phonemis-build.sh"
BUILD_SH="$ws/proj/scripts/phonemis-build.sh"
DEFAULT_PREFIX="$ws/proj/phonemis"

pass=0 fail=0
ok()    { pass=$((pass+1)); printf '  \033[32mPASS\033[0m  %s\n' "$1"; }
bad()   { fail=$((fail+1)); printf '  \033[31mFAIL\033[0m  %s\n' "$1"; }
check() { local name="$1"; shift; if "$@" >/dev/null 2>&1; then ok "$name"; else bad "$name"; fi; }
title() { printf '\n== %s\n' "$*"; }
# Predykaty do check (jeden warunek = jedna asercja; łączenie przez && poza check nie wpływałoby na wynik).
rc_is()  { [ "$rc" = "$1" ]; }
rc_not() { [ "$rc" != "$1" ]; }
logs()   { grep -q -- "$2" "$1"; }          # logs PLIK FRAGMENT
nologs() { ! grep -q -- "$2" "$1"; }

# --- źródła -------------------------------------------------------------------------------------------
if [ -z "$src" ]; then
  echo "pobieranie źródeł: $REPO_URL"
  GIT_LFS_SKIP_SMUDGE=1 git clone --depth 1 -q "$REPO_URL" "$ws/src" || { echo "nie udało się pobrać źródeł (podaj --src)" >&2; exit 2; }
  src="$ws/src"
fi
[ -f "$src/CMakeLists.txt" ] && [ -d "$src/src/phonemis" ] || { echo "$src nie wygląda na checkout Phonemis" >&2; exit 2; }

# mk_upstream NAZWA pointer|blob [broken]  ->  $ws/up-NAZWA (repo git z tagiem v0.0.0-test)
#   pointer: wagi PL jako wskaźnik LFS (jak upstream bez git-lfs); blob: 2 MB zwykłego pliku (zepsute wagi)
mk_upstream() {
  local u="$ws/up-$1"
  mkdir -p "$u"
  (cd "$src" && tar --exclude=.git --exclude=build --exclude=data --exclude=.gitattributes -cf - .) | (cd "$u" && tar xf -)
  mkdir -p "$u/data/pl"
  case "$2" in
    pointer) printf 'version https://git-lfs.github.com/spec/v1\noid sha256:%064d\nsize 7094120\n' 0 > "$u/data/pl/phonemizer_pl.bin" ;;
    blob)    head -c 2000000 /dev/zero | tr '\0' '\377' > "$u/data/pl/phonemizer_pl.bin" ;;
  esac
  [ "${3:-}" = broken ] && printf '\n#error test_failure\n' >> "$u/src/phonemis/base/types.h"
  (cd "$u" && git init -q -b main && git add -A && git -c user.email=t@t -c user.name=t commit -q -m test && git tag v0.0.0-test)
}

# bld LOG ARGI...  ->  uruchamia skrypt z izolowanym TMPDIR; kod wyjścia w $rc
bld() {
  local log="$1"; shift
  TMPDIR="$ws/tmp" PHONEMIS_REPO_URL="$URL" bash "$BUILD_SH" "$@" >"$log" 2>&1
  rc=$?
}
tmp_clean()  { [ -z "$(ls -A "$ws/tmp")" ]; }
tree_hash()  { (cd "$1" && find . -type f | sort | xargs sha256sum | sha256sum | cut -c1-20); }
file_list()  { (cd "$1" && find . -type f | sort | tr '\n' ' '); }

# =============================================================================================
title "V: walidacja opcji (bez kompilacji)"
URL="file://$ws/none"
expect_err() { # expect_err NAZWA FRAGMENT ARGI...
  local name="$1" frag="$2"; shift 2
  bld "$ws/v.log" "$@"
  if [ "$rc" = 1 ] && grep -q -- "$frag" "$ws/v.log"; then ok "$name"; else bad "$name (rc=$rc)"; fi
}
expect_err "nieznana opcja"                   "nieznana opcja"            --nie-ma
expect_err "--jobs nie-liczba"                "wymaga liczby całkowitej"  --jobs abc
expect_err "--jobs bez wartości"              "wymaga wartości"           --jobs
expect_err "--type nieznany"                  "nieznany typ budowania"    --type Foo
expect_err "--update i --ref wykluczają się"  "wykluczają się"            --update --ref v1 --dir "$ws/w"
expect_err "--update bez --dir"               "tylko z --dir"             --update
expect_err "--clean bez --dir"                "tylko z --dir"             --clean
expect_err "--no-install bez --dir"           "wymaga --dir"              --no-install
expect_err "--tests bez --dir/--keep-tmp"     "tylko z --dir albo --keep-tmp" --tests
expect_err "--prefix z --no-install"          "wykluczają się"            --dir "$ws/w" --no-install --prefix "$ws/p"
mkdir -p "$ws/niegit" && echo x > "$ws/niegit/plik"
expect_err "--dir istnieje i nie jest repo"   "nie jest repozytorium git" --dir "$ws/niegit" --no-lfs
bld "$ws/v.log" --help
check "--help: kod 0"                          rc_is 0
check "--help: pokazuje użycie"                logs "$ws/v.log" "^Użycie"

# =============================================================================================
title "przygotowanie fałszywych upstreamów"
mk_upstream pointer pointer
mk_upstream blob blob
[ -f "$ws/up-pointer/src/phonemis/base/types.h" ] && mk_upstream broken pointer broken
ok "repozytoria przygotowane ($(ls -d "$ws"/up-* | wc -l))"

# =============================================================================================
title "S2: nieudane budowanie nie rusza istniejącej instalacji"
# Najpierw sztuczna "poprzednia instalacja", potem budowanie zepsutego upstreamu.
if [ -d "$ws/up-broken" ]; then
  mkdir -p "$ws/pfx-old/build"; echo "stary runner" > "$ws/pfx-old/build/phonemis_runner"; echo "stare" > "$ws/pfx-old/BUILDINFO"
  before="$(tree_hash "$ws/pfx-old")"
  URL="file://$ws/up-broken"
  bld "$ws/s2.log" --no-lfs --no-verify --prefix "$ws/pfx-old"
  check "build kończy się błędem (rc=$rc)"             rc_not 0
  check "poprzednia instalacja nietknięta (te same sumy kontrolne)" test "$(tree_hash "$ws/pfx-old")" = "$before"
  check "katalog tymczasowy posprzątany mimo błędu"    tmp_clean
  check "brak plików .new w prefixie"                  test -z "$(find "$ws/pfx-old" -name '*.new')"
else
  echo "  (pomijam: brak src/phonemis/base/types.h w źródłach)"
fi

if [ "$quick" = 1 ]; then
  echo; echo "tryb --quick: pomijam scenariusze z pełną kompilacją (S1, S3, S4)"
else
# =============================================================================================
title "S1: tryb domyślny — build w katalogu tymczasowym, instalacja minimalnego zestawu (wagi = wskaźnik LFS)"
URL="file://$ws/up-pointer"
bld "$ws/s1.log" --no-lfs --no-verify --prefix "$ws/pfx1"
check "rc=0"                                          rc_is 0
check "zainstalowano dokładnie: BUILDINFO LICENSE runner" test "$(file_list "$ws/pfx1")" = "./BUILDINFO ./LICENSE ./build/phonemis_runner "
check "runner ma tryb 755"                            test "$(stat -c %a "$ws/pfx1/build/phonemis_runner")" = 755
check "wag nie zainstalowano (to był wskaźnik LFS)"   test ! -e "$ws/pfx1/data"
check "ostrzeżenie o braku wag w logu"                logs "$ws/s1.log" "wag PL nie zainstalowano"
check "katalog tymczasowy usunięty"                   tmp_clean
check "BUILDINFO: wersja i typ Release"               grep -q "typ:        Release" "$ws/pfx1/BUILDINFO"
check "runner działa z innego katalogu"               bash -c "cd / && '$ws/pfx1/build/phonemis_runner' --lang pl x"
if command -v ldd >/dev/null; then
  check "runner nie zależy od niczego z katalogu testu" bash -c "! ldd '$ws/pfx1/build/phonemis_runner' | grep -q '$ws'"
fi

# =============================================================================================
title "S3: wagi zainstalowane, --keep-tmp, test dymny wykrywa zepsute wagi"
URL="file://$ws/up-blob"
bld "$ws/s3.log" --no-lfs --prefix "$ws/pfx3" --keep-tmp
check "test dymny kończy się błędem na zepsutych wagach (rc=$rc)" rc_not 0
check "komunikat: test dymny"                         logs "$ws/s3.log" "test dymny"
check "wagi zainstalowane (2000000 B, tryb 644)"      test "$(stat -c '%s %a' "$ws/pfx3/data/pl/phonemizer_pl.bin" 2>/dev/null)" = "2000000 644"
check "runner zainstalowany przed testem dymnym"      test -x "$ws/pfx3/build/phonemis_runner"
kept="$(sed -n 's/^katalog tymczasowy zachowany: //p' "$ws/s3.log")"
check "--keep-tmp wypisał ścieżkę zachowanego katalogu" test -n "$kept"
check "--keep-tmp zachował katalog z build/"          test -d "$kept/Phonemis/build"
[ -n "$kept" ] && rm -rf "$kept"

# =============================================================================================
title "S4: trwały --dir + DOMYŚLNY prefix — przyrostowo, reinstalacja, flagi Release, --ref, --update, --no-install"
URL="file://$ws/up-pointer"
bld "$ws/s4a.log" --dir "$ws/work" --no-lfs --no-verify
check "pierwsze uruchomienie: rc=0"                   rc_is 0
check "runner zainstalowany w domyślnym prefixie (<projekt>/phonemis)" test -x "$DEFAULT_PREFIX/build/phonemis_runner"
check "--dir nie tworzy katalogu tymczasowego"        tmp_clean
flags="$(grep -h '^CXX_FLAGS' "$ws/work/build/CMakeFiles/phonemis.dir/flags.make" 2>/dev/null)"
check "Release: flagi kompilacji zawierają -O3"       test -n "$(printf '%s' "$flags" | grep -- '-O3')"
check "Release: flagi kompilacji zawierają -DNDEBUG"  test -n "$(printf '%s' "$flags" | grep -- '-DNDEBUG')"
m1="$(stat -c %Y "$DEFAULT_PREFIX/build/phonemis_runner")"; sleep 1.1
bld "$ws/s4b.log" --dir "$ws/work" --no-lfs --no-verify
check "drugie uruchomienie: rc=0"                     rc_is 0
check "drugie uruchomienie: zero rekompilacji"        test "$(grep -c 'Building CXX' "$ws/s4b.log")" = 0
check "runner w prefixie odświeżony"                  test "$(stat -c %Y "$DEFAULT_PREFIX/build/phonemis_runner")" -gt "$m1"
check "brak plików .new"                              test -z "$(find "$DEFAULT_PREFIX" -name '*.new')"
bld "$ws/s4c.log" --dir "$ws/work" --no-lfs --no-verify --update
check "--update na gałęzi: rc=0"                      rc_is 0
check "--update na gałęzi: wykonał git pull"          logs "$ws/s4c.log" "git pull --ff-only"
bld "$ws/s4d.log" --dir "$ws/work" --no-lfs --no-verify --ref v0.0.0-test
check "--ref TAG: rc=0"                               rc_is 0
check "--ref TAG: przełączył na tag"                  logs "$ws/s4d.log" "przełączanie na v0.0.0-test"
bld "$ws/s4e.log" --dir "$ws/work" --no-lfs --no-verify --update
check "--update na odłączonym HEAD odmawia (rc=1)"    rc_is 1
check "--update na odłączonym HEAD: komunikat"        logs "$ws/s4e.log" "przypięte do wersji"
m_before="$(stat -c %Y "$DEFAULT_PREFIX/build/phonemis_runner")"; sleep 1.1
bld "$ws/s4f.log" --dir "$ws/work" --no-install --no-lfs --no-verify
check "--no-install: rc=0"                            rc_is 0
check "--no-install: brak kroku instalacji w logu"    nologs "$ws/s4f.log" "instalacja do"
check "--no-install: zainstalowany runner bez zmian"  test "$(stat -c %Y "$DEFAULT_PREFIX/build/phonemis_runner")" = "$m_before"
fi

# =============================================================================================
echo
if [ "$fail" = 0 ]; then
  printf '\033[32mWSZYSTKO OK\033[0m — %d asercji przeszło\n' "$pass"
else
  printf '\033[31mPORAŻKI: %d\033[0m (przeszło: %d). Logi: uruchom z --keep i zajrzyj do %s\n' "$fail" "$pass" "$ws"
  keep=1
  exit 1
fi
