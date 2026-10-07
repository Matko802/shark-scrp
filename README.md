<div align="center">

# shark-scrp — Shark Media Station

GTK4 + libadwaita **compressor platform** with **TikTok slideshow** and **yt-dlp downloader** built in.
Overengineered inside, one-button simple outside.

</div>

## Tabs

- **Compress** — drop files, pick `Balanced / Tiny / Quality`, hit Compress.
  - JPEG → MozJPEG (`cjpeg`), fallback ImageMagick
  - PNG → ECT lossless (`-9 --strict`), fallback ImageMagick strip
  - GIF → Gifsicle (`-O3 --colors --lossy`), fallback ImageMagick
  - WebP / AVIF / other images → ImageMagick
  - Video → FFmpeg (`libx264/x265/VP9`, CRF + preset + max-height)
  - Audio → FFmpeg (`opus/mp3/aac/flac`)
  - Queue with per-job progress, before → after + % saved, clear-finished
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
