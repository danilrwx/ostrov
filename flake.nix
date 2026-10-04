{
  description = "ostrov: a whole desktop shell for Hyprland in one binary";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: rec {
        ostrov = pkgs.rustPlatform.buildRustPackage {
          pname = "ostrov";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          # ostrov and its official plugins (plugins/README.md), the binaries side by side in $out/bin
          cargoBuildFlags = [ "--workspace" ];
          cargoTestFlags = [ "--workspace" ];

          nativeBuildInputs = with pkgs; [ pkg-config wrapGAppsHook4 ];
          buildInputs = with pkgs; [ gtk4 gtk4-layer-shell glib pam wayland ];

          # the programs ostrov runs: the sound's; the plugins record's and games' (wf-recorder, powerprofilesctl)
          preFixup = ''
            gappsWrapperArgs+=(--prefix PATH : ${pkgs.lib.makeBinPath (with pkgs; [ wireplumber pipewire wf-recorder wl-clipboard power-profiles-daemon udisks ])})
          '';

          # NixOS reads no PAM file of a package's: security.pam.services.ostrov = {}; makes the lock screen's
          postInstall = ''
            install -Dm644 completions/_ostrov $out/share/zsh/site-functions/_ostrov
            # xdg-desktop-portal-hyprland's screen-share picker, ostrov run by that name (README.md, Screen sharing)
            ln -s ostrov $out/bin/ostrov-share-picker
            install -Dm644 completions/ostrov.bash $out/share/bash-completion/completions/ostrov
            install -Dm644 completions/ostrov.fish $out/share/fish/vendor_completions.d/ostrov.fish
            # the official plugins' manifests and texts, read from share/ beside ostrov's bin/; hello, the SDK's
            # example, is not one
            rm -f $out/bin/ostrov-plugin-hello
            for d in plugins/*/; do
              id=$(basename "$d")
              if [ "$id" = hello ] || [ ! -f "$d/manifest.toml" ]; then continue; fi
              install -Dm644 "$d/manifest.toml" -t "$out/share/ostrov/plugins/$id"
              if [ -d "$d/i18n" ]; then install -Dm644 "$d"/i18n/* -t "$out/share/ostrov/plugins/$id/i18n"; fi
            done
          '';

          meta = with pkgs.lib; {
            description = "A whole desktop shell for Hyprland in one binary";
            homepage = "https://github.com/ostrov-shell/ostrov";
            license = licenses.mit;
            platforms = platforms.linux;
            mainProgram = "ostrov";
          };
        };
        default = ostrov;
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.system}.ostrov ];
          packages = with pkgs; [ cargo rustc clippy rustfmt ];
        };
      });
    };
}
