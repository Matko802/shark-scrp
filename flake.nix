{
  description = "shark-scrp - TikTok slideshow downloader with TikTok-identical swipe (no loop)";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  inputs.crane.url = "github:ipetkov/crane";

  outputs =
    {
      self,
      nixpkgs,
      crane,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      craneLibFor = pkgs: crane.mkLib pkgs;
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          shark-scrp = pkgs.callPackage ./package.nix { craneLib = craneLibFor pkgs; };
        in
        {
          inherit shark-scrp;
          default = shark-scrp;
        }
      );

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.system}.default}/bin/shark-scrp";
          meta.description = "TikTok slideshow downloader";
        };
      });

      overlays.default = final: _prev: {
        shark-scrp = final.callPackage ./package.nix { craneLib = craneLibFor final; };
      };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            ffmpeg
            yt-dlp
          ];
        };
      });
    };
}
