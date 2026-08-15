# Blackbody

Blackbody is a viewer for FLIR and Fluke thermograms and other thermal images, for Linux and Windows. Explore temperatures under the cursor, render to different palettes and ranges, overlay the visible-light photo, and inspect embedded measurements and camera metadata.

Blackbody reads thermograms from many FLIR cameras, including the visible-light photo, and also opens Fluke thermograms from cameras such as the Ti400, Ti401p, TiS75+, and Ti25. Plain thermal data can be imported and exported as single-band TIFFs or 16-bit PNGs.

![Screenshot of the application](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody2-01.png)

🐧 [linux](https://flathub.org/en/apps/eu.nimmerfort.blackbody) --- 🪟 [windows](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody-2.1.1-windows.zip) --- 📦 [source](https://github.com/anieuwland/blackbody)

## Features

* Inspect the temperature under the cursor, in celsius, fahrenheit, or kelvin
* View embedded measurements: spots, lines, and areas
* Overlay view (Picture-in-Picture or MSX) combining thermal and visible images
* Render to the camera's embedded palette or many built-in ones, including [turbo](https://ai.googleblog.com/2019/08/turbo-improved-rainbow-colormap-for.html) and inferno
* Adjust the temperature range to make details stand out
* Metadata panel with camera info and GPS location, openable in a map
* Export renders to PNG and thermal data to TIFF or 16-bit PNG
* Zoom, pan, and browse through a folder of thermograms
* Translated to multiple languages
* Free and open source software, licensed under the EUPL
* Written in Rust to make it portable, reliable and fast

## Get it
Blackbody is available for Linux on [Flathub](https://flathub.org/apps/details/eu.nimmerfort.blackbody). 
It can be installed using your software center, the linked page or the following 
commands:

```shell
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install flathub eu.nimmerfort.blackbody
flatpak run eu.nimmerfort.blackbody
``` 

The [downloads](https://bitbucket.org/nimmerwoner/blackbody/downloads/) page
lists download links for Windows. It is a zip that can simply be extracted and
with an exe inside that can be ran.

## Supported file formats

| File format    | Temperatures | Visual data | Measurements | Embedded settings | PiP / MSX  | Write |
|----------------|-------------:|------------:|-------------:|------------------:|-----------:|------:|
| FLIR JPEG, FFF |          ✅  |         ✅  |          ✅  |               ✅  |        ✅  |       | 
| Fluke is2      |          ✅  |         ✅  |          ✅  |               ✅  |        ✅  |       |
| HTI / ToolTop  |          ✅  |         ✅  |              |                   |            |    ✅ |
| PNG, TIFF      |          ✅  |             |              |                   |            |    ✅ |

### libblackbody

Blackbody is the interactive UI, but there is a spin-off project 
[libblackbody](https://github.com/anieuwland/blackbody/tree/main/libblackbody) 
that aims to be *the* reusable, general purpose interface for thermograms, 
supporting all file formats.

## Screenshots

![View measurements drawn directly on the render and in a side-pane](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody2-05.png)
*View measurements drawn directly on the render and in a side-pane*

![The info panel shows camera details and metadata for every thermogram](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody2-02.png)
*The info panel shows camera details and metadata for every thermogram*

![Spot measurements on a thermogram in fahrenheit with a custom palette](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody2-03.png)
*Spot measurements on a thermogram in fahrenheit with a custom palette*

![Overlay view blends the thermal image with the visible-light photo](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody2-04.png)
*Overlay view blends the thermal image with the visible-light photo*

![Zoom in and narrow the temperature range to make the details stand out](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody2-06.png)
*Zoom in and narrow the temperature range to make the details stand out*

![Cold air leaking in under a door, captured with a Fluke is2](https://bitbucket.org/nimmerwoner/blackbody/downloads/blackbody2-07.png)
*Cold air leaking in under a door, captured with a Fluke is2*

## Building it

### Compile for Linux

Either use GNOME Builder (easier) or do it manually. Using Builder it is a
matter of pressing the compile&run button. Doing it manually involves
`cargo build --release`, copying the compiled gresource to the same directory as
the binary, and then `cargo run --release`.

### Compile for Windows
Compiling for Windows is more involved, but does work. Refer to 
`bitbucket-pipelines.yml` for the necessary steps.

## Comparable to

* Joe-C's [Thermovision](https://github.com/JoeC-de/ThermoVision_JoeC/tree/master) ([website](https://joe-c.de/software/thermovision)) - A C# general purpose thermogram studio supporting many different files formats, but unmaintained
* [Thermogram](https://github.com/s-du/Thermogram) - A Python DJI thermogram studio
* [ThermView](https://github.com/v0l/thermview) - Pre-alpha stage web based thermogram viewer

## Source code

* **[Github](https://github.com/anieuwland/blackbody)** - Blackbody's home
* **[bitbucket](https://bitbucket.org/nimmerwoner/blackbody/)** - Original where the build pipeline still lives.
