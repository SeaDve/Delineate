<h1 align="center">
  <img src="data/icons/io.github.seadve.Dagger.svg" alt="Dagger" width="192" height="192"/>
  <br>
  Dagger
</h1>

<p align="center">
  <strong>View and edit Graphviz DOT graphs</strong>
</p>

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
