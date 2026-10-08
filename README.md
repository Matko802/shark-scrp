<div align="center">

# shark-scrp

GTK4 + libadwaita **compressor platform** with **TikTok slideshow** and **yt-dlp downloader** built in.
Overengineered inside, one-button simple outside.

</div>

## Tabs

- **Compress** — add files or paste a URL, pick a target size, hit Compress.
  - Target sizes: 8 / 10 / 25 / 50 / 100 / 500 MB or custom, with Remember choice
  - Output formats: Auto, MP4, WebM, MP3, Opus, JPEG, PNG, WebP, GIF, each with an efficiency note
  - Effort levels: Fast / Balanced / Thorough (quality-time tradeoff, Thorough uses two-pass video)
  - Auto-fit engine: tries a high-quality encode first, then searches downward (quality binary search for images, Auto-Rez bitrate + resolution ladder for video, bitrate ladder for audio) so the result is the best quality that fits
  - Backends: JPEG → MozJPEG, PNG → ECT, GIF → Gifsicle, other images → ImageMagick, video/audio → FFmpeg
  - 2 GiB input limit, per-job progress, before → after + % saved, per-file Open button
- **Download** — one URL field:
  - TikTok / Instagram → slideshow video (swipe, music muxed), images-only folder, or direct video
  - YouTube / anything else → yt-dlp best-video or audio-only
  - Paste button, output folder, transition/FPS/no-music options
- **Tools dialog** — live probe of `ffmpeg ffprobe magick gifsicle cjpeg ect yt-dlp` + parallel-jobs / keep-originals settings

Presets live in `~/.config/shark-scrp/compress-presets.json`, settings in `settings.json`.

## Run

```sh
nix run github:Matko802/shark-scrp
```

Dev:

```sh
nix develop
cargo test
cargo run
```

Needs on Nix: `gtk4 libadwaita ffmpeg imagemagick gifsicle mozjpeg efficient-compression-tool yt-dlp`.
`yt-dlp` is also auto-fetched from GitHub into `~/.cache/shark-scrp` if missing.

## Nix flakes

`shark-scrp` ships its own flake, so you can pull it straight from GitHub.

### As a flake input

```nix
{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    shark-scrp = {
      url = "github:Matko802/shark-scrp";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, shark-scrp, ... }: {
    packages.x86_64-linux.default = shark-scrp.packages.x86_64-linux.default;
  };
}
```

### As an overlay

```nix
nixpkgs.overlays = [ shark-scrp.overlays.default ];
# gives you pkgs.shark-scrp
```

### Standalone

```sh
nix build github:Matko802/shark-scrp
nix run github:Matko802/shark-scrp
```

### Development

```sh
nix develop github:Matko802/shark-scrp
```

## License

This project is released under the MIT License. See [LICENSE](https://github.com/Matko802/shark-scrp/blob/main/LICENSE).
