#!/usr/bin/env bash
# Generate Android launcher icons from the single source icon/app-icon-1024.png.
# Run from repository root. Uses sips (macOS) or ImageMagick (Linux).

set -e
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

ICON_SRC="icon/app-icon-1024.png"
RES="android/app/src/main/res"

if [[ ! -f "$ICON_SRC" ]]; then
  echo "Error: $ICON_SRC not found. Run from repo root." >&2
  exit 1
fi

# Copy 1024 source as drawable foreground (adaptive icon uses this)
mkdir -p "$RES/drawable"
cp "$ICON_SRC" "$RES/drawable/ic_launcher_foreground.png"

# Resize helper: $1=size, $2=output path
resize() {
  local size=$1
  local out=$2
  if command -v sips >/dev/null 2>&1; then
    sips -z "$size" "$size" "$ICON_SRC" --out "$out" >/dev/null 2>&1
  elif command -v convert >/dev/null 2>&1; then
    convert "$ICON_SRC" -resize "${size}x${size}" "$out"
  else
    echo "Error: need sips (macOS) or ImageMagick (convert) to generate mipmap icons." >&2
    exit 1
  fi
}

# Generate mipmap densities: mdpi=48, hdpi=72, xhdpi=96, xxhdpi=144, xxxhdpi=192
declare -A SIZES=( ["mipmap-mdpi"]=48 ["mipmap-hdpi"]=72 ["mipmap-xhdpi"]=96 ["mipmap-xxhdpi"]=144 ["mipmap-xxxhdpi"]=192 )
for dir in "${!SIZES[@]}"; do
  sz=${SIZES[$dir]}
  mkdir -p "$RES/$dir"
  resize "$sz" "$RES/$dir/ic_launcher.png"
  resize "$sz" "$RES/$dir/ic_launcher_round.png"
done

echo "Android icons updated from $ICON_SRC"
