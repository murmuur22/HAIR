# HAIR

HAIR is a small spatial browser for macOS. Local folders, Markdown sites and bounded static HTTPS pages become rooms you walk through: documents are objects you read, and links and subfolders are doors.

Version 0.4.1 (test build, unreleased).

## New in 0.4.1

- **Readable address and door panels:** the HTTPS address field (L) and the panel shown when you aim at a web door now use the same 2x text size as the reader and menus. Settings → UI scale enlarges them further. A long address entry scrolls with Up/Down; any edit returns to the cursor. A door address too long for the panel keeps its start and its exact end, with a count of the characters not shown. Tab to the link in the reader to see the full address.
- **Reader scrolling:** the mouse wheel scrolls 3 rows per notch, and trackpad pixels scroll smoothly. Up/Down repeat while held. Scrolling stops at the ends of the content. The wheel does nothing in the world, the paused menu, address entry, the folder chooser or the image viewer. Following a link or returning starts the new document with no leftover partial scroll. After the window loses focus, press Up/Down again.
- **Code in web pages:** `<pre>` blocks keep exact indentation, blank lines, case and punctuation. They are drawn on a code background with a gutter: `|` starts a source line and `>` marks a wrapped continuation. Inline `<code>` gets a code colour. Python, Rust, JavaScript/TypeScript, C/C++, shell, JSON and HTML/XML get passive colouring; other languages stay plain. No code, script or style is ever run.
- **Return door:** the visitor-history Return door is a real opening in the rear wall, opposite the link doors. It does the same as R and shows the previous page's title and address. On the first page it says that no previous page exists.
- **Web map (M):** web rooms and readers have a collapsible WEB LINKS/HISTORY panel:
  - `<` history
  - `=` rooms of the same page
  - `>` current page
  - `*` visited and `-` unvisited links
  - `^` another origin

  The panel shows only what this session already knows; nothing is crawled or preloaded. Whether it is shown or hidden survives following links, R, refresh, theme changes and the menu.
- **UI scale:** Settings scales the HUD, reader, address entry, door panels and menus from 100% to 200%. Smaller windows apply less and say so. For example, a 960x640 window is limited to 120% and a 1280x960 window to 160%; 1920x1280 reaches 200%. 3D world labels are unchanged.

## Launch

- `hair` — the bundled demo room. Press **O** to choose a folder with the macOS chooser.
- `hair --folder PATH` — a read-only folder explorer.
- `hair --url https://host/page` — a bounded static HTML spatial reader.
- `hair --site /folder` — a local Markdown + PNG site. Restart to reload.
- `hair --pack /pack.toml` — a local data-only appearance pack. An invalid pack falls back to the office pack.
- `hair --help` — the full control reference.

## Controls

**Moving**
- Click to look. **WASD** moves; hold either Shift to run.
- **E** activates the near, aimed object or door.
- **Tab / Shift+Tab** cycles nearby targets or the destinations of a grouped doorway.

**Navigating**
- **R** returns through visitor history (in a web page, R also cancels a load in progress).
- **U** goes to the parent folder (folders).
- **F5** refreshes.
- **M** toggles the outline or web map.
- **T** changes appearance.
- **I** shows full escaped source or metadata.

**Web**
- **L** opens the HTTPS address field: type the literal URL. **Enter** loads, **Backspace** edits, **Delete** clears, **Up/Down** scroll a long entry and **Escape** dismisses the field.
- **F8** shows exact destination and error details for a load in progress or a failed load.

**Reader**
- **Up/Down**, the mouse wheel or the trackpad scroll.
- **Tab** selects links; **E** follows supported links.
- **Left/Right** page 4096-byte raw chunks of local files.
- **V** switches text/raw/image; **C** retries the same file handle.
- **Escape** closes.

**Images**
- **= / -** zoom, the arrows pan and **0** resets to fit.

**Menu**
- **Escape** (after closing the address field or a reader) opens the paused menu: Resume, Settings, Quit.
- **Q** opens the menu with Quit selected; only Enter or a click on Quit exits.
- Settings → UI scale lasts for the current session only.

## Reading files

E on a regular file reads one bounded 4096-byte range through a held, read-only handle. Valid UTF-8 opens as text and binary files open as raw bytes. Invalid or boundary UTF-8 and control characters are shown escaped, never dropped. Static 8-bit PNG files of up to 4 MiB and 1024 pixels per side open in the image viewer.

On macOS, baseline JPEG files open with the same limits. Other formats show raw bytes and source information only.

## Limits and safety

- **Files:** HAIR is read-only. It never writes, executes, uploads or decrypts files, and it never changes permissions.
- **Folders:** a directory listing is capped at 16384 entries and history at 64 entries. Folders are never indexed recursively.
- **Web content:** HTTPS to public DNS only. HAIR keeps static HTML text and links. It does not run scripts, submit forms, log in or apply styles. Images from websites are not fetched or displayed.
- **Web history:** at most 8 history pages are kept, with 64 links per page and 8 grouped door slots per page. A page larger than one bounded window continues in the same response's next and previous rooms.
- **Code:** highlighting is small lexical colouring, not a language parser. A single literal code run larger than the 16 KiB window budget is refused with a message rather than cropped silently.
- **Display:** the bitmap text is ASCII. Other characters are shown as `?` or escaped, and `~` marks a shortened label.
- **Not a sandbox:** these limits are bounds on work, not a security sandbox.
- **Platform:** this is an unnotarized test build for macOS.
