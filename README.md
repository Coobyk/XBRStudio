# XBR Studio

Create Minecraft texture packs with the xBRZ algorithm — and a lot more.

XBR Studio upscales Minecraft textures, keeps the pixel art crisp, and
knows how the blocks and entities of the game are put together: it reads the
models of the game, stitches the faces of a cube together and can build a
complete resource pack out of a client jar.

```sh
# one texture
xbrstudio stone.png --factor 4

# a whole resource pack
xbrstudio --jar minecraft-client.jar --output pack_x4 --zip pack_x4.zip

# an existing resource pack, a zip or a folder
xbrstudio --jar my-pack.zip --output pack_x4
```

## What it does

* **Upscales** with xBRZ (the pixel art classic) or with FSR, Lanczos-3,
  Catmull-Rom and nearest neighbour.
* **Stitches model faces.** Vanilla textures that hold several faces of a
  block in one image are cut apart and every face is given the border of the
  face that touches it in three dimensions, so the seams disappear.
* **Understands the game.** Block models are read out of the client jar,
  entity models out of a decompiled source tree or a model directory, and
  animated textures keep their animation.
* **Builds a pack.** `pack.mcmeta` is written, the pack can be zipped and
  installed into Minecraft, and OptiFine connected textures can be generated
  for blocks that have several faces.

## Building

```sh
cargo build --release
```

The binaries end up in `target/release`:

| Binary | What it does |
| --- | --- |
| `xbrstudio` | The command line tool: single textures and whole packs. |
| `gui` | The desktop app. |
| `extract_models` | Turns a decompiled Minecraft source tree into entity model JSONs. |
| `slice` | Cuts a texture into one image per model face (debugging). |
| `visualize` | Draws the UV rectangles of a model onto a texture (debugging). |
| `preview3d` | Serves a small web page that shows a model in 3D. |

The GPU backend is behind the `gpu` feature, which is on by default. Build
without it with `cargo build --release --no-default-features`.

## Using the command line

### One texture

```sh
xbrstudio stone.png                       # writes stone_x4.png
xbrstudio stone.png --factor 6 --output big.png
xbrstudio stone.png --model lantern.json  # stitch the faces of the model
```

### A whole pack

```sh
xbrstudio --jar minecraft-client.jar --output pack_x4
```

The source can be a client jar, a resource pack zip or a resource pack
folder — the layout is the same, so an existing pack is upscaled the
same way.

The tool upscales every texture under `assets/minecraft/textures/`:

* Biome colormaps are copied byte for byte, they must not be scaled.
* Animated textures are scaled frame by frame and their `.mcmeta` is
  rewritten.
* Block textures get a one pixel self tiling border, so the edges of a
  tiling block keep their context.
* Textures that belong to a model with several faces are stitched.

Useful flags:

| Flag | What it does |
| --- | --- |
| `--factor 2..6` | The scale factor. |
| `--algorithm xbrz\|fsr\|lanczos3\|bicubic\|nearest` | The upscaler. |
| `--alpha straight\|premultiplied\|masked` | How transparent texels are handled. |
| `--sharpen 0..1` | An extra sharpening pass. |
| `--models <dir>` | Where the entity models live (default `models/entity`). |
| `--only <text>` | Only process the textures whose path contains `<text>`. |
| `--resume` | Skip the textures that are already up to date. |
| `--zip <file>` | Also write the pack as a zip. |
| `--install` | Install the zip into the Minecraft resource pack folder. |
| `--connected-textures` | Also write OptiFine connected textures. |
| `--pack-format <n>`, `--pack-description <text>`, `--mipmap-levels <n>` | What ends up in `pack.mcmeta`. |
| `--config <file>` | Read the settings from a file. |

### Settings in a file

Every flag has a counterpart in `xbrstudio.toml`, which is read from the
working directory or from the user configuration directory:

```toml
factor = 4
algorithm = "xbrz"
alpha = "premultiplied"
sharpen = 0.0
stitch = true
wrap = true
models = "models/entity"
pack_format = 97
```

### Alpha modes

Minecraft textures are full of transparency. How a scaler treats it decides
whether leaves and glass grow grey fringes:

* `straight` — every channel is interpolated on its own. This is what most
  tools do, and it lets the colour of a transparent texel bleed into its
  neighbours.
* `premultiplied` — the colour is scaled together with the alpha and
  unpremultiplied afterwards, so a transparent texel cannot darken its
  neighbours.
* `masked` — every tap is weighted by the alpha of the texel it reads, so
  fully transparent texels do not contribute at all.

### Entity models

Modern Minecraft defines its entity models in code, so the models have to be
extracted from a decompiled source tree:

```sh
extract_models --source /path/to/decompiled --output models/entity
```

The extractor understands the two shapes Mojang uses: chained
`addOrReplaceChild` calls and children that are attached through a variable.
Every `texOffs` applies to the boxes that follow it, the texture size comes
from `LayerDefinition.create` and values that are not plain numbers are kept
as text.

## The desktop app

```sh
cargo run --bin gui
```

* Compare the original and the upscaled texture with a wipe slider.
* Pick a model from the block models of a jar or from the entity models of
  the model directory.
* Look at the model in 3D — the preview is rendered by a small software
  rasterizer, no browser involved.
* Run a batch with a progress bar, cancel it, then zip the pack and install
  it into Minecraft.

## Connected textures

With `--connected-textures` the pack also gets OptiFine connected textures
for every block that has more than one face. The tool reproduces OptiFine's
tile selection (the 47 tile sheet and the corner refinement) and composes
the tiles from the faces of the block: a quadrant that touches a side which
does not connect is taken from the face on that side.

## Development

```sh
cargo test          # the unit tests
cargo fmt --all     # format the code
cargo clippy --all-targets
```

The CI runs the same checks on Linux, macOS and Windows.

## License

GPL-3.0, see [LICENSE](LICENSE).
