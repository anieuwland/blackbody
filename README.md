# Blackbody

An image viewer specialized in rendering thermograms.

![Screenshot of the application](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody-01.png)

## Features
* Opens many FLIR thermograms, especially recent ones
* Licensed under the EUPL, it is free software, both in price and in user rights
* Works on Linux and Windows
* Renders to several different color palettes (grayscale, [turbo](https://ai.googleblog.com/2019/08/turbo-improved-rainbow-colormap-for.html), inferno, and more)
* Dynamically set minimum and maximum values for rendering
* Zoom in and out
* Written in Rust and therefore very fast

## Download and use
The [downloads](https://bitbucket.org/nimmerwoner/blackbody/downloads/) page
lists download options for Linux and Windows. The Linux version is available as
a flatpak. After installing, Blackbody then appears in your overview. The
Windows is provided as zip. You will have to unzip it and place the containing
directory where you want to install it manually. Run the application by double
clicking `blackbody.exe`.

## Compile for Linux

Either use GNOME Builder (easier) or do it manually. Using Builder it is a
question of pressing the compile&run button. Doing it manually involves
`cargo build --release`, copying the compile gresource to the same directory as
the binary, and then `cargo run --release`.

## Compile for Windows
Compiling for Windows is more involved, but does work.

1. Install the necessary mingw packages
2. Run the build command with cross-compilation flags set. On Fedora: `PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_PATH=/usr/x86_64-w64-mingw32/sys-root/mingw/lib/pkgconfig/ MINGW_PREFIX=/usr/x86_64-w64-mingw32/sys-root/mingw/ cargo build --target=x86_64-pc-windows-gnu --release`
3. Copy DLLs, icons, glib schemas and the gresource to the same directory with the binary.
    1. `mkdir /wherever/release`
    2. `cp target/x86_64-pc-windows-gnu/release/*.exe /wherever/release`
    3. `cp $GTK_INSTALL_PATH/bin/*.dll /wherever/release`
    4. `mkdir -p /wherever/release/share/glib-2.0/schemas && cp $GTK_INSTALL_PATH/share/glib-2.0/schemas/* /wherever/release/share/glib-2.0/schemas`
    5. `cp -r $GTK_INSTALL_PATH/share/icons /wherever/release/share/`
    6. Compile the gresource bundle and copy it to `/wherever/release`
4. Run with Wine or zip up the release dir and ru non Windows! Make sure `XDG_DATA_DIRS` is correctly set however: `XDG_DATA_DIRS=path/to/app/share blackbody.exe`

Reference: [Cross-compiling Rust Linux -> Window](https://gtk-rs.org/docs-src/tutorial/cross)
