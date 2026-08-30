# TikTok Slideshow / Instagram Carousel Downloader → Swipe Video (TikTok-identical, no loop) — Rust

Downloads TikTok **photo slideshow** posts (`/photo/` or `vm.tiktok.com` short links) and Instagram **photo posts / carousels / reels** (`instagram.com/p/…`, `/reel/…`) and converts them to a portrait video (1080×1920) with:

- ✨ **TikTok-identical swipe animation** — card slides left, next card enters from right, `easeOutCubic`, **no gaps** (gap=0, seamless), no rounded corners during swipe, black background
- 🎵 **Original music** automatically downloaded and muxed (AAC 192k, via `yt-dlp` behind the scenes as fallback), video duration perfectly synced to audio — **no repeat / no loop**, stops on last image
- 🎯 **No repetition** — sequence plays once; no boomerang, no loop metadata
- 📸 **Instagram** — photo posts/carousels become swipe videos; reels are downloaded directly as MP4 (video + audio), all anonymously via `yt-dlp`
- 🦀 **Pure Rust + yt-dlp behind** — no Python, single binary, fast cover-resize + frame pipe to `ffmpeg`, `yt-dlp` handles audio fallback + WAF

> Supports: `https://www.tiktok.com/@user/photo/…`, `https://m.tiktok.com/v/…`, `https://vm.tiktok.com/…`, `https://vt.tiktok.com/…`, `https://www.instagram.com/p/…`, `/reel/…`, `/reels/…`, `/tv/…`, `instagr.am/…`

---

## Install (NixOS)

```bash
# Requires: Rust + ffmpeg/ffprobe + yt-dlp (optional)
# Already available: cargo 1.97, ffmpeg 9.0.1-full, yt-dlp 2026.08.19

# Clone / enter project
cd /mnt/ssd/My-Files/Projects/shark-scrp

# Build release binary (16 MB)
cargo build --release
# Binary at target/release/shark-scrp

# Optional: add to PATH
cp target/release/shark-scrp ~/.local/bin/
# or nix: nix-shell -p cargo --run "cargo build --release"
```

### Dependencies (Cargo.toml)
- `clap 4` (CLI), `tokio 1` + `reqwest 0.12` (rustls-tls, no openssl), `image 0.25` + `imageproc 0.25` (fit 1080x1920 Lanczos3, preserve aspect, black bars), `indicatif 0.17` (progress), `serde_json`/`regex` (extractor), `tempfile`, `sha2`/`base64` (WAF PoW)
- System: `ffmpeg` + `ffprobe` in PATH (`/etc/profiles/per-user/matko/bin/ffmpeg`), `yt-dlp` behind extractor/downloader

NixOS `pkg-config`/`openssl` not needed because `reqwest` uses `rustls-tls` and `ffmpeg` is spawned as subprocess (no `ffmpeg-sys` linking).

---

## Usage

### Basic

```bash
./target/release/shark-scrp "https://www.tiktok.com/@account/photo/7400000000000000000" -o output.mp4

./target/release/shark-scrp "https://vm.tiktok.com/XXXX" -o myvideo.mp4

# Custom swipe speed and fps
./target/release/shark-scrp URL --trans 0.6 --fps 30 -o out.mp4

# Silent video (no music)
./target/release/shark-scrp URL --no-music -o silent.mp4

# Keep temp files (images/audio)
./target/release/shark-scrp URL --keep-temp
```

### Instagram

```bash
# Photo post / carousel → swipe video (photo slides only)
./target/release/shark-scrp "https://www.instagram.com/p/DT3mYHqkrRy/" -o insta.mp4

# Reel → downloaded directly as MP4 (video + muxed audio)
./target/release/shark-scrp "https://www.instagram.com/reel/DU_sasHEruj/" -o reel.mp4

# Works with ?igsh= share tokens, /reels/…, and instagr.am short links
```

