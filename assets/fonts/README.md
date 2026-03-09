# Bundled fonts

Place `.ttf`, `.otf`, and `.ttc` files here. They are copied to Android and iOS by `./build-rust.sh` (see `deploy_fonts`).

## Required for jianpu (numbered notation)

- **JianpuASCII.ttf** — [jianpu-ascii-font](https://github.com/RobertWinslow/jianpu-ascii-font)  
  Download: https://github.com/RobertWinslow/jianpu-ascii-font/raw/main/JianpuASCII.ttf  
  Save as: `assets/fonts/JianpuASCII.ttf`  
  Without this file, jianpu view shows raw ASCII (e.g. `5/` `3'`) instead of notation glyphs.

## Other fonts (lyrics, body text)

- **NotoSansCJK-Regular.ttc** — Noto Sans CJK (Chinese, Japanese, Korean). Primary CJK font for lyrics. TTC contains SC/TC/JP/KR faces.
- **Edwin-Bold.otf**, **Edwin-Italic.otf** — Edwin (for Western lyrics and body/titles)

After adding any `.ttf`, `.otf`, or `.ttc`, run `./build-rust.sh` (or your full app build) so they are deployed to the app. Release builds on GitHub Actions also copy these fonts into the iOS and Android bundles.
