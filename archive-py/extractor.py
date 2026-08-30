"""
TikTok slideshow extractor
Supports:
 - TikWM public API (primary, no JS challenge)
 - Web scraping via __UNIVERSAL_DATA_FOR_REHYDRATION__ / SIGI_STATE
 - yt-dlp fallback
"""
import re
import json
import time
import random
import urllib.parse
from typing import Optional, Dict, List

import requests

DEFAULT_HEADERS = {
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
    "Referer": "https://www.tiktok.com/",
    "Accept-Language": "en-US,en;q=0.9",
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
}

TIKWM_API = "https://www.tikwm.com/api/"
TIMEOUT = 20

def resolve_url(url: str) -> str:
    """Follow redirects for vm.tiktok.com / vt.tiktok.com short links."""
    try:
        r = requests.head(url, allow_redirects=True, headers=DEFAULT_HEADERS, timeout=10)
        if r.url and r.url != url:
            return r.url
        # some short links need GET
        r = requests.get(url, allow_redirects=True, headers=DEFAULT_HEADERS, timeout=10, stream=True)
        return r.url
    except Exception:
        return url

def extract_tikwm(url: str) -> Optional[Dict]:
    """Try TikWM API. Returns dict with images, music_url etc. or None."""
    try:
        # TikWM expects url in json
        payload = {"url": url, "count": 12, "cursor": 0, "web": 1, "hd": 1}
        headers = {
            **DEFAULT_HEADERS,
            "Content-Type": "application/json",
            "Origin": "https://www.tikwm.com",
            "Referer": "https://www.tikwm.com/",
        }
        resp = requests.post(TIKWM_API, json=payload, headers=headers, timeout=TIMEOUT)
        data = resp.json()
        # debug: print(data)
        if data.get("code") != 0:
            # try alternative payload without count
            return None
        inner = data.get("data") or {}
        images = inner.get("images") or []
        # fallback: data.images may be list of strings or dicts
        # normalize
        clean_images = []
        for img in images:
            if isinstance(img, str):
                clean_images.append(img)
            elif isinstance(img, dict):
                # tikwm sometimes returns dict with url
                u = img.get("url") or img.get("download_url") or img.get("display_url")
                if u:
                    clean_images.append(u)
        # music
        music_url = inner.get("music") or inner.get("music_url") or ""
        if not music_url:
            music_info = inner.get("music_info") or {}
            music_url = music_info.get("play") or music_info.get("url") or ""
        # also check hdplay vs play
        if not music_url and isinstance(inner.get("music_info"), dict):
            music_url = inner["music_info"].get("play")
        # duration maybe
        duration = inner.get("duration") or inner.get("music_info", {}).get("duration") if isinstance(inner.get("music_info"), dict) else None
        # title
        title = inner.get("title") or inner.get("desc") or ""
        author = inner.get("author") or {}
        # if images empty, not a slideshow
        if not clean_images:
            # TikWM returns single video even for slideshow? check vs
            # For slideshow, images present. For video, images empty => not slideshow
            # Return None to signal fallback
            # But we can still indicate no images
            if inner.get("images") is not None and len(clean_images) == 0:
                return None
            # video case: return None so fallback can handle? However we want to support slideshow only
            return None
        return {
            "images": clean_images,
            "music_url": music_url,
            "duration": duration,
            "title": title,
            "author": author,
            "raw": inner,
            "source": "tikwm",
        }
    except Exception as e:
        # print(f"TikWM failed: {e}")
        return None

def _extract_json_from_html(html: str, pattern: str):
    m = re.search(pattern, html, flags=re.DOTALL)
    if not m:
        return None
    txt = m.group(1)
    try:
        return json.loads(txt)
    except Exception:
        # try unescape?
        return None