How Instagram extraction works (`src/extractor.rs`):
- `yt-dlp` is the only anonymous access path (it solves Instagram's challenge/impersonation) — the web page itself is a login-gated shell even for public posts.
- yt-dlp only emits entries with **video** formats, so pure-photo slides are invisible to it. `shark-scrp` runs `yt-dlp --write-pages --skip-download --ignore-errors` in a temp dir and parses yt-dlp's dumped **GraphQL response** (`api_graphql.dump`), reading `carousel_media`/`image_versions2` for every slide (ordered, full resolution) plus the caption and reel duration.
- Mixed carousels: video slides are skipped, photo slides become the swipe video. Multi-video carousels are not supported.
- Reels (single video) are downloaded directly via `yt-dlp -o dest` (video + audio muxed), duration probed with `ffprobe`.
- Posts that are not anonymously accessible surface a clear error suggesting `--cookies`.

### Parameters Explained

| Flag | Default | TikTok-identical |
|------|---------|-----------------|
| `--trans` | `0.6` | 0.5–0.65s is TikTok authentic; easeOutCubic |
| `--fps` | `30` | TikTok uses 30fps |
| `--width`/`--height` | `1080`×`1920` | 9:16 portrait |
| `--image-duration` | `auto` | `audio_duration / n_images` when music exists else 2.8s |

Swipe math (`src/video.rs:142`):
- `image_duration = audio_duration / n` → total perfectly matches music
- `static_time = image_duration - trans_duration`
- last image holds full `image_duration` (no outgoing swipe)

---

## How it Works

1. **Extract** (`src/extractor.rs:1`) — **yt-dlp behind** `TikWM`:
   - **TikWM API** primary (TikTok) — POST `https://www.tikwm.com/api/` with URL, returns `images[]` + `music.play` (13 images for slideshow)
   - `yt-dlp` behind: `extract_ytdlp()` converts `/photo/`→`/video/`, runs `yt-dlp --dump-json` (TikTok extractor uses app API + WAF solver) to get reliable `music_url` (`tiktokcdn` `audio/mp4`) + `duration`; merged: if TikWM music is relative/`/video/music/...` or `duration==0`, supplement with `yt-dlp` (`source: tikwm+yt-dlp`)
   - **Instagram**: `extract_instagram()` runs `yt-dlp --write-pages --skip-download --ignore-errors` into a temp dir and parses the dumped GraphQL response for ordered `carousel_media`/`image_versions2` image URLs + caption + duration (see Instagram section above)
   - Fallback (TikTok): web scrape `__UNIVERSAL_DATA_FOR_REHYDRATION__` / `SIGI_STATE` + regex for `imageURL.urlList` and `music.playUrl` with WAF solver (SHA256 PoW, base64 `cs`/`wci`/`rs`, 1M brute force, set cookies, retry)
   - Handles `vm.tiktok.com` resolve, `//` → `https:`, `/video/music` → `https://www.tikwm.com`, `\u002F` unescape, `statusCode 10204` IP block detection

2. **Download** (`src/downloader.rs:1`) — **yt-dlp behind `reqwest`**:
   - Downloads each image (cover urls) with `indicatif` progress + retry 3× (`1.5s*attempt`) via `reqwest` (UA `Chrome 122`, correct `Referer`); on `HTTP 403`/`builder error` fallback to `yt-dlp --no-warnings -o dest <url>` behind it
   - Downloads music (m4a/mp3) via `reqwest` stream; candidates `https://www.tikwm.com/video/music/...` ↔ `https://www.tiktok.com/...` + `tiktokcdn` absolute; fallback to `yt-dlp` generic extractor for direct `tiktokcdn` URLs

3. **Render** (`src/video.rs:1`) — **simple, no gaps, no rounded, fit aspect**:
   - Resize each image to **fit** 1080×1920 (preserve aspect, `FilterType::Lanczos3`, centered on black `1080x1920` canvas — no cropping)
   - For N images at `total_duration = audio_duration` (now 60s for 13 imgs → 4.62s/img), each holds `image_duration`
   - For each 30fps frame: if `t` in static → show `cover[i]`; else during `trans_duration` → composite `cover[i]` at `x=-eased*travel` and `cover[i+1]` at `x=travel-eased*travel` with `gap=0`, `radius=0` (seamless, no black line), `travel=W+GAP=1080`, `easeOutCubic` (manual `paste_rgba_onto`)
   - Pipes raw `rgb24` frames to `ffmpeg -f rawvideo -pixel_format rgb24 -video_size 1080x1920 -r 30 -i - -c:v libx264 -pix_fmt yuv420p -crf 18 -preset medium -movflags +faststart silent.mp4`
   - Muxes audio via `ffmpeg -i silent -i audio -map 0:v -map 1:a -c:v copy -c:a aac -b:a 192k -shortest -movflags +faststart` → final mp4 **no loop**

---

## Project Structure

```
.
├── Cargo.toml          # Rust manifest (no Python)
├── src/
│   ├── main.rs         # clap CLI + orchestration
│   ├── extractor.rs    # TikTok slideshow + Instagram extractor chain + WAF PoW
│   ├── downloader.rs   # reqwest streaming + indicatif
│   └── video.rs        # Swipe animation renderer + ffmpeg pipe/mux
├── archive-py/         # Original Python version (archived)
├── target/release/shark-scrp  # 16 MB binary
└── README.md
```

---

## Troubleshooting

- **“Failed to extract”**: URL must be a **photo** post (`/photo/`) or an **Instagram** post/carousel/reel. Video URLs (`/video/`) are not slideshows. Try short link `vm.tiktok.com` resolved via browser. If `10204` IP blocked → try VPN/residential IP or `--cookies`.
- **Instagram "not accessible"**: many posts are login-gated even when public in the app; the error suggests `--cookies`. Multi-video IG carousels are not supported.
- **“Audio too small”**: TikTok blocked music CDN; video will be silent but images still converted.
- **WAF challenge**: TikTok sometimes requires JS challenge; Rust solves SHA256 PoW automatically (1M iterations). Wait 30s and retry, TikWM usually bypasses.
- **ffmpeg not found**: `which ffmpeg` must point to binary; on NixOS `ffmpeg` is at `/etc/profiles/per-user/matko/bin/ffmpeg`.
- **Cargo build fails pkg-config**: Not needed with `rustls-tls`; if you enable `native-tls` add `nix-shell -p pkg-config openssl`.

---

## Legal

For personal / educational use. Respect TikTok ToS and creator rights. Do not reupload without permission.

## Verify No Loop

```bash
ffprobe -show_entries stream=codec_name -of csv output.mp4
# mp4 has no loop atom; this tool ensures no `-stream_loop` and no legacy `loop` flag.
# Check duration: ffprobe -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 out.mp4
# Should equal audio_duration and last frame holds.
```
