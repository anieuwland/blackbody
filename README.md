# Blackbody

An image viewer specialized for thermograms. Currently supported are many FLIR
cameras and single-banded TIFF files.

![Screenshot of the application](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody-01.png)

## Compile for Linux

Either use GNOME Builder (easier) or do it manually. Using Builder it is a
question of pressing the compile&run button. Doing it manually involves 
`cargo build --release`, copying the compile gresource to the same directory as
the binary, and then `cargo run --release`.

## Compile for Windows
Compiling for Windows is more involved, but does work.

1. Install the necessary mingw packages
2. Run the build command with cross-compilation flags set. On Fedora: `PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_PATH=/usr/x86_64-w64-mingw32/sys-root/mingw/lib/pkgconfig/ MINGW_PREFIX=/usr/x86_64-w64-mingw32/sys-root/mingw/ cargo run --target=x86_64-pc-windows-gnu`
3. Copy DLLs, icons, glib schemas and the gresource to the same directory with the binary.
    1. `mkdir /wherever/release`
    2. `cp target/x86_64-pc-windows-gnu/release/*.exe /wherever/release`
    3. `cp $GTK_INSTALL_PATH/bin/*.dll /wherever/release`
    4. `mkdir -p /wherever/release/share/glib-2.0/schemas && cp $GTK_INSTALL_PATH/share/glib-2.0/schemas/* /wherever/release/share/glib-2.0/schemas`
    5. `mkdir -p /wherever/release/share/glib-2.0/schemas && cp -r $GTK_INSTALL_PATH/share/icons/* /wherever/release/share/icons`
4. Run with Wine or zip up the release dir and ru non Windows! Make sure `XDG_DATA_DIRS` is correctly set however: `XDG_DATA_DIRS=path/to/app/share blackbody.exe`

Reference: [Cross-compiling Rust Linux -> Window](https://gtk-rs.org/docs-src/tutorial/cross)
