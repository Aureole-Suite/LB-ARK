# LB-ARK
###### Liberl Archive Redirection Kludge

A small dll that allows the *Trails in the Sky* games to load files from
directories instead of archive files. It's more convenient that way. Currently
it is mainly meant to simplify *development* of mods, but it could also be a
way to reduce conflicts between mods, by refining the granularity of conflicts
to files rather than whole archives.

To install, download [`d3dxof.dll`](releases/latest) and place it in the game's main
data directory (next to `ed6_win*.exe`).

## Compatibility

The dll is compatible with the latest Steam release (as of 2023-03-27) of all
three *Sky* games, both DX8 and DX9 versions, 
