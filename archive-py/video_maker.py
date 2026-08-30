"""
Video maker with TikTok-identical swipe animation.

TikTok photo mode swipe spec (reverse engineered):
- Portrait 1080x1920 black background
- Each image is cover-cropped to fill full 9:16
- Static hold, then swipe transition ~0.6s
- During swipe: outgoing slides left (-travel), incoming slides from right (travel -> 0)
- Easing: cubic ease-out (TikTok uses easeOutCubic / cubic-bezier 0.4,0,0.2,1 approx)
- Gap between cards: 24px black visible during transition
- Rounded corners: 32px radius during movement, square when static (our implementation)
- No loop: video ends on last image, holds till audio ends

All durations computed to match audio perfectly, no repetition.
"""

import os
import math
import subprocess
import tempfile
import shutil
from pathlib import Path
from typing import List, Optional

import numpy as np
from PIL import Image, ImageDraw, ImageFilter
import imageio.v2 as imageio

try:
    import imageio_ffmpeg  # noqa
    HAS_IMAGEIO_FFMPEG = True
except Exception:
    HAS_IMAGEIO_FFMPEG = False

W = 1080
H = 1920
FPS_DEFAULT = 30
TRANS_DURATION = 0.6  # seconds, TikTok default ~0.5-0.65
GAP = 32  # black gap during swipe
RADIUS = 32  # rounded corner radius during transition
BG_COLOR = (0, 0, 0)

def ease_out_cubic(t: float) -> float:
    # 1 - (1-t)^3  snappy start, smooth end like TikTok
    return 1 - pow(1 - t, 3)

def ease_in_out_cubic(t: float) -> float:
    if t < 0.5:
        return 4 * t * t * t
    return 1 - pow(-2 * t + 2, 3) / 2

def ease_tiktok(t: float) -> float:
    # cubic-bezier approx 0.25, 0.46, 0.45, 0.94 -> close to easeOutQuad
    # we use easeOutCubic which is very close to real TikTok
    return ease_out_cubic(t)

def ffprobe_duration(path: str) -> Optional[float]:
    try:
        result = subprocess.run(
            ["ffprobe", "-v", "error", "-show_entries", "format=duration",
             "-of", "default=noprint_wrappers=1:nokey=1", path],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=10
        )
        if result.returncode == 0:
            return float(result.stdout.strip())
    except Exception:
        pass
    return None

def prepare_cover(image_path: str, width=W, height=H) -> Image.Image:
    img = Image.open(image_path).convert("RGB")
    iw, ih = img.size
    if iw == 0 or ih == 0:
        # fallback black
        return Image.new("RGB", (width, height), BG_COLOR)
    scale = max(width / iw, height / ih)
    new_w = int(round(iw * scale))
    new_h = int(round(ih * scale))
    # LANCZOS for quality
    img = img.resize((new_w, new_h), Image.LANCZOS)
    left = (new_w - width) // 2
    top = (new_h - height) // 2
    # ensure within bounds
    left = max(0, left)
    top = max(0, top)
    img = img.crop((left, top, left + width, top + height))
    # ensure exact size (crop may be off by 1 due to rounding)
    if img.size != (width, height):
        img = img.resize((width, height), Image.LANCZOS)
    return img

def rounded_image(img: Image.Image, radius: int = RADIUS) -> Image.Image:
    """Return RGBA image with rounded corners."""
    w, h = img.size
    # Create mask
    mask = Image.new("L", (w, h), 0)
    draw = ImageDraw.Draw(mask)
    # Use rounded_rectangle
    draw.rounded_rectangle((0, 0, w, h), radius=radius, fill=255)
    # Apply slight anti-alias via filter? Pillow already anti-aliases
    rgba = img.convert("RGBA")
    rgba.putalpha(mask)
    return rgba

