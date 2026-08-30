{
  lib,
  craneLib,
  ffmpeg,
  yt-dlp,
  makeWrapper,
}:
let
  version = "1.0.0";
  src = lib.cleanSourceWith {
    src = ./.;
    filter = path: _type:
      !(lib.hasInfix "/target/" path)
      && !(lib.hasInfix "/.git/" path)
      && !(lib.hasInfix "/result" path);
  };
  commonArgs = {
    pname = "shark-scrp";
    inherit version src;
    nativeBuildInputs = [ makeWrapper ];
    buildInputs = [];
  };
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    # Wrap binary so ffmpeg/ffprobe + yt-dlp are in PATH at runtime
    # (video.rs spawns `ffmpeg`/`ffprobe`, extractor.rs spawns `yt-dlp`)
    postInstall = ''
      wrapProgram $out/bin/shark-scrp \
        --prefix PATH : ${lib.makeBinPath [ ffmpeg yt-dlp ]}
    '';
    doCheck = false;
    meta = {
      mainProgram = "shark-scrp";
      description = "TikTok slideshow downloader → swipe video (TikTok-identical, no loop) - Rust";
      homepage = "https://github.com/Matko802/TiktokSlideDowlander";
      license = lib.licenses.mit;
      platforms = lib.platforms.linux;
    };
  }
)