def extract_via_web(url: str) -> Optional[Dict]:
    """Scrape TikTok web page for slideshow images."""
    try:
        resolved = resolve_url(url)
        # ensure www.tiktok.com url
        headers = {**DEFAULT_HEADERS}
        # add msToken random
        headers["Cookie"] = f"msToken={random.randint(1000000000000000000, 9999999999999999999)}; ttwid=1%7Cfake"
        resp = requests.get(resolved, headers=headers, timeout=TIMEOUT)
        html = resp.text
        # Check challenge
        if "Please wait" in html or "challenge" in html.lower() and "base64" in html.lower():
            # we cannot solve WAF challenge with simple requests; return None to try other method
            return None

        # Try __UNIVERSAL_DATA_FOR_REHYDRATION__
        universal = None
        m = re.search(r'<script[^>]+id="__UNIVERSAL_DATA_FOR_REHYDRATION__"[^>]*>(.*?)</script>', html, flags=re.DOTALL)
        if m:
            try:
                j = json.loads(m.group(1))
                universal = j.get("__DEFAULT_SCOPE__") or j
            except Exception:
                pass
        # Try SIGI_STATE
        sigi = None
        if not universal:
            m2 = re.search(r'<script[^>]+id="SIGI_STATE"[^>]*>(.*?)</script>', html, flags=re.DOTALL)
            if m2:
                try:
                    sigi = json.loads(m2.group(1))
                except Exception:
                    pass
        # Also try script with window._ROUTER etc? fallback search for json containing imagePost
        search_space = universal or sigi
        images = []
        music_url = None
        duration = None
        title = ""

        def traverse_extract(data):
            nonlocal images, music_url, duration, title
            # attempt to walk any dict looking for imagePost
            found_images = []
            found_music = None
            found_duration = None
            found_title = None

            def walk(obj, depth=0):
                nonlocal found_images, found_music, found_duration, found_title
                if depth > 30:
                    return
                if isinstance(obj, dict):
                    # check imagePost keys
                    if "imagePost" in obj and isinstance(obj["imagePost"], dict):
                        imgs = obj["imagePost"].get("images") or []
                        for im in imgs:
                            # im may have imageURL
                            url_list = None
                            if isinstance(im, dict):
                                # new structure: displayImageUrl / imageURL
                                candidates = [
                                    im.get("imageURL", {}).get("urlList"),
                                    im.get("imageURL", {}).get("url_list"),
                                    im.get("displayImage", {}).get("urlList"),
                                    im.get("urlList"),
                                    im.get("url_list"),
                                    [im.get("url")] if im.get("url") else None,
                                ]
                                for cand in candidates:
                                    if cand and isinstance(cand, list) and len(cand) > 0:
                                        url_list = cand
                                        break
                                if url_list:
                                    # pick first highest quality
                                    u = url_list[0]
                                    if isinstance(u, str) and u.startswith("http"):
                                        found_images.append(u)
                                # fallback: directly string
                                if not url_list and im.get("url") and isinstance(im.get("url"), str):
                                    found_images.append(im["url"])
                            elif isinstance(im, str) and im.startswith("http"):
                                found_images.append(im)
                    # check images key that looks like slideshow
                    # sometimes video.imagePost.images alternative
                    if "images" in obj and isinstance(obj["images"], list) and len(obj["images"]) > 0:
                        # heuristic: if first element contains imageURL or urlList and not thumbnails
                        first = obj["images"][0]
                        if isinstance(first, dict) and ("imageURL" in first or "urlList" in first or "url_list" in first):
                            for im in obj["images"]:
                                candidates = [
                                    im.get("imageURL", {}).get("urlList") if isinstance(im.get("imageURL"), dict) else None,
                                    im.get("urlList"),
                                    im.get("url_list"),
                                    [im.get("url")] if im.get("url") else None,
                                ]
                                for cand in candidates:
                                    if cand and isinstance(cand, list):
                                        if len(cand) > 0 and isinstance(cand[0], str) and cand[0].startswith("http"):
                                            found_images.append(cand[0])
                                            break
                                # string fallback
                    # music
                    if not found_music:
                        # try music.playUrl
                        for key in ["playUrl", "play_url", "playAddr", "downloadUrl"]:
                            if key in obj and isinstance(obj[key], str) and obj[key].startswith("http") and "music" in str(obj)[:500]:
                                # heuristic only if nearby music
                                pass
                        # generic music object
                        if "music" in obj and isinstance(obj["music"], dict):
                            mi = obj["music"]
                            for mk in ["playUrl", "play_url", "downloadUrl", "url", "playUrlList"]:
                                v = mi.get(mk)
                                if isinstance(v, str) and v.startswith("http"):
                                    found_music = v
                                    break
                                if isinstance(v, dict):
                                    # urlList
                                    ul = v.get("urlList") or v.get("url_list")
                                    if isinstance(ul, list) and len(ul) > 0:
                                        found_music = ul[0]
                                        break
                                if isinstance(v, list) and len(v) > 0 and isinstance(v[0], str):
                                    found_music = v[0]
                                    break
                            if not found_music:
                                # playUrl inside music
                                pu = mi.get("playUrl")
                                if isinstance(pu, dict):
                                    ul = pu.get("urlList") or pu.get("UrlList")
                                    if isinstance(ul, list) and len(ul) > 0:
                                        found_music = ul[0]
                        # also check authorInfo.music
                    # title
                    if not found_title and "desc" in obj and isinstance(obj["desc"], str) and len(obj["desc"]) > 0:
                        # prefer longer desc
                        if not found_title or len(obj["desc"]) > len(found_title):
                            found_title = obj["desc"]
                    # duration
                    if not found_duration and "duration" in obj and isinstance(obj["duration"], (int, float)):
                        # only if seems music duration
                        # keep last
                        found_duration = obj["duration"]
                    for v in obj.values():
                        walk(v, depth + 1)
                elif isinstance(obj, list):
                    for item in obj:
                        walk(item, depth + 1)

            walk(data)
            images.extend(found_images)
            if not music_url and found_music:
                music_url = found_music
            if not title and found_title:
                title = found_title
            if not duration and found_duration:
                duration = found_duration

        if universal:
            # universal structure: __DEFAULT_SCOPE__['webapp.video-detail'] etc
            # try direct paths
            wd = None
            if isinstance(universal, dict):
                # find nested video-detail
                for k, v in universal.items():
                    if "video-detail" in k:
                        wd = v
                        break
                if not wd and "webapp.video-detail" in universal:
                    wd = universal["webapp.video-detail"]
            if wd:
                traverse_extract(wd)
                # also try direct extraction via known paths
                # path: webapp.video-detail.itemInfo.itemStruct
                item = None
                if isinstance(wd, dict):
                    item = wd.get("itemInfo", {}).get("itemStruct")
                    if not item:
                        item = wd.get("itemStruct")
                if item:
                    traverse_extract(item)
            traverse_extract(universal)

        if sigi:
            # SIGI_STATE: keys are ItemModule, ItemDetail etc
            # ItemModule[videoId] contains imagePost
            item_module = sigi.get("ItemModule") or {}
            for _, val in item_module.items():
                traverse_extract(val)
            traverse_extract(sigi)

        # regex fallback: search html for imageURL urlList patterns
        if not images:
            # find all occurrences of "urlList":["https://..."]
            # look for image specific
            # greedy search for images array
            # pattern: "imagePost":{"images":[{"imageURL":{"urlList":["URL"]}}]}
            # simple regex for urlList containing tiktok image cdn
            urls = re.findall(r'"urlList"\s*:\s*\[(.*?)\]', html, flags=re.DOTALL)
            # filter for image urls (containing .jpeg .jpg .webp)
            cand_images = []
            for block in urls:
                # extract quoted strings
                for u in re.findall(r'"(https:[^"]+)"', block):
                    # heuristics: image cdn domains are *.tiktokcdn.com or *.muscdn.com or bytes
                    # keep only those that look like image (contains ~ or .jpeg)
                    if any(ext in u for ext in [".jpeg", ".jpg", ".webp", ".png", "image", "obj/tos"]):
                        # exclude video urls (contains /video/ and .mp4)
                        if ".mp4" in u or "playAddr" in u:
                            continue
                        # unescape
                        u = u.replace("\\u002F", "/")
                        cand_images.append(u)
            # deduplicate preserving order
            seen = set()
            uniq = []
            for u in cand_images:
                if u not in seen:
                    seen.add(u)
                    uniq.append(u)
            # if we have many, take those that are likely slideshow (need at least 2 images)
            if len(uniq) >= 2 and len(uniq) < 50:
                # Heuristic: slideshow images are similar count 2-35
                # Use these if we didn't get structured images
                if len(images) == 0:
                    images = uniq[:35]

        # clean music url if proto relative
        if music_url and music_url.startswith("//"):
            music_url = "https:" + music_url
        # also try regex for music playUrl if still none
        if not music_url:
            m_music = re.search(r'"playUrl"\s*:\s*"(https:[^"]+)"', html)
            if m_music:
                music_url = m_music.group(1).replace("\\u002F", "/")
            else:
                # search for '"music":... "playUrl":'
                m2 = re.search(r'"music"[^}]*"playUrl"\s*:\s*\{[^}]*"urlList"\s*:\s*\["(https:[^"]+)"', html)
                if m2:
                    music_url = m2.group(1).replace("\\u002F", "/")

        # Fix music url escaping
        if music_url:
            music_url = music_url.replace("\\u002F", "/")

        # deduplicate images
        seen = set()
        uniq_images = []
        for u in images:
            if u not in seen:
                seen.add(u)
                uniq_images.append(u.replace("\\u002F", "/"))

        if uniq_images:
            return {
                "images": uniq_images,
                "music_url": music_url,
                "duration": duration,
                "title": title,
                "source": "web",
                "raw": {"universal": universal, "sigi": sigi},
            }
        return None
    except Exception as e:
        # import traceback; traceback.print_exc()
        return None

