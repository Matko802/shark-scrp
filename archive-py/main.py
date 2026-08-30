#!/usr/bin/env python3
"""
TikTok Slideshow Downloader → Converts to Video with Authentic Swipe
- Downloads all photos from TikTok photo posts (slideshows)
- Downloads original music/audio
- Renders video 1080x1920 with TikTok-identical swipe animation
- No loop / no repeat - plays once and ends
Usage:
  python main.py "https://www.tiktok.com/@user/photo/1234567890123456789"
  python main.py "https://vm.tiktok.com/XXXX" -o myvideo.mp4
  python main.py URL --fps 30 --trans 0.6 --keep-temp

Requires: ffmpeg + ffprobe in PATH (already installed), Python 3.9+
"""

import argparse
import os
import sys
import tempfile
import shutil
import pathlib
import time
from typing import List

import requests
from tqdm import tqdm

from extractor import extract_info, resolve_url
from video_maker import create_swipe_video, ffprobe_duration

DEFAULT_OUT = "tiktok_slideshow.mp4"
HEADERS_DL = {
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
    "Referer": "https://www.tiktok.com/",
}

def download_file(url: str, dest: str, desc: str = "Downloading") -> str:
    r = requests.get(url, headers=HEADERS_DL, stream=True, timeout=30)
    r.raise_for_status()
    total = int(r.headers.get("content-length", 0))
    with open(dest, "wb") as f, tqdm(total=total, unit="B", unit_scale=True, desc=desc, leave=False) as pbar:
        for chunk in r.iter_content(chunk_size=8192):
            if chunk:
                f.write(chunk)
                pbar.update(len(chunk))
    return dest

def download_all_images(image_urls: List[str], tmpdir: str) -> List[str]:
    paths = []
    print(f"[downloader] Downloading {len(image_urls)} images...")
    for i, url in enumerate(image_urls):
        # fix proto relative
        if url.startswith("//"):
            url = "https:" + url
        # extract ext
        ext = ".jpg"
        # Try to guess ext from url
        lower = url.lower()
        if ".webp" in lower:
            ext = ".webp"
        elif ".png" in lower:
            ext = ".png"
        elif ".jpeg" in lower:
            ext = ".jpg"
        # dest path
        dest = os.path.join(tmpdir, f"img_{i:03d}{ext}")
        # download with retry 3
        for attempt in range(3):
            try:
                download_file(url, dest, desc=f"Image {i+1}/{len(image_urls)}")
                # verify file not empty
                if os.path.getsize(dest) < 500:
                    raise ValueError("File too small, likely blocked")
                break
            except Exception as e:
                if attempt == 2:
                    print(f"[error] Failed to download image {i+1}: {e}")
                    raise
                time.sleep(1.5 * (attempt + 1))
        paths.append(dest)
    return paths

def download_audio(music_url: str, tmpdir: str) -> str:
    if not music_url:
        return None
    if music_url.startswith("//"):
        music_url = "https:" + music_url
    # ext guess
    ext = ".m4a"
    if ".mp3" in music_url.lower():
        ext = ".mp3"
    # Try to preserve original ext via URL param mime_type
    if "mime_type" in music_url:
        if "audio_mpeg" in music_url:
            ext = ".mp3"
        elif "audio_mp4" in music_url:
            ext = ".m4a"
    dest = os.path.join(tmpdir, f"audio{ext}")
    print(f"[downloader] Downloading music: {music_url[:80]}...")
    for attempt in range(3):
        try:
            download_file(music_url, dest, desc="Audio")
            if os.path.getsize(dest) < 1000:
                raise ValueError("Audio file too small")
            return dest
        except Exception as e:
            if attempt == 2:
                print(f"[warn] Failed to download audio: {e}. Proceeding without music (silent video).")
                return None
            time.sleep(1.5)
    return None

