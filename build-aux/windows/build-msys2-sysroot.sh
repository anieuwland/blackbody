#!/bin/bash
# Builds a Windows (x86_64) cross-compile sysroot by installing MSYS2's
# MINGW64 packages with the host's pacman. MSYS2 is the only repository that
# ships mingw builds of libadwaita >= 1.7; Fedora has no mingw libadwaita and
# its mingw64-gtk4 is too old to build one against. The MINGW64 (msvcrt)
# repo is used, matching Rust's x86_64-pc-windows-gnu target and Fedora's
# x86_64-w64-mingw32 toolchain.
set -euo pipefail

SYSROOT="${1:-/opt/msys2}"

mkdir -p "$SYSROOT/var/lib/pacman"
cat > /tmp/msys2-pacman.conf <<EOF
[options]
RootDir = $SYSROOT
DBPath = $SYSROOT/var/lib/pacman
CacheDir = $SYSROOT/var/cache/pacman/pkg
HookDir = $SYSROOT/no-hooks
SigLevel = Never
Architecture = x86_64

[mingw64]
Server = https://mirror.msys2.org/mingw/mingw64/
EOF

# Scriptlets expect a Windows shell and fail harmlessly under chroot.
pacman --config /tmp/msys2-pacman.conf -Sy --noconfirm \
    mingw-w64-x86_64-gtk4 \
    mingw-w64-x86_64-libadwaita \
    mingw-w64-x86_64-adwaita-icon-theme

# Regenerate what the scriptlets could not, with format-compatible host tools.
glib-compile-schemas "$SYSROOT/mingw64/share/glib-2.0/schemas"

# gdk-pixbuf's loaders.cache is normally generated on Windows by a pacman
# hook (gdk-pixbuf-query-loaders.exe), which cannot run here. The per-loader
# blocks are static, so write the ones we need: svg for icon themes and the
# placeholder graphic, png/jpeg for completeness. Relative paths resolve
# against the app dir at runtime. If a loader misbehaves on Windows, this is
# the file to regenerate with gdk-pixbuf-query-loaders.exe on a real machine.
cat > "$SYSROOT/mingw64/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache" <<'EOF'
"lib/gdk-pixbuf-2.0/2.10.0/loaders/pixbufloader_svg.dll"
"svg" 6 "gdk-pixbuf" "Scalable Vector Graphics" "LGPL"
"image/svg+xml" "image/svg" "image/svg-xml" "image/vnd.adobe.illustrator" "image/x-svg" "image/svg+xml-compressed" ""
"svg" "svgz" "svg.gz" ""
" <svg" "*    " 100
" <!DOCTYPE svg" "*             " 100

"lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-png.dll"
"png" 5 "gdk-pixbuf" "PNG" "LGPL"
"image/png" ""
"png" ""
"\211PNG\r\n\032\n" "" 100

"lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-jpeg.dll"
"jpeg" 5 "gdk-pixbuf" "JPEG" "LGPL"
"image/jpeg" ""
"jpeg" "jpe" "jpg" ""
"\377\330" "" 100

EOF
