#!/bin/sh

export MESON_BUILD_ROOT="$1"
export MESON_SOURCE_ROOT="$2"
export CARGO_TARGET_DIR="$MESON_BUILD_ROOT"/target
export OUTPUT="$3"
export BUILDTYPE="$4"
export APP_BIN="$5"


if [ -z $CARGO_HOME ]
then
    # If $CARGO_HOME is unspecified, set it to a directory outside of the
    # sandbox to persist the downloaded dependencies. This prevents having to
    # redownload all dependencies every bild.
    export CARGO_HOME="$CARGO_TARGET_DIR"/cargo-home
    echo "UNDEFINED: \$CARGO_HOME - Setting to $CARGO_HOME"
fi


if [[ $BUILDTYPE = "release" ]]
then
    echo "RELEASE MODE ($BLACKBODY_BUILDER)"
    if [[ $BLACKBODY_BUILDER = "flathub" ]]
    then
        # When building for flathub, we have no access to the network. In this
        # case the dependencies are specified in the manifest instead of
        # Cargo.toml and downloaded by flatpak-builder.
        cargo --offline fetch --manifest-path "$MESON_SOURCE_ROOT"/Cargo.toml \
            && cargo --offline build --release \
            && cp "$CARGO_TARGET_DIR"/release/"$APP_BIN" "$OUTPUT"
    else
    	echo $CARGO_HOME
        cargo build --manifest-path "$MESON_SOURCE_ROOT"/Cargo.toml --release \
            && cp "$CARGO_TARGET_DIR"/release/"$APP_BIN" "$OUTPUT"
    fi
else
    cargo build --manifest-path "$MESON_SOURCE_ROOT"/Cargo.toml --verbose \
        && cp "$CARGO_TARGET_DIR"/debug/"$APP_BIN" "$OUTPUT"
fi
