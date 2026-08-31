<div align="center">

# shark-scrp

Turns TikTok **photo slideshows** and **Instagram posts / carousels / reels** into a TikTok-style swipe video — no loop, no repetition.

</div>

## Features

- TikTok-like smooth swipe animation (easeOutCubic, seamless, no gaps)
- original audio auto-downloaded and muxed, video length synced to music
- Instagram carousels with a soundtrack are supported
- pure Rust, single binary — `ffmpeg` + `yt-dlp` used behind the scenes

## Building

Not on Nix? Requires `cargo` + `ffmpeg`/`ffprobe` + `yt-dlp`:

```sh
git clone https://github.com/Matko802/shark-scrp.git
cd shark-scrp
cargo build --release
./target/release/shark-scrp <url>
```

On NixOS or any distro with Nix, just:

```sh
nix run github:Matko802/shark-scrp
```

## Usage

```sh
shark-scrp "https://www.tiktok.com/@user/photo/123..."
shark-scrp "https://www.instagram.com/reel/DceabVZT3sN/"
shark-scrp URL -o out.mp4       # custom output
shark-scrp URL --no-music       # silent video
shark-scrp -v                   # version
```

| Flag              | Default     | Meaning                        |
| ----------------- | ----------- | ------------------------------ |
| `-o, --output`    | title name  | output mp4 path                |
| `--trans`         | `0.6`       | swipe transition seconds       |
| `--fps`           | `30`        | video fps                      |
| `--width`/`--height` | `1080`x`1920` | resolution              |
| `--no-music`      |             | skip audio                     |
| `--keep-temp`     |             | keep downloaded images/audio   |

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