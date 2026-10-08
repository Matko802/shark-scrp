{
  description = "shark-scrp - TikTok slideshow / Instagram carousel downloader -> TikTok-style swipe video";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      shark-scrp =
        { pkgs }:
        pkgs.rustPlatform.buildRustPackage {
          pname = "shark-scrp";
          version = "1.0.0";
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
          nativeBuildInputs = [ pkgs.makeWrapper ];
          # Wrap binary so ffmpeg/ffprobe + yt-dlp are in PATH at runtime
          # (video.rs spawns ffmpeg/ffprobe, extractor.rs spawns yt-dlp)
          postInstall = ''
            wrapProgram $out/bin/shark-scrp \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.ffmpeg pkgs.yt-dlp ]}
          '';
          doCheck = false;
          meta = {
            mainProgram = "shark-scrp";
            description = "TikTok slideshow / Instagram carousel downloader -> TikTok-style swipe video";
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
            ffmpeg
            yt-dlp
          ];
        });
    };
}