def extract_via_ytdlp(url: str) -> Optional[Dict]:
    try:
        import yt_dlp  # type: ignore
    except ImportError:
        return None
    try:
        ydl_opts = {
            "quiet": True,
            "no_warnings": True,
            "skip_download": True,
            "allow_unplayable_formats": True,
        }
        with yt_dlp.YoutubeDL(ydl_opts) as ydl:
            info = ydl.extract_info(url, download=False)
        # info may contain thumbnails vs formats etc.
        # For slideshow, ydl may return formats with audio only and not images.
        # We attempt to pull images from info if present under 'thumbnails' ? Not enough.
        # So ytdlp fallback mainly for music and for triggering challenge bypass.
        # But we can also try to fetch webpage via ytdlp internal extractor that solves challenge.
        # For simplicity, return None if not slideshow? We'll try to extract images via ytdlp's webpage data if ytdlp solved challenge.
        # Check if info has 'entries' etc.
        images = []
        # try to find imagePost via info internal?
        # Look for 'formats' that are images? yt-dlp doesn't expose images.
        # fallback: use ytdlp to get webpage html solved
        # Use TikTokBaseIE internal? Instead we can try to use downloader's webpage method via yt_dlp.extractor.tiktok
        try:
            from yt_dlp.extractor.tiktok import TikTokBaseIE
            # Instantiate extractor to reuse challenge solver
            # But easier: try to call ydl's extractor directly? We'll just return music if present
            # Instead we fallback to web extractor after ytdlp cookies are set
            pass
        except Exception:
            pass

        music_url = None
        duration = info.get("duration")
        title = info.get("title") or info.get("description") or ""
        # try formats to get audio url
        for f in info.get("formats") or []:
            if f.get("vcodec") == "none" and f.get("acodec") != "none":
                music_url = f.get("url")
                break
        if not music_url:
            # try thumbnail? not music
            pass
        if images:
            return {"images": images, "music_url": music_url, "duration": duration, "title": title, "source": "ytdlp"}
        # If we got music but no images, still not useful for slideshow; return None
        return None
    except Exception as e:
        return None

