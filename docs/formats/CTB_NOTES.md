# CHITUBOX print files (`.ctb`): what MeshPeek reads

A `.ctb` file is what a resin printer prints. It holds the sliced layers as images, the print settings and two
preview pictures; it holds no mesh. MeshPeek rebuilds the solid the layers print. The reader is
`crates/meshpeek-core/src/io/ctb.rs`. The format below was checked against a CHITUBOX 1.8.1 file (version 3,
ELEGOO Mars 2 Pro) and an encrypted file (version 4, Elegoo Mars 3): every layer decodes to exactly the screen's
pixel count.

## Header (versions 2 to 4, little-endian)

| Offset | Type | Field |
|---|---|---|
| 0 | u32 | Magic `0x12FD0086` (the encrypted format of newer CHITUBOX versions, below, is `0x12FD0107`) |
| 4 | u32 | Version |
| 8 | f32 ×3 | Bed size x, y, z (mm) |
| 28 | f32 | Total height (mm) |
| 32 | f32 | Layer height (mm) |
| 36 | f32 | Exposure (s) |
| 40 | f32 | Bottom exposure (s) |
| 44 | f32 | Light-off delay (s) |
| 48 | u32 | Bottom layer count |
| 52 | u32 ×2 | Screen resolution x, y (pixels) |
| 60 | u32 | Offset of the large preview |
| 64 | u32 | Offset of the layer table |
| 68 | u32 | Layer count |
| 72 | u32 | Offset of the small preview |
| 76 | u32 | Print time (s) |
| 80 | u32 | Projector type (1: mirrored LCD) |
| 84 | u32 ×2 | Print parameters: offset, size |
| 92 | u32 | Anti-aliasing level |
| 96 | u16 ×2 | Light PWM, bottom light PWM |
| 100 | u32 | Encryption key of the layer images (0: none) |
| 104 | u32 ×2 | Slicer information: offset, size |

The slicer information holds, at +28 and +32, the offset and length of the machine name, and at +48 the slicer's
version (`0x01080100` is 1.8.1).

## Layers

The layer table has 36 bytes per layer: height (f32, mm), exposure (f32), light-off delay (f32), the offset and
size of the layer's image (u32), the page number (u32: the offset is `page · 2³² + offset`) and three more words.

A layer image is one byte per run:

- bits 0–6: the grey level, 0 dark; it stands for `level << 1 | 1` (127 is full white);
- bit 7 set: a run length follows in 1 to 4 bytes, recognised by their first bits: `0xxxxxxx` (7 bits), `10xxxxxx`
  + 1 byte (14 bits), `110xxxxx` + 2 bytes (21 bits), `1110xxxx` + 3 bytes (28 bits), big-endian;
- bit 7 clear: a single pixel.

Pixels run row by row from the top left; a layer covers exactly `resolution x × resolution y` pixels. Image x
runs along the bed's x, and the top row is the back of the plate (+y).

With a non-zero encryption key, the bytes of layer `i` are XORed with a key stream (32-bit arithmetic, wrapping):

```text
seed   = key · 0x2D83CDAC + 0xD8A83423
stream = (i · 0x1E1530CD + 0xEC3D47CD) · seed
byte n of the image: XOR with byte (n mod 4) of stream (lowest first); after every 4 bytes, stream += seed
```

The previews use the 15-bit run-length encoding of CHITUBOX projects (`CHITUBOX_FILE_FORMAT.md`, section 4).

## Encrypted format (magic `0x12FD0107`)

Newer CHITUBOX versions save print files whose settings, and part of some layers, are encrypted with AES-256-CBC
(no padding). The key and IV are not published by CHITUBOX; UVtools found them and publishes them, scrambled, in
`UVtools.Core/FileFormats/CTBEncryptedFile.cs` (`Secret1` and `Secret2`, base64, XORed with the text `UVtools`).
MeshPeek stores the unscrambled bytes. The layout follows UVtools.

- Header (48 bytes): magic, settings size (288), settings offset, 0, version (4 or 5), signature size and
  offset (an AES-encrypted SHA-256 of the settings' first 8 bytes; MeshPeek does not check it), and five more
  words.
- Settings (288 bytes, encrypted): checksum (u64), offset of the layer pointers (+8), bed width, depth and height
  (+12, +16, +20), total height (+32), layer height (+36), exposure (+40), bottom exposure (+44), light-off delay
  (+48), bottom layers (+52), resolution x, y (+56, +60), layer count (+64), preview offsets (+68, +72), print time
  (+76), projector type (+80), lift and retract settings, the light PWM (+124), the key of the layer key stream
  (+128), the machine name's offset and length in the file (+160, +164), the anti-aliasing level (+176), and more
  settings.
- Layer pointers (16 bytes each): offset, page (`page · 2³² + offset`), table size (88), 0.
- Layer header (88 bytes, at the pointer): its size, height, exposure, light-off delay, data offset, page, image
  length (+24), 0, the offset and length of the encrypted part of the image (+32, +36), and the layer's lift,
  retract, rest and PWM settings. The image follows the header.
- A layer's image: decrypt its encrypted part (whole AES blocks, the same key and IV for every layer), then remove
  the key stream above with the key from the settings, then decode the runs. CHITUBOX writes broken images for some
  small encrypted layers (shorter than 512 bytes and not whole blocks); they are taken as empty, as UVtools does.

## How MeshPeek shows a print file

1. Every layer is decoded (in parallel) to find the area with lit pixels.
2. The layers are averaged into a grid of cells about 0.1 mm wide and high (a few pixels and layers each; coarser
   for very big prints, at most 64 million cells), the anti-aliasing grey levels included.
3. The surface where a cell is half lit is extracted with surface nets: one vertex per cell the surface passes
   through, one quad per grid edge it crosses. The surface is closed and faces outwards.
4. The result stands on the printer's plate (the bed size from the header), model and supports as one object;
   the information window shows the printer, the slicer version, the layers, exposures, print time, screen and
   the cell size used.

