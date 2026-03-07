# Bundled fonts

Place `.ttf` files here. They are copied to Android and iOS by `./build-rust.sh` (see `deploy_fonts`).

## Required for jianpu (numbered notation)

- **JianpuASCII.ttf** — [jianpu-ascii-font](https://github.com/RobertWinslow/jianpu-ascii-font)  
  Download: https://github.com/RobertWinslow/jianpu-ascii-font/raw/main/JianpuASCII.ttf  
  Save as: `assets/fonts/JianpuASCII.ttf`  
  Without this file, jianpu view shows raw ASCII (e.g. `5/` `3'`) instead of notation glyphs.

## Other fonts (lyrics, body text)

- **Lora-Regular.ttf**, **Lora-Italic.ttf** — Lora (optional, for body/titles)
- **LXGWWenKai-Regular.ttf** — LXGW WenKai (optional, for Chinese lyrics)

After adding any `.ttf`, run `./build-rust.sh` (or your full app build) so they are deployed to the app. Release builds on GitHub Actions also copy these fonts into the iOS and Android bundles.