def extract_info(url: str) -> Dict:
    """
    Unified extractor chain. Returns dict:
      images: List[str]
      music_url: Optional[str]
      title: str
      duration: Optional[int/float]
      source: str
    Raises ValueError if extraction failed.
    """
    original_url = url.strip()
    if not original_url.startswith("http"):
        raise ValueError("URL must start with http/https")
    # First try tikwm (most reliable for slideshow)
    res = extract_tikwm(original_url)
    if res and res.get("images"):
        # Validate images count >=1
        if len(res["images"]) >= 1:
            return res

    # Second: try web scraping (after resolving)
    res2 = extract_via_web(original_url)
    if res2 and res2.get("images"):
        return res2

    # Third: yt-dlp solver then retry web
    res3 = extract_via_ytdlp(original_url)
    if res3 and res3.get("images"):
        return res3
    # As last attempt, try web again with yt-dlp cookies set (maybe solver set cookies)
    # If still no images, raise error with guidance
    # Provide debug fallback: try tikwm again with resolved url
    resolved = resolve_url(original_url)
    if resolved != original_url:
        res_retry = extract_tikwm(resolved)
        if res_retry and res_retry.get("images"):
            return res_retry
        res_retry2 = extract_via_web(resolved)
        if res_retry2 and res_retry2.get("images"):
            return res_retry2

    raise ValueError(
        "Failed to extract TikTok slideshow. Reasons:\n"
        "- URL may not be a photo slideshow (image post). Video URLs are not supported by this tool.\n"
        "- TikTok blocked extraction (WAF challenge). Try again in a minute or use a different network.\n"
        "- TikWM API may be temporarily down. Try again later.\n"
        f"URL: {original_url}\n"
        "Tip: Ensure URL is full TikTok photo URL like https://www.tiktok.com/@user/photo/123... or vm.tiktok.com/... short link."
    )

def extract_images_and_music(url: str):
    """Convenience wrapper returning images, music_url, meta"""
    info = extract_info(url)
    return info["images"], info.get("music_url"), info
