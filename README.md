# LB-ARK
###### Liberl Archive Redirection Kludge

A small dll that allows the *Trails in the Sky* games to load files from
directories instead of archive files. It's more convenient that way. This is
meant both to simplify development of mods by cutting out the repacking step
from the edit cycle, and also as a way of distributing mods, reducing file
sizes and reducing file conflicts by shipping files instead of entire archives.

To install, download
[`d3dxof.dll`](https://github.com/Kyuuhachi/LB-ARK/releases/tag/v1.0.0) and
place it in the game's main data directory (next to `ed6_win*.exe`).

## Background: How archives work

Most of the games' data is packed into a number of archive files, named
`ED6_DT00` through `ED6_DT3F`. It is not known exactly why these archives are
used, but possibly it improves performance since the operating system knows
that those files are frequently used and keeps them close at hand.

Each archive consists of two files: `ED6_DTnn.dir` and `ED6_DTnn.dat`. The
`.dir` file lists which files exist in the archive, with filename, location in
the `.dat` file, and some metadata. The `.dat` file, correspondingly, holds the
contents of all the files.

Most files in the archives are compressed. There is no real way to tell just
from the files whether they are compressed: the games instead determines this
based on what context they are used in. I believe `.wav` and `._ds` (DDS) files
are uncompressed while everything else is compressed, but I am not certain
about this.

Files inside the archives are referenced in two different ways: by filename, or
by file id. Just like with compression, which method is used in each situation
seems largely arbitrary.

**Filename** lookups always happen in a specific archive (for example, `._da`
files, containing fonts, are always in `ED6_DT20`). The list of files is simply
scanned linearly, and the first entry with a matching name is used. Internally,
the filenames are stored as uppercase 8.3 strings like `T0310   ._DT` — as you
may have seen in SoraVoice, for example. However, I reject this notation: all
my tools display this as `t0310._dt`. To the game, those two are identical, and
one of them does not make my eyes bleed.

**File ids** consist of a pair of numbers, denoting which archive the file is
in, and an ordinal inside this archive, as listed in the `.dir` file. In this
method, the filename is ignored. However, they can be assigned freely to any
archive, unlike filenames, which as mentioned are only looked for in a single
archive.

Some `ED6_DTnn.dat` files have no corresponding `.dir` file. These are in fact
not archive files at all — they are video files, which can be played with just
about any video player.

## Usage

To substitute a file inside an archive, place the substitute in
`data\ED6_DTnn\filename`, for example `data\ED6_DT21\u7000._sn`.

To enable adding custom files, LB-ARK scans for `data\ED6_DTnn\*.dir` files.
These files file should contain a list of files, one per line, with an ordinal
(either decimal or hexadecimal), followed by a filename (which can contain
subdirectories, if desired). If this filename is convertible to `8.3` format it
will be so, otherwise it will be inaccessible to filename lookups.

Multiple `*.dir` files are allowed in each archive, to enable multiple mods to
add their own files. However, duplicate ordinals is not allowed. Each archive
hold 65536 files[^65536], so don't just pick the lowest available numbers;
using more diverse ordinals will likely lead to less conflicts.

## Compatibility

LB-ARK supports the latest Steam release (as of 2023-03-27) of all three *Sky*
games, both DX8 and DX9 versions. It should be reasonably stable on past and
future versions too, but this is untested.

It is compatible with SoraVoice, with one caveat: SoraVoice's file redirection
happens before LB-ARK's, meaning if you have files both in `voice/scena` and
`data/ED6_DT21`, SoraVoice's are the ones that will be loaded. A script
([`move_sora_voice.ps1`](https://github.com/Kyuuhachi/LB-ARK/raw/main/move_sora_voice.ps1))
is provided to move a SoraVoice installation into LB-ARK's format.

[^65536]: The games as written only support up to 2047 (FC) or 4096 (SC and 3rd)
  files, but lifting this restriction was easier, and more useful, than coding
  in this different restriction for each game.
