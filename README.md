<h1 align="center">
  <img src="data/icons/io.github.seadve.Dagger.svg" alt="Dagger" width="192" height="192"/>
  <br>
  Dagger
</h1>

<p align="center">
  <strong>View and edit Graphviz DOT graphs</strong>
</p>

<p align="center">
  <a href="https://www.buymeacoffee.com/seadve">
    <img alt="Buy Me a Coffee" src="https://img.buymeacoffee.com/button-api/?text=Buy me a coffee&emoji=&slug=seadve&button_colour=FFDD00&font_colour=000000&font_family=Inter&outline_colour=000000&coffee_colour=ffffff" width="150"/>
  </a>
</p>

<br>

<p align="center">
 <a href="https://hosted.weblate.org/engage/kooha">
    <img alt="Translation status" src="https://hosted.weblate.org/widgets/kooha/-/dagger/svg-badge.svg"/>
  </a>
  <a href="https://github.com/SeaDve/dagger/actions/workflows/ci.yml">
    <img alt="CI status" src="https://github.com/SeaDve/dagger/actions/workflows/ci.yml/badge.svg"/>
  </a>
</p>

<p align="center">
  <img src="data/resources/screenshots/preview.png" alt="Preview"/>
</p>

Dagger provides facilities to edit and draw graphs specified in the [DOT language](https://graphviz.org/doc/info/lang.html). It is designed to be a simple and intuitive tool for creating and editing graphs, with a focus on the user experience.

The main features of Dagger include the following:
- 🖼️ Live and interactive preview of the graph as you type
- ⏺️ Multiple Graphviz layout engines support
- 📝 Fully-featured DOT language editor
- 📦 Export graph as PNG, SVG, or JPEG

## 🏗️ Building from source

### GNOME Builder
GNOME Builder is the environment used for developing this application. It can use Flatpak manifests to create a consistent building and running environment cross-distro. Thus, it is highly recommended you use it.

1. Download [GNOME Builder](https://flathub.org/apps/details/org.gnome.Builder).
2. In Builder, click the "Clone Repository" button at the bottom, using `https://github.com/SeaDve/Dagger.git` as the URL.
3. Click the build button at the top once the project is loaded.

### Meson
```
git clone https://github.com/SeaDve/Dagger.git
cd Dagger
meson _build --prefix=/usr/local
ninja -C _build install
```

## 📦 Third-Party Packages

Unlike Flatpak, take note that these packages are not officially supported by the developer.

### Repology

You can also check out other third-party packages on [Repology](https://repology.org/project/dagger/versions).

## 🙌 Help translate

You can help Dagger translate into your native language. If you find any typos
or think you can improve a translation, you can use the [Weblate](https://hosted.weblate.org/engage/kooha/) platform.

## ☕ Support me and the project

Dagger is free and will always be for everyone to use. If you like the project and
would like to support it, you may [buy me a coffee](https://www.buymeacoffee.com/seadve).

## 💝 Acknowledgment

I would like to thank the [contributors](https://github.com/SeaDve/dagger/graphs/contributors)
and [translators](https://hosted.weblate.org/engage/kooha/) of the project for helping Dagger
to grow and improve.

I would also like to acknowledge the open-source software projects, libraries, and APIs that were
used in developing this app, such as GStreamer, GTK, LibAdwaita, `d3-graphviz`, etc.,
for making Dagger possible.
