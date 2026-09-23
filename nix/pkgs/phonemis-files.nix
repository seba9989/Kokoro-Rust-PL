# Pliki danych Phonemis (Git LFS) na rewizji z nix/pkgs/phonemis.nix: { <język> = { <plik> = <sha256 = oid LFS>; }; }.
# Klucze najwyższego poziomu to obsługiwane języki (plkokoro::PHONEMIS_LANGS); czytane też przez moduł (typ enum)
# bez budowania pakietu.
{
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
}
