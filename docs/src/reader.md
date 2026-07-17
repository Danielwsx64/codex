# TUI reader

codex can read books directly in the terminal. The reader is TUI-only — open a
book from the Library (there is no CLI equivalent). Text is reflowed to the
viewport and recomputed on resize.

## Supported formats

| Format        | Notes                                                          |
| ------------- | ------------------------------------------------------------- |
| EPUB          | Spine extraction + HTML→text                                   |
| TXT / Markdown| Markdown via CommonMark; TXT read directly                      |
| MOBI          | PalmDOC / uncompressed; HUFF/CDIC is refused with a clear message |
| AZW3 (KF8)    | Dual-stream AZW3 reads the legacy stream; KF8-only is refused with a suggestion to convert to EPUB |
| PDF           | Single-column text; multi-column is best-effort with a warning |

DRM-protected books (Amazon Topaz / KFX / protected AZW) are detected and
refused — **codex does not remove DRM**. Only sideloaded, DRM-free files work.

## Navigation

Vim-style cursor and paging:

| Keys                         | Action                          |
| ---------------------------- | ------------------------------- |
| `h j k l`                    | Move the cursor                 |
| `w b e` `0` `$`              | Word / line motions             |
| `gg` `G`                     | Top / bottom                    |
| `Space` `Ctrl+f` `Ctrl+b`    | Page down / up                  |
| `Ctrl+d` `Ctrl+u`            | Half-page down / up             |
| `[` `]`                      | Previous / next chapter         |
| `:N`                         | Jump to absolute page N         |
| `:cN`                        | Jump to chapter N               |
| `?`                          | Contextual help                 |
| `Esc`                        | Back to the Library             |

The footer shows `chapter X/Y · page A/B`. For PDF, each page is a chapter, so
`:cN` jumps to the real document page.

## Reading progress

Progress (last chapter, offset, and time) is saved when you change chapters,
page, or leave the reader, and restored when you reopen the book.

## Performance

Conversion results (PDF/EPUB/MOBI/AZW3) are cached on disk under the XDG cache
dir and invalidated by source mtime + size. Opening runs in a background thread
with an animated loading screen; the TUI stays responsive and `Esc` cancels.
