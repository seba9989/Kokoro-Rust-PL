{
  description = "plkokoro — TTS Kokoro (ONNX) + Phonemis: CLI, pakiety Phonemis i modelu Kokoro, moduł devenv";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs, ... }:
    let
      lib = nixpkgs.lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      # Moduł devenv: opcje plkokoro.voices / plkokoro.phonemisLanguages / plkokoro.onnxruntime, pakiety i zmienne
      # ORT_LIBRARY_PATH, PHONEMIS_RUNNER, KOKORO_MODEL_DIR. W devenv.yaml zamiast tego: `imports: - plkokoro/nix`.
      devenvModules = {
        default = ./nix/devenv.nix;
        plkokoro = ./nix/devenv.nix;
      };

      # Budowanie z własnym `pkgs` i własną listą głosów/języków (jak lib.mkPython w nixpkgs-python).
      lib = {
        mkPhonemis = { pkgs, languages ? [ "pl" ] }: pkgs.callPackage ./nix/pkgs/phonemis.nix { inherit languages; };
        mkKokoroModel = { pkgs, voices ? [ "pm_mateusz" ] }: pkgs.callPackage ./nix/pkgs/kokoro-model.nix { inherit voices; };
        mkPlkokoro = { pkgs, voices ? [ "pm_mateusz" ], phonemisLanguages ? null }:
          pkgs.callPackage ./nix/pkgs/plkokoro.nix { inherit voices phonemisLanguages; };
        # Obsługiwane języki Phonemis i wszystkie głosy z katalogu (do walidacji / list w innych projektach).
        phonemisLanguages = lib.attrNames (import ./nix/pkgs/phonemis-files.nix);
        voices = lib.concatMap (l: map (v: v.id) l.voices) (lib.importJSON ./nix/catalog.json).languages;
      };

      overlays.default = final: _prev: {
        plkokoro = final.callPackage ./nix/pkgs/plkokoro.nix { };
        plkokoro-phonemis = final.callPackage ./nix/pkgs/phonemis.nix { };
        plkokoro-kokoro-model = final.callPackage ./nix/pkgs/kokoro-model.nix { };
      };

      # Domyślne zestawy (głos pm_mateusz, Phonemis pl). Inne: self.lib.mk* albo `.override { voices = [ … ]; }`.
      #   nix run github:seba9989/Kokoro-Rust-PL -- "Cześć." -o out.wav
      #   nix build github:seba9989/Kokoro-Rust-PL#phonemis
      packages = forAllSystems (pkgs: rec {
        plkokoro = pkgs.callPackage ./nix/pkgs/plkokoro.nix { };
        phonemis = pkgs.callPackage ./nix/pkgs/phonemis.nix { };
        kokoro-model = pkgs.callPackage ./nix/pkgs/kokoro-model.nix { };
        default = plkokoro;
      });
    };
}
