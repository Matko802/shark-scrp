{
  description = "shark-scrp - GTK4 compressor + TikTok slideshow + yt-dlp downloader";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      mediaBinPath = pkgs: pkgs.lib.makeBinPath [
        pkgs.ffmpeg
        pkgs.imagemagick
        pkgs.gifsicle
        pkgs.mozjpeg
        pkgs.efficient-compression-tool
        pkgs.yt-dlp
      ];

      shark-scrp =
        { pkgs }:
        pkgs.rustPlatform.buildRustPackage {
          pname = "shark-scrp";
          version = "1.1.0";
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let
                b = pkgs.lib.baseNameOf path;
              in
              !(builtins.elem b [
                "target"
                ".git"
                "archive-py"
                "tmp"
              ])
              && !(pkgs.lib.hasPrefix "result" b)
              && !(pkgs.lib.hasPrefix "sharktmp" b)
              && !(pkgs.lib.hasSuffix ".mp4" b);
          };
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = with pkgs; [
            makeWrapper
            wrapGAppsHook4
            pkg-config
            glib
            gobject-introspection
          ];
          buildInputs = with pkgs; [
            gtk4
            libadwaita
            glib
            pango
            cairo
            gdk-pixbuf
            graphene
          ];
          postInstall = ''
            mkdir -p $out/share/applications $out/share/icons/hicolor/scalable/apps
            cp ${./assets/applications/io.github.matko802.shark-scrp.desktop} $out/share/applications/
            cp ${./assets/icons/hicolor/scalable/apps/io.github.matko802.shark-scrp.svg} $out/share/icons/hicolor/scalable/apps/
            wrapProgram $out/bin/shark-scrp \
              --prefix PATH : ${mediaBinPath pkgs} \
              "''${gappsWrapperArgs[@]}"
          '';
          doCheck = false;
          meta = {
            mainProgram = "shark-scrp";
            description = "shark-scrp - GTK4 compressor + TikTok slideshow + yt-dlp downloader";
            homepage = "https://github.com/Matko802/shark-scrp";
            license = pkgs.lib.licenses.mit;
            platforms = pkgs.lib.platforms.linux;
          };
        };

      overlay = final: _prev: {
        shark-scrp = shark-scrp { pkgs = final; };
      };
    in
    {
      packages = forAllSystems (pkgs: {
        default = shark-scrp { inherit pkgs; };
        shark-scrp = shark-scrp { inherit pkgs; };
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.system}.default}/bin/shark-scrp";
        };
      });

      overlays.default = overlay;

      devShells = forAllSystems (pkgs:
        pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            pkg-config
            glib
            gtk4
            libadwaita
            pango
            cairo
            gdk-pixbuf
            graphene
            gobject-introspection
            ffmpeg
            imagemagick
            gifsicle
            mozjpeg
            efficient-compression-tool
            yt-dlp
            desktop-file-utils
          ];
          shellHook = ''
            export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}:$XDG_DATA_DIRS
          '';
        });
    };
}
