#!/bin/sh

export MESON_BUILD_ROOT="$1"
export MESON_SOURCE_ROOT="$2"
export CARGO_TARGET_DIR="$MESON_BUILD_ROOT"/target
export OUTPUT="$3"
export BUILDTYPE="$4"
export APP_BIN="$5"

echo $BLACKBODY_BUILDER

if [ -z $CARGO_HOME ]
then
    # If $CARGO_HOME is unspecified, set it to a directory outside of the
    # sandbox to persist the downloaded dependencies. This prevents having to
    # redownload all dependencies every bild.
    export CARGO_HOME="$CARGO_TARGET_DIR"/cargo-home
    echo "UNDEFINED: \$CARGO_HOME - Setting to $CARGO_HOME"
fi

echo "Build type: $BUILDTYPE ($BLACKBODY_BUILDER)"
if [[ $BUILDTYPE = "release" ]]
then
    if [[ $BLACKBODY_BUILDER = "flathub" ]]
    then
        # When building for flathub, we have no access to the network. In this
        # case the dependencies are specified in the manifest instead of
        # Cargo.toml and downloaded by flatpak-builder.
        cargo --offline fetch --manifest-path "$MESON_SOURCE_ROOT"/Cargo.toml \
            && cargo --offline build --release \
            && cp "$CARGO_TARGET_DIR"/release/"$APP_BIN" "$OUTPUT"
    elif [[ $BLACKBODY_BUILDER = "windows" ]]
    then
        export MINGW_PREFIX="/usr/x86_64-w64-mingw32/sys-root/mingw/"
        export PKG_CONFIG_ALLOW_CROSS=1
        export PKG_CONFIG_PATH=$MINGW_PREFIX/lib/pkgconfig
        cargo build --manifest-path "$MESON_SOURCE_ROOT"/Cargo.toml --target=x86_64-pc-windows-gnu --release \
            && cp "$CARGO_TARGET_DIR"/x86_64-pc-windows-gnu/release/"$APP_BIN".exe "$OUTPUT".exe
    else
    	echo $CARGO_HOME
        cargo build --manifest-path "$MESON_SOURCE_ROOT"/Cargo.toml --release \
            && cp "$CARGO_TARGET_DIR"/release/"$APP_BIN" "$OUTPUT"
    fi
else
    cargo build --manifest-path "$MESON_SOURCE_ROOT"/Cargo.toml --verbose \
        && cp "$CARGO_TARGET_DIR"/debug/"$APP_BIN" "$OUTPUT"
fi
