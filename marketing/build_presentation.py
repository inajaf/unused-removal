from __future__ import annotations

import math
import shutil
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont


ROOT = Path(__file__).resolve().parent
STATES = ROOT / "demo_states"
FRAMES = ROOT / ".frames"
DIST = ROOT / "dist"
W, H = 960, 600

BG = (8, 11, 18)
PANEL = (17, 22, 33)
LINE = (43, 50, 67)
TEXT = (244, 246, 252)
MUTED = (151, 162, 185)
PURPLE = (124, 105, 255)
GREEN = (30, 205, 139)
AMBER = (245, 180, 51)


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    name = "seguisb.ttf" if bold else "segoeui.ttf"
    return ImageFont.truetype(str(Path("C:/Windows/Fonts") / name), size)


def rounded(draw: ImageDraw.ImageDraw, box, radius, fill, outline=None, width=1):
    draw.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=width)


def fit_crop(image: Image.Image, size: tuple[int, int], anchor: float = 0.0) -> Image.Image:
    target_w, target_h = size
    scale = max(target_w / image.width, target_h / image.height)
    resized = image.resize((round(image.width * scale), round(image.height * scale)), Image.Resampling.LANCZOS)
    left = max(0, (resized.width - target_w) // 2)
    overflow = max(0, resized.height - target_h)
    top = round(overflow * anchor)
    return resized.crop((left, top, left + target_w, top + target_h))


def background() -> Image.Image:
    image = Image.new("RGB", (W, H), BG)
    glow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gd.ellipse((550, -290, 1130, 290), fill=(111, 86, 255, 75))
    gd.ellipse((-260, 360, 330, 940), fill=(25, 183, 140, 28))
    glow = glow.filter(ImageFilter.GaussianBlur(90))
    image = Image.alpha_composite(image.convert("RGBA"), glow)
    return image.convert("RGB")


def windows_screen(state: Image.Image, callout: str | None = None, accent=PURPLE, anchor=0.0) -> Image.Image:
    image = Image.new("RGB", (W, H), BG)
    body = fit_crop(state.convert("RGB"), (W, H - 31), anchor)
    image.paste(body, (0, 31))
    draw = ImageDraw.Draw(image, "RGBA")
    draw.rectangle((0, 0, W, 31), fill=(20, 23, 31, 255))
    draw.line((0, 30, W, 30), fill=(51, 56, 70, 255), width=1)
    rounded(draw, (11, 8, 26, 23), 4, (*PURPLE, 255))
    draw.ellipse((15, 12, 22, 19), outline=(255, 255, 255, 240), width=1)
    draw.text((34, 7), "unused-removal", font=font(12, True), fill=(230, 233, 242, 255))
    draw.text((141, 8), "Windows desktop", font=font(11), fill=(129, 139, 161, 255))
    draw.line((856, 15, 868, 15), fill=(170, 176, 190, 255), width=1)
    draw.rectangle((891, 10, 901, 20), outline=(170, 176, 190, 255), width=1)
    draw.line((928, 10, 940, 21), fill=(170, 176, 190, 255), width=1)
    draw.line((940, 10, 928, 21), fill=(170, 176, 190, 255), width=1)
    if callout:
        label_font = font(16, True)
        bbox = draw.textbbox((0, 0), callout, font=label_font)
        width = bbox[2] - bbox[0] + 42
        x, y = 28, 49
        rounded(draw, (x, y, x + width, y + 42), 13, (9, 12, 20, 224), (*accent, 210), 1)
        draw.ellipse((x + 15, y + 16, x + 25, y + 26), fill=(*accent, 255))
        draw.text((x + 31, y + 10), callout, font=label_font, fill=TEXT)
    return image


def wrap(draw, text: str, fnt, max_width: int) -> list[str]:
    words = text.split()
    lines: list[str] = []
    line = ""
    for word in words:
        candidate = f"{line} {word}".strip()
        if draw.textlength(candidate, font=fnt) <= max_width:
            line = candidate
        else:
            if line:
                lines.append(line)
            line = word
    if line:
        lines.append(line)
    return lines


def intro_slide(home: Image.Image) -> Image.Image:
    image = background()
    draw = ImageDraw.Draw(image, "RGBA")
    draw.text((54, 46), "UNUSED / REMOVAL", font=font(13, True), fill=PURPLE)
    draw.text((54, 88), "Your disk,", font=font(52, True), fill=TEXT)
    draw.text((54, 145), "explained.", font=font(52, True), fill=TEXT)
    draw.text((56, 220), "A focused desktop app that finds large files,", font=font(18), fill=MUTED)
    draw.text((56, 247), "labels risk, and leaves the final decision to you.", font=font(18), fill=MUTED)

    for index, (label, color) in enumerate((("Large files", PURPLE), ("Protected paths", (244, 91, 120)), ("Recycle Bin", GREEN))):
        x = 56 + index * 135
        rounded(draw, (x, 295, x + 124, 329), 11, (18, 23, 34, 240), (*color, 150), 1)
        draw.ellipse((x + 12, 308, x + 20, 316), fill=(*color, 255))
        draw.text((x + 27, 303), label, font=font(12, True), fill=(220, 225, 237, 255))

    preview = fit_crop(home.convert("RGB"), (465, 291), 0.05)
    shadow = Image.new("RGBA", (510, 346), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shadow)
    sd.rounded_rectangle((18, 20, 492, 329), radius=20, fill=(0, 0, 0, 190))
    shadow = shadow.filter(ImageFilter.GaussianBlur(16))
    image.paste(shadow, (438, 120), shadow)
    draw = ImageDraw.Draw(image, "RGBA")
    rounded(draw, (456, 139, 938, 465), 17, (20, 23, 31, 255), (62, 70, 91, 255), 1)
    draw.rectangle((457, 169, 937, 464), fill=(8, 11, 18, 255))
    image.paste(preview, (465, 169))
    draw.ellipse((473, 151, 481, 159), fill=(255, 95, 86, 255))
    draw.ellipse((487, 151, 495, 159), fill=(255, 190, 66, 255))
    draw.ellipse((501, 151, 509, 159), fill=(43, 199, 111, 255))
    draw.text((54, 525), "WINDOWS 11  /  NATIVE DESKTOP  /  NO LOCALHOST PORT", font=font(13, True), fill=(184, 190, 207, 255))
    return image


def comparison_slide() -> Image.Image:
    image = background()
    draw = ImageDraw.Draw(image, "RGBA")
    draw.text((48, 34), "WHY IT STANDS OUT", font=font(13, True), fill=PURPLE)
    draw.text((48, 62), "One guided review, not four separate tools.", font=font(32, True), fill=TEXT)
    draw.text((49, 108), "Positioning based on official product feature pages · August 2026", font=font(13), fill=MUTED)

    columns = [
        ("unused-removal", "Guided cleanup", "Large files + risk labels", "Safe / Review / Protected", "Windows · macOS · Linux", PURPLE),
        ("WinDirStat", "Disk analysis", "Treemap + file lists", "File operations", "Windows", (98, 174, 255)),
        ("TreeSize", "Storage analysis", "Treemap + reporting", "Edition-dependent tools", "Windows", (240, 177, 63)),
        ("BleachBit", "Rule-based cleaner", "Junk + privacy cleaning", "Cleaner rules", "Win/Linux · Mac experimental", (57, 201, 133)),
    ]
    card_w, gap, y = 207, 12, 155
    for i, (name, workflow, focus, safety, platforms, color) in enumerate(columns):
        x = 48 + i * (card_w + gap)
        selected = i == 0
        rounded(draw, (x, y, x + card_w, 486), 17, (20, 25, 37, 245) if selected else (14, 18, 28, 230), (*color, 235 if selected else 90), 2 if selected else 1)
        if selected:
            rounded(draw, (x + 14, y + 14, x + 84, y + 38), 8, (*PURPLE, 35), (*PURPLE, 150), 1)
            draw.text((x + 25, y + 19), "THIS APP", font=font(10, True), fill=(*PURPLE, 255))
            name_y = y + 56
        else:
            name_y = y + 25
        draw.text((x + 16, name_y), name, font=font(19, True), fill=TEXT)
        draw.line((x + 16, name_y + 34, x + card_w - 16, name_y + 34), fill=(*color, 110), width=1)
        rows = (("PRIMARY", workflow), ("FINDS", focus), ("SAFETY MODEL", safety), ("PLATFORMS", platforms))
        row_y = name_y + 52
        for label, value in rows:
            draw.text((x + 16, row_y), label, font=font(9, True), fill=(*color, 230))
            value_font = font(12, selected)
            for line in wrap(draw, value, value_font, card_w - 32)[:2]:
                row_y += 17
                draw.text((x + 16, row_y), line, font=value_font, fill=(217, 222, 234, 255) if selected else (171, 181, 201, 255))
            row_y += 22

    rounded(draw, (48, 516, 912, 566), 14, (25, 24, 49, 240), (*PURPLE, 140), 1)
    draw.ellipse((69, 535, 80, 546), fill=(*GREEN, 255))
    draw.text((91, 529), "Scan → explain → review → Recycle Bin", font=font(18, True), fill=TEXT)
    draw.text((555, 532), "inside one desktop flow", font=font(15), fill=MUTED)
    return image


def outro_slide() -> Image.Image:
    image = background()
    draw = ImageDraw.Draw(image, "RGBA")
    rounded(draw, (421, 82, 539, 200), 31, (113, 91, 242, 255), (164, 151, 255, 230), 2)
    draw.ellipse((446, 107, 506, 167), outline=(255, 255, 255, 255), width=7)
    draw.line((497, 158, 522, 183), fill=(255, 255, 255, 255), width=7)
    draw.line((476, 121, 476, 153), fill=(255, 255, 255, 255), width=5)
    draw.line((461, 137, 491, 137), fill=(255, 255, 255, 255), width=5)
    title = "Find space. Understand it."
    sub = "Reclaim it safely."
    tw = draw.textlength(title, font=font(35, True))
    sw = draw.textlength(sub, font=font(35, True))
    draw.text(((W - tw) / 2, 252), title, font=font(35, True), fill=TEXT)
    draw.text(((W - sw) / 2, 300), sub, font=font(35, True), fill=(*PURPLE, 255))
    draw.text((314, 368), "Large files visible. System files protected.", font=font(16), fill=MUTED)
    rounded(draw, (340, 421, 620, 472), 15, (*PURPLE, 255), None)
    draw.text((393, 435), "unused-removal for desktop", font=font(17, True), fill=(255, 255, 255, 255))
    draw.text((346, 526), "WINDOWS  ·  macOS  ·  LINUX", font=font(13, True), fill=(176, 183, 202, 255))
    return image


def blend(a: Image.Image, b: Image.Image, amount: float) -> Image.Image:
    return Image.blend(a.convert("RGB"), b.convert("RGB"), amount)


def cursor(frame: Image.Image, x: float, y: float, pressed: bool = False) -> Image.Image:
    image = frame.copy()
    draw = ImageDraw.Draw(image, "RGBA")
    if pressed:
        draw.ellipse((x - 18, y - 18, x + 18, y + 18), outline=(*PURPLE, 125), width=4)
    points = [(x, y), (x + 2, y + 23), (x + 8, y + 17), (x + 15, y + 29), (x + 21, y + 26), (x + 14, y + 14), (x + 24, y + 12)]
    draw.polygon(points, fill=(255, 255, 255, 255), outline=(8, 11, 18, 255))
    return image


def append_hold(frames: list[Image.Image], image: Image.Image, count: int):
    frames.extend(image.copy() for _ in range(count))


def append_transition(frames: list[Image.Image], a: Image.Image, b: Image.Image, count: int = 4):
    for i in range(1, count + 1):
        frames.append(blend(a, b, i / (count + 1)))


def main():
    DIST.mkdir(parents=True, exist_ok=True)
    if FRAMES.exists():
        shutil.rmtree(FRAMES)
    FRAMES.mkdir(parents=True)

    state_images = [Image.open(STATES / f"0{i}-{name}.png") for i, name in ((1, "home"), (2, "progress"), (3, "results"), (4, "details"))]
    intro = intro_slide(state_images[0])
    home = windows_screen(state_images[0], "Choose the scan. You keep the decision.")
    progress = windows_screen(state_images[1], "Live progress through scan and analysis", GREEN, 0.15)
    results = windows_screen(state_images[2], "Large files first. Risk before deletion.", AMBER)
    details = windows_screen(state_images[3], "Protected files stay visible — and locked.", (244, 91, 120))
    comparison = comparison_slide()
    outro = outro_slide()

    comparison.save(DIST / "unused-removal-windows-comparison.png", optimize=True)

    frames: list[Image.Image] = []
    append_hold(frames, intro, 13)
    append_transition(frames, intro, home)
    for i in range(10):
        t = i / 9
        x = 820 - 330 * (1 - math.cos(t * math.pi)) / 2
        y = 505 - 70 * (1 - math.cos(t * math.pi)) / 2
        frames.append(cursor(home, x, y, pressed=i >= 8))
    append_transition(frames, home, progress)
    append_hold(frames, progress, 12)
    append_transition(frames, progress, results)
    append_hold(frames, results, 13)
    append_transition(frames, results, details)
    append_hold(frames, details, 13)
    append_transition(frames, details, comparison)
    append_hold(frames, comparison, 22)
    append_transition(frames, comparison, outro)
    append_hold(frames, outro, 16)

    for index, frame in enumerate(frames):
        frame.save(FRAMES / f"frame_{index:03d}.png", optimize=True)
    print(f"Generated {len(frames)} frames in {FRAMES}")


if __name__ == "__main__":
    main()
