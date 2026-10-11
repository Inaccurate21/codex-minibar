#!/usr/bin/env bash
# Renders every patch-notes asset and the launch video into out/release.
# Usage: ./render.sh [composition...]   (default: all)
set -euo pipefail
cd "$(dirname "$0")"
OUT=out/release
mkdir -p "$OUT"
ALL=(HomeStill Notifications NotificationsLight Home Layouts Themes Languages Usage Launch)
TARGETS=("${@:-${ALL[@]}}")

frames() { # composition, scale -> PNG sequence with alpha in out/frames/<id>
  rm -rf "out/frames/$1"
  npx remotion render "$1" "out/frames/$1" --sequence --image-format=png --scale="$2" --log=error
}

animated() { # id, name, transparent(0/1)
  local id=$1 name=$2 alpha=$3 in="out/frames/$1/element-%03d.png"
  [ -f "out/frames/$1/element-0000.png" ] && in="out/frames/$1/element-%04d.png"
  # 30 fps for the light formats, every other 60 fps frame.
  ffmpeg -y -v error -framerate 60 -i "$in" -vf "fps=30" -c:v libwebp_anim -lossless 0 -q:v 82 -loop 0 "$OUT/$name.webp"
  ffmpeg -y -v error -framerate 60 -i "$in" -vf "fps=25,scale=iw/2:-1:flags=lanczos,split[a][b];[a]palettegen=reserve_transparent=$alpha:stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a" "$OUT/$name.gif"
  if [ "$alpha" = 1 ]; then
    ffmpeg -y -v error -framerate 60 -i "$in" -c:v libvpx-vp9 -pix_fmt yuva420p -b:v 0 -crf 24 -row-mt 1 "$OUT/$name.webm"
    ffmpeg -y -v error -framerate 60 -i "$in" -c:v prores_ks -profile:v 4444 -pix_fmt yuva444p10le "$OUT/$name.mov"
  else
    ffmpeg -y -v error -framerate 60 -i "$in" -c:v libx264 -pix_fmt yuv420p -crf 16 -preset slow -movflags +faststart "$OUT/$name.mp4"
  fi
}

# Release notes media: a 16:9 hero still and every video chapter as a 720p WebP.
patchnotes() {
  local dir="$OUT/patchnotes"
  mkdir -p "$dir"
  npx remotion still Hero "$dir/home.png" --log=error
  for id in accounts openrouter activation notifications usage themes languages; do
    rm -rf "out/frames/pn-$id"
    npx remotion render "Chapter-$id" "out/frames/pn-$id" --sequence --image-format=jpeg --jpeg-quality=95 --scale=0.6667 --log=error
    ffmpeg -y -v error -framerate 60 -i "out/frames/pn-$id/element-%03d.jpeg" -vf "fps=25" -c:v libwebp_anim -lossless 0 -q:v 65 -compression_level 6 -loop 0 "$dir/$id.webp"
  done
}

for target in "${TARGETS[@]}"; do
  echo "== $target"
  case $target in
    HomeStill) npx remotion still HomeStill "$OUT/home.png" --scale=2 --log=error ;;
    Notifications) frames Notifications 2 && animated Notifications notifications 1
      # A soundtrack-carrying MP4 on the dark desktop is part of the launch video.
      ;;
    NotificationsLight) frames NotificationsLight 2 && animated NotificationsLight notifications-light 1 ;;
    Patchnotes) patchnotes ;;
    Launch)
      # 4K master; the 1080p cut is downscaled from it.
      npx remotion render Launch "$OUT/launch-4k.mp4" --codec=h264 --crf=18 --scale=2 --log=error
      ffmpeg -y -v error -i "$OUT/launch-4k.mp4" -vf "scale=1920:1080:flags=lanczos" -c:v libx264 -crf 16 -preset slow -c:a copy -movflags +faststart "$OUT/launch-1080p.mp4"
      ;;
    *) frames "$target" 2 && animated "$target" "$(echo "$target" | tr '[:upper:]' '[:lower:]')" 0 ;;
  esac
done
ls -la "$OUT"
