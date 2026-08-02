# Blackbody

An image viewer specialized in rendering thermograms. For an animated example, visit [here](PREVIEW.md) (loads a 7.5MB image). 

![Screenshot of the application](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody-01.png)

## Features
* Opens many FLIR thermograms, especially recent ones
* Licensed under the EUPL, it is free software, both in price and in user rights
* Works on Linux and Windows
* Renders to several different color palettes (grayscale, [turbo](https://ai.googleblog.com/2019/08/turbo-improved-rainbow-colormap-for.html), inferno, and more)
* Dynamically set minimum and maximum values for rendering
* Zoom in and out
* Written in Rust and therefore fast

## Get it
Blackbody is available on [Flathub](https://flathub.org/apps/details/eu.nimmerfort.blackbody). 
It can be installed using your software center, the linked page or the following 
commands:

```shell
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install flathub eu.nimmerfort.blackbody
flatpak run eu.nimmerfort.blackbody
``` 

The [downloads](https://bitbucket.org/nimmerwoner/blackbody/downloads/) page
lists download options for Linux and Windows. The Linux version is available as
a flatpak. After installing, Blackbody then appears in your overview. The
Windows is provided as zip. You will have to unzip it and place the containing
directory where you want to install it manually. Run the application by double
clicking `blackbody.exe`.

## Compile for Linux

Either use GNOME Builder (easier) or do it manually. Using Builder it is a
matter of pressing the compile&run button. Doing it manually involves
`cargo build --release`, copying the compiled gresource to the same directory as
the binary, and then `cargo run --release`.

## Compile for Windows
Compiling for Windows is more involved, but does work.

1. Install the necessary mingw packages
2. Run the build command with cross-compilation flags set. On Fedora: `PKG_CONFIG_ALLOW_CROSS=1 MINGW_PREFIX=/usr/x86_64-w64-mingw32/sys-root/mingw PKG_CONFIG_PATH=$MINGW_PREFIX/lib/pkgconfig cargo build --target=x86_64-pc-windows-gnu --release`
3. Copy DLLs, icons, glib schemas and the gresource to the same directory with the binary.
    1. `mkdir blackbody-windows`
    2. `cp target/x86_64-pc-windows-gnu/release/*.exe blackbody-windows/`
    3. `cp /usr/x86_64-w64-mingw32/sys-root/mingw/bin/*.dll blackbody-windows/`
    4. `cp /usr/x86_64-w64-mingw32/sys-root/mingw/bin/gdbus.exe blackbody-windows/`
    4. `mkdir -p blackbody-windows/share/glib-2.0/schemas`
    5. `cp /usr/x86_64-w64-mingw32/sys-root/mingw/share/glib-2.0/schemas/gschemas.compiled blackbody-windows/share/glib-2.0/schemas/gschemas.compiled`
    5. `cp -r /usr/x86_64-w64-mingw32/sys-root/mingw/share/icons blackbody-windows/share/icons`
    6. Compile the gresource bundle and copy it to `blackbody-windows`
4. Run with Wine or zip up the release dir and ru non Windows! When using wine `XDG_DATA_DIRS` is correctly set however: `XDG_DATA_DIRS=blackbody-windows/share wine blackbody.exe`. In Windows it doesn't seem to matter.

Reference: [Cross-compiling Rust Linux -> Windows](https://gtk-rs.org/docs-src/tutorial/cross)

## Comparable to

* [ThermView](https://github.com/v0l/thermview)