def make_silent_video(
    image_paths: List[str],
    output_path: str,
    fps: int = FPS_DEFAULT,
    trans_duration: float = TRANS_DURATION,
    image_duration: Optional[float] = None,
    total_duration: Optional[float] = None,
    width: int = W,
    height: int = H,
    gap: int = GAP,
    radius: int = RADIUS,
):
    """
    Create silent video with swipe.
    One of image_duration or total_duration must be provided or will be auto.
    - If total_duration provided (e.g., audio duration), image_duration = total_duration / n
    - Else image_duration defaults to 3.0
    """
    n = len(image_paths)
    if n == 0:
        raise ValueError("No images provided")
    if n == 1:
        trans_duration = 0  # no transition needed

    if total_duration is not None:
        image_duration = total_duration / n
    if image_duration is None:
        image_duration = 3.0  # default portrait hold

    # clamp transition
    max_trans = image_duration * 0.8
    if trans_duration > max_trans:
        trans_duration = max_trans
        if trans_duration < 0.15:
            trans_duration = 0.15
    if trans_duration < 0:
        trans_duration = 0

    total_duration = n * image_duration
    total_frames = int(round(total_duration * fps))

    # Preprocess covers
    print(f"[video_maker] Preparing {n} images (cover {width}x{height})...")
    covers = []
    rounded_covers = []
    for p in image_paths:
        cov = prepare_cover(p, width, height)
        covers.append(cov)
        rounded_covers.append(rounded_image(cov, radius))

    travel = width + gap  # distance to travel

    # Use imageio writer for silent video
    # macro_block_size=1 avoids resize warnings for 1080x1920 (both divisible by 16? 1080 not divisible by 16, but macro_block_size=1 allows)
    print(f"[video_maker] Generating {total_frames} frames @ {fps}fps (~{total_duration:.2f}s total, {image_duration:.2f}s per image, {trans_duration:.2f}s swipe)")
    # Ensure parent dir exists
    os.makedirs(os.path.dirname(os.path.abspath(output_path)) or ".", exist_ok=True)

    # Write via imageio
    # imageio-ffmpeg will auto download ffmpeg if needed, but we have system ffmpeg
    # codec params: libx264, yuv420p, crf 18 quality high
    writer = imageio.get_writer(
        output_path,
        fps=fps,
        codec="libx264",
        quality=7,  # 0-10, 7 high
        pixelformat="yuv420p",
        macro_block_size=1,
        ffmpeg_params=["-crf", "18", "-preset", "medium"],
    )

    # For efficiency, pre-create black background numpy for static?
    # But we need PIL compositing for transitions.

    # Create loop
    # Use tqdm if available
    try:
        from tqdm import tqdm
        iterator = tqdm(range(total_frames), desc="Rendering swipe", unit="frame")
    except Exception:
        iterator = range(total_frames)

    for frame_idx in iterator:
        t = frame_idx / fps
        # Determine which image(s) to show
        idx = int(t // image_duration) if image_duration > 0 else 0
        if idx >= n:
            idx = n - 1
        residual = t - idx * image_duration

        # Last image: always static
        if idx == n - 1:
            # static
            frame_img = covers[idx]
            writer.append_data(np.array(frame_img))
            continue

        static_end = image_duration - trans_duration
        if residual < static_end or trans_duration <= 0.001:
            # static period
            frame_img = covers[idx]
            writer.append_data(np.array(frame_img))
        else:
            # transition between idx and idx+1
            p = (residual - static_end) / trans_duration  # 0..1
            p = max(0.0, min(1.0, p))
            eased = ease_tiktok(p)
            offset = int(round(eased * travel))

            out_x = -offset
            in_x = travel - offset

            # Composite
            # Background RGBA black
            canvas = Image.new("RGBA", (width, height), (0, 0, 0, 255))
            # Paste outgoing (rounded) at out_x
            # Need to handle clipping: PIL paste handles negative coords via cropping automatically? Actually paste with box and mask handles clipping, but we must ensure image fully covers.
            # Create temporary helper: paste with offset
            # For negative out_x, part offscreen left; for in_x positive, part offscreen right.
            # PIL's paste with a 4-tuple box does NOT clip automatically for negative; but 2-tuple does with some handling? We'll use paste with (x,y) and mask.
            # It will clip correctly if x negative: Pillow will put image such that (0,0) of image at x; negative means left part not visible but paste will still try? Need to test.
            # Safer: create compositing via alpha_composite after positioning on larger canvas?
            # Easiest: use canvas.paste with mask and position, Pillow internally clips.
            canvas.paste(rounded_covers[idx], (out_x, 0), rounded_covers[idx])
            canvas.paste(rounded_covers[idx + 1], (in_x, 0), rounded_covers[idx + 1])

            # Optional: add subtle shadow / vignette between gap? The gap is black already, but we can add linear dark edge
            # TikTok shows gap as black with no extra. Keep simple.

            # Also add slight scale? TikTok cards slightly scale? Option: scale down outgoing to 0.98 at end? We skip.

            # Add thin divider line during transition for realism? No.

            # Convert to RGB
            rgb = Image.new("RGB", (width, height), BG_COLOR)
            # alpha composite: paste canvas onto rgb using alpha
            # Since canvas is RGBA with rounded corners transparent, we need to composite onto rgb
            # Use Image.alpha_composite requires both RGBA
            base = Image.new("RGBA", (width, height), (0, 0, 0, 255))
            # base is black opaque
            # Actually canvas is already our composite; we can just composite canvas onto base? Simpler: canvas already on black? We pasted onto transparent black? Let's instead composite directly:
            # We already did canvas = black RGBA, pasted rounded images; rounded corners transparency will show black background, which is desired.
            # Convert canvas RGBA to RGB: need to remove alpha, blend with black.
            # canvas has alpha 255 for opaque parts and <255 for rounded corners edge, and 0 for transparent gap? Gap area is black opaque (alpha 255). For rounded corners, transparent shows black.
            # To properly convert, we can just do canvas.convert("RGB") which blends alpha? But convert discards alpha without blending. Better to composite onto RGB background.
            rgb_canvas = Image.new("RGB", (width, height), BG_COLOR)
            # Paste using mask
            # Since canvas is RGBA, we can extract its alpha? Instead we should paste via alpha composite method:
            # Create final RGBA after pasting, then composite onto RGB.
            # Actually canvas already is RGBA with black background, so its alpha is 255 everywhere except rounded corners outer edge (but we pasted black background as opaque, so still 255). The rounded corners outer pixels are still black from background, not transparent. Wait our canvas initial is black opaque RGBA (0,0,0,255). Pasting rounded image with its alpha will keep background black where transparent.
            # So canvas is fully opaque. Converting to RGB is fine with .convert("RGB")
            # However paste with negative offsets may leave gap transparent? But gap is just background black which stays.
            rgb_frame = canvas.convert("RGB")
            writer.append_data(np.array(rgb_frame))

    writer.close()
    print(f"[video_maker] Silent video written: {output_path} ({total_duration:.2f}s)")
    return total_duration, image_duration, trans_duration

def mux_audio(silent_path: str, audio_path: str, output_path: str, shortest: bool = True):
    """Mux audio onto silent video using ffmpeg."""
    if not os.path.exists(audio_path):
        raise FileNotFoundError(f"Audio not found: {audio_path}")
    if not os.path.exists(silent_path):
        raise FileNotFoundError(f"Silent video not found: {silent_path}")

    # Ensure output dir
    os.makedirs(os.path.dirname(os.path.abspath(output_path)) or ".", exist_ok=True)

    # Probe durations to decide shortest vs pad
    # Use ffmpeg to mux copy video and encode audio to aac if needed
    # If audio is mp3/m4a, we can just copy or aac
    cmd = [
        "ffmpeg", "-y",
        "-i", silent_path,
        "-i", audio_path,
        "-map", "0:v:0",
        "-map", "1:a:0",
        "-c:v", "copy",
        # ensure audio compatible with mp4: aac
        "-c:a", "aac", "-b:a", "192k",
    ]
    if shortest:
        cmd += ["-shortest"]
    # Avoid extra frames
    cmd += ["-movflags", "+faststart", output_path]

    print(f"[mux] Muxing audio: {' '.join(cmd)}")
    result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if result.returncode != 0:
        print("[mux] ffmpeg stderr:", result.stderr[:2000])
        raise RuntimeError(f"ffmpeg mux failed: {result.stderr[:500]}")
    print(f"[mux] Final video: {output_path}")
    # cleanup silent? caller decides
    return output_path

def create_swipe_video(
    image_paths: List[str],
    audio_path: Optional[str],
    output_path: str,
    fps: int = FPS_DEFAULT,
    trans_duration: float = TRANS_DURATION,
    image_duration: Optional[float] = None,
    width: int = W,
    height: int = H,
    gap: int = GAP,
    radius: int = RADIUS,
    keep_silent: bool = False,
):
    """
    High level: create swipe video from images + optional audio.
    Returns output_path.
    """
    audio_duration = None
    if audio_path and os.path.exists(audio_path):
        audio_duration = ffprobe_duration(audio_path)
        if audio_duration:
            print(f"[create] Audio duration: {audio_duration:.2f}s")
        else:
            # try to get from extractor info later? fallback use file size estimate? else None
            print("[create] Unable to probe audio duration, using image_duration fallback")
    else:
        if audio_path:
            print(f"[create] Audio path missing: {audio_path}, proceeding silent")
            audio_path = None

    # Determine total/image duration
    if audio_duration:
        total_dur = audio_duration
    else:
        total_dur = None
        if image_duration is None:
            image_duration = 2.8  # nice TikTok pace when no music

    # Temporary silent file
    tmp_silent = None
    if audio_path:
        # Use temp file in same dir for silent
        tmp_dir = tempfile.mkdtemp(prefix="tiktok_swipe_")
        tmp_silent = os.path.join(tmp_dir, "silent.mp4")
    else:
        # No audio, output is silent itself
        tmp_silent = output_path

    total_used, img_dur, trans = make_silent_video(
        image_paths=image_paths,
        output_path=tmp_silent,
        fps=fps,
        trans_duration=trans_duration,
        image_duration=image_duration,
        total_duration=total_dur,
        width=width,
        height=height,
        gap=gap,
        radius=radius,
    )

    if audio_path and tmp_silent != output_path:
        try:
            mux_audio(tmp_silent, audio_path, output_path, shortest=True)
        finally:
            if not keep_silent:
                # cleanup temp
                try:
                    shutil.rmtree(os.path.dirname(tmp_silent))
                except Exception:
                    pass
                try:
                    os.remove(tmp_silent)
                except Exception:
                    pass
    else:
        # Already written to output_path
        if tmp_silent != output_path:
            shutil.move(tmp_silent, output_path)
        print(f"[create] Silent video (no audio) at {output_path}")

    # Verify no loop: ensure video not looping metadata? mp4 doesn't loop by default
    return output_path

# CLI helper for standalone testing
if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="TikTok swipe video maker (standalone)")
    parser.add_argument("images", nargs="+", help="Image paths")
    parser.add_argument("--audio", help="Audio path")
    parser.add_argument("--output", default="output.mp4")
    parser.add_argument("--fps", type=int, default=30)
    parser.add_argument("--trans", type=float, default=0.6)
    args = parser.parse_args()
    create_swipe_video(args.images, args.audio, args.output, fps=args.fps, trans_duration=args.trans)