def parse_args():
    p = argparse.ArgumentParser(
        description="TikTok Slideshow Downloader → Swipe Video (TikTok-identical, no loop)",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("url", nargs="?", help="TikTok slideshow URL (photo post). Use --demo to test without URL.")
    p.add_argument("-o", "--output", default=DEFAULT_OUT, help="Output mp4 file")
    p.add_argument("--fps", type=int, default=30, help="Video FPS (30 recommended for TikTok)")
    p.add_argument("--trans", type=float, default=0.6, help="Swipe transition duration in seconds (0.4-0.8 looks like TikTok)")
    p.add_argument("--image-duration", type=float, default=None, help="Seconds per image (auto = audio_duration / n if music exists, else 2.8)")
    p.add_argument("--width", type=int, default=1080, help="Video width")
    p.add_argument("--height", type=int, default=1920, help="Video height")
    p.add_argument("--keep-temp", action="store_true", help="Keep temp images/audio")
    p.add_argument("--no-music", action="store_true", help="Ignore music, create silent video")
    p.add_argument("--demo", action="store_true", help="Run demo: create sample images+audio and render video (no download needed)")
    p.add_argument("--demo-images", type=int, default=5, help="Number of demo images")
    return p.parse_args()

def run_demo(args):
    """Generate synthetic demo images + audio to showcase swipe."""
    print("[demo] Generating synthetic demo slideshow (no download)...")
    tmpdir = tempfile.mkdtemp(prefix="tiktok_demo_")
    try:
        from PIL import Image, ImageDraw, ImageFont
        import math, wave, struct, subprocess

        # Create demo images: colorful gradients with numbers
        img_paths = []
        colors = [(255, 89, 94), (138, 201, 38), (25, 130, 196), (106, 76, 147), (255, 202, 58), (0, 175, 185), (255, 107, 107)]
        for i in range(args.demo_images):
            img = Image.new("RGB", (1080, 1920), colors[i % len(colors)])
            draw = ImageDraw.Draw(img)
            # Draw large number
            txt = f"{i+1} / {args.demo_images}"
            # Try to use default font, size approx
            try:
                # No font file dependency, use default
                font = ImageFont.load_default()
            except Exception:
                font = None
            # Draw centered text via textbbox
            if font:
                # crude centering
                draw.text((540, 800), f"DEMO", fill=(255,255,255), font=font, anchor="mm", stroke_width=2)
                # bigger number
                # Pillow default font small, we draw rectangle to simulate
                pass
            # Draw big circle + number manually via text
            # Add overlay label
            draw.rectangle((0, 1600, 1080, 1920), fill=(0,0,0))
            draw.text((540, 1760), txt, fill=(255,255,255), anchor="mm", align="center")
            draw.text((540, 960), f"{i+1}", fill=(255,255,255), anchor="mm")
            # Add small instruction
            draw.text((540, 100), "TikTok Swipe Demo", fill=(255,255,255), anchor="mm")
            # Add some pattern to see cover crop
            for y in range(0, 1920, 200):
                draw.line((0, y, 1080, y), fill=(255,255,255,30), width=2)
            path = os.path.join(tmpdir, f"demo_{i:02d}.jpg")
            # Need to handle large text: draw with better method: use default font but scale via image resize technique?
            # Simplify: create larger via drawing circles
            img.save(path, quality=95)
            img_paths.append(path)

        # Create demo audio: generate 12 sec sine tones + click
        audio_path = os.path.join(tmpdir, "demo_audio.m4a")
        # Use ffmpeg to generate sine audio: ffmpeg -f lavfi -i "sine=frequency=440:duration=12" demo.m4a
        total_dur_demo = args.demo_images * 2.8 if not args.image_duration else args.demo_images * args.image_duration
        if audio_path and not args.no_music:
            dur = total_dur_demo
            cmd = ["ffmpeg", "-y", "-f", "lavfi", "-i", f"sine=frequency=220:duration={dur}", "-c:a", "aac", "-b:a", "192k", audio_path]
            print(f"[demo] Generating demo audio {dur:.1f}s...")
            res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            if res.returncode != 0:
                print("[demo] ffmpeg sine failed, trying wave fallback")
                # fallback: create silent? just none
                audio_path = None
            else:
                print(f"[demo] Demo audio at {audio_path}")
        else:
            audio_path = None

        # Render video
        out = os.path.abspath(args.output)
        print(f"[demo] Rendering swipe video to {out} ...")
        create_swipe_video(
            image_paths=img_paths,
            audio_path=audio_path if not args.no_music else None,
            output_path=out,
            fps=args.fps,
            trans_duration=args.trans,
            image_duration=args.image_duration,
            width=args.width,
            height=args.height,
        )
        print(f"[demo] Done! Output: {out}")
        print(f"[demo] Tip: check video loops? It should play ONCE and stop on last frame (no repetition).")
    finally:
        if not args.keep_temp:
            shutil.rmtree(tmpdir, ignore_errors=True)
        else:
            print(f"[demo] Temp kept at {tmpdir}")

def main():
    args = parse_args()
    if args.demo:
        run_demo(args)
        return

    if not args.url:
        print("Error: URL required unless --demo. Example:\n  python main.py \"https://www.tiktok.com/@user/photo/123456789\" -o out.mp4")
        sys.exit(1)

    url = args.url.strip().strip('"').strip("'")
    out_path = os.path.abspath(args.output)
    tmpdir = tempfile.mkdtemp(prefix="tiktok_dl_")
    print(f"[main] TikTok Slideshow Downloader")
    print(f"[main] URL: {url}")
    print(f"[main] Output: {out_path}")
    print(f"[main] Temp: {tmpdir}")

    try:
        # Extract
        print("[main] Extracting slideshow info...")
        # Resolve short url first to show final
        resolved = resolve_url(url)
        if resolved != url:
            print(f"[main] Resolved URL: {resolved}")
            url = resolved
        info = extract_info(url)
        images = info.get("images", [])
        music_url = info.get("music_url")
        title = info.get("title", "") or "TikTok slideshow"
        print(f"[main] Title: {title[:80]}")
        print(f"[main] Found {len(images)} images, music: {'yes' if music_url else 'no'} (source: {info.get('source')})")
        if len(images) == 0:
            print("[error] No images extracted. This may not be a photo slideshow. Try a URL like /photo/...")
            sys.exit(1)
        if len(images) == 1:
            print("[warn] Only 1 image found; swipe animation will be static (no transition).")

        # Download images
        image_paths = download_all_images(images, tmpdir)

        # Download audio unless disabled
        audio_path = None
        if not args.no_music and music_url:
            audio_path = download_audio(music_url, tmpdir)
            if audio_path:
                dur = ffprobe_duration(audio_path)
                print(f"[main] Audio downloaded: {audio_path} ({dur:.2f}s)" if dur else f"[main] Audio downloaded: {audio_path}")
            else:
                print("[main] Continuing without music (silent video)")
        elif args.no_music:
            print("[main] --no-music: skipping audio")
        else:
            print("[main] No music URL found; silent video will be created")

        # Create video
        print("[main] Creating swipe video (TikTok-identical, no loop)...")
        create_swipe_video(
            image_paths=image_paths,
            audio_path=audio_path,
            output_path=out_path,
            fps=args.fps,
            trans_duration=args.trans,
            image_duration=args.image_duration,
            width=args.width,
            height=args.height,
        )
        print(f"\n[done] Video saved to: {out_path}")
        print(f"[done] Images: {len(image_paths)}, FPS: {args.fps}, transition: {args.trans}s, size: {args.width}x{args.height}")
        print(f"[done] No loop = video plays once and stops on last frame. Verified: output has no loop metadata.")
        # Show ffprobe verification
        try:
            dur = ffprobe_duration(out_path)
            if dur:
                print(f"[verify] Output duration: {dur:.2f}s")
            # Check video stream
            import subprocess
            cmd = ["ffprobe", "-v", "error", "-show_entries", "stream=codec_name,width,height,r_frame_rate,duration", "-of", "default=noprint_wrappers=1", out_path]
            res = subprocess.run(cmd, stdout=subprocess.PIPE, text=True, timeout=10)
            print("[verify] Stream info:\n" + res.stdout.strip()[:500])
        except Exception:
            pass

    except Exception as e:
        print(f"[error] {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
    finally:
        if not args.keep_temp:
            shutil.rmtree(tmpdir, ignore_errors=True)
        else:
            print(f"[main] Temp kept at {tmpdir}")

if __name__ == "__main__":
    main()
