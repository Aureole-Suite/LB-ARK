# LB-ARK
###### Liberl Archive Redirection Kludge

A small dll that allows the *Trails in the Sky* games to load files from
directories instead of archive files. It's more convenient that way. Currently
it is mainly meant to simplify *development* of mods, but it could also be a
way to reduce conflicts between mods, by refining the granularity of conflicts
to files rather than whole archives.

To install, download
[`d3dxof.dll`](https://github.com/Kyuuhachi/LB-ARK/releases/tag/v1.0.0) and
place it in the game's main data directory (next to `ed6_win*.exe`).

## Compatibility

LB-ARK supports the latest Steam release (as of 2023-03-27) of all three *Sky*
games, both DX8 and DX9 versions.

It is compatible with SoraVoice, with one caveat: SoraVoice's file redirection
happens before LB-ARK's, meaning if you have files both in `voice/scena` and
`data/ED6_DT21`, SoraVoice's are the ones that will be loaded. A script
([`move_sora_voice.ps1`](https://github.com/Kyuuhachi/LB-ARK/raw/main/move_sora_voice.ps1))
is provided to move a SoraVoice installation into LB-ARK's format.
