---
description: The Settings window's pages and their headed sections, which setting is on which page, how a page scrolls, the search field, and the one table the window is drawn from
read_when: Settings window, settings page, where a setting goes, adding a setting, settings scroll, settings search, settings table, settings_rows, page order, General page, Appearance page
type: spec
status: current
last_verified: 2026-10-22
covers:
  - crates/ui/src/settings/settings_view.rs
  - crates/ui/src/settings/settings_rows.rs
  - crates/ui/src/settings.rs
  - crates/ui/tests/settings_window.rs
---

# The Settings window

Options › Settings (Ctrl+,) opens the Settings window over the main window. How it is drawn
(sizes, colours, the controls) is in [components](../design/components.md) › Settings; this spec
says what is on each page, how the pages scroll and how a setting finds its place.

## Sources

Suhina's own layout, shaped by public design guidance on settings screens (Apple's and Microsoft's
settings guidelines, and articles on settings UX; the wording rule in *Adding a setting* follows [Microsoft's guidelines for app settings](https://learn.microsoft.com/windows/apps/design/app-settings/guidelines-for-app-settings) and [Toptal's settings UX article](https://www.toptal.com/designers/ux/settings-ux)) and by how three other programs arrange theirs,
all summarised in [#868](https://github.com/suhina-daw/suhina-daw/issues/868) in our own words: group by what the
user is doing, small groups, the most used first, no status or commands among settings, a change
applies at once. The grouping follows what the user is doing when they look for a setting (setting
up audio, editing clips, browsing samples), not which part of the code reads it.

## Pages and groups

The list on the left shows the pages in this order. Each page has headed groups; every setting is
on exactly one page, under one heading. A group that holds rarely changed settings is called
**Advanced**, comes last on its page and is folded until its heading is clicked (a search shows
what is inside it).

| Page | Group | Settings |
|---|---|---|
| Projects and backups | Projects | Open projects in, At start, Projects folder, Templates folder, Startup template (shown with At start › Template) |
| Projects and backups | Backups | Backup interval (the note names the newest backup of the active project), Delete old backups |
| Audio | Status | Audio status: the output and input in use, how long sound takes to come out, dropouts, List devices again (a panel, not a setting) |
| Audio | Output | Audio device: Driver, Device, Sample rate, Buffer size (or Engine block), Exclusive mode, Input |
| Audio | Cue | Headphone output: Cue output, Cue volume |
| MIDI | MIDI inputs | Each MIDI input port, on or off |
| MIDI | Arming | Arm selected track |
| Plug-ins | Actions | Scan, Rescan all, Manage plug-ins (commands, set off above the folders) |
| Plug-ins | Folders | Folders (each on or off, with its format; add with a format, remove) |
| Plug-ins | Editors | Plug-in editors open in (Window, Device view) |
| Devices | NAM | Amp models folder, Cabinet sounds folder ([nam spec](nam-amp-and-cab.md) › Folders) |
| Devices | Device panels | Device chain layout, Detail area width |
| Importing | Dropping files | Tracks for dropped files, Warp mode for samples |
| Arrangement | Clip editing | Let audio clips overlap, Let MIDI clips overlap, Double-click makes a clip, Clip menu name field, Clip drag handle |
| Arrangement | Time and play | Select time with, Start marker follows selection, Play from, Playhead after Stop |
| Arrangement | View | Grid inside clips, Waveform size |
| Arrangement | Tracks | New track colour, Track header side, Return tracks |
| Arrangement | Advanced | Edge drag stretches audio, Rename all selected clips |
| Session and mixer | Launching | Select launched clip |
| Session and mixer | Mixer | Mixer control style |
| Clip editor | Piano roll | Full-height piano roll covers, Piano roll grid source, Piano roll toolbar size, Moved notes replace notes |
| Browser | Browser | Browser side, Recent item kinds layout |
| Browser | Previews | Preview selected file, Preview volume |
| Browser | Sample map | Sample map layout, Preview on map hover, Map sample length limit |
| Browser | Tags | Colour tag limit |
| Keys | Mouse and key behaviour | The clip editor's workflow settings (click, right click, double-click, duplicate key); kept in `keymap.json` |
| Keys | Shortcuts | The preset, every action's keys, Reset all |
| Appearance | Window | Window zoom, Animations |
| Appearance | Waveforms | Waveform scale |
| Appearance | Advanced | Graphics renderer |
| Theme | Theme | The themes, the theme in use, its colours |
| Agents and updates | Agents | AI agents edit projects (off by default; [ADR 0028](../decisions/0028-mcp-in-the-running-app.md)) |
| Agents and updates | MCP write permissions | Project folder for agents (on by default, with the active folder shown), More agent write folders (Add folder… and Remove); all grants include subfolders |
| Agents and updates | Updates | Check for updates, Update source (Test builds only in developer mode) ([spec](updates.md)) |
| Agents and updates | Advanced | Developer mode |

The Settings window opens on the page used last in the session. Descriptions are shown whole: they
wrap under the setting's name. A yes/no setting is a switch; a choice of up to five is a row of
option buttons, each as wide as its text needs; more than five is a drop-down.

Where a setting is shown never changes how it is saved: each keeps the key it has in
`settings.json` (or `keymap.json` for keys and the mouse switches), so a settings file written
before a setting moved loads with the same values.

Export permissions apply to attached MCP writes only. The person adds a folder through the
system folder picker or removes its grant; the project-folder grant can also be switched off.
The settings persist across restarts. Missing folders grant no access, and each subsequent
operation uses the current grants. File › Export still writes to the destination the person
chooses. Agents cannot change these settings. Standalone MCP keeps its `--write-root` option.

## Scrolling

Each page scrolls when it is taller than the room under the title, at any window size: with the
mouse wheel over the page, by dragging its scroll bar on the right, and by moving the keyboard
focus (Tab) to a control out of view, which scrolls it into view. Opening another page shows it
from its top. A list that fills the space left on its page (the theme colours, the shortcuts, the
plug-ins found) is at least 160 px tall, so in a short window the page scrolls down to it, and
the list scrolls on its own.

## Search

A field above the list of pages, "Search settings", filters the settings by what is typed. A
setting matches when every word typed is in its name, its note, its section or its page (case
does not matter). While the field has text, the page shows only the matching settings, from
every page, under headings that name the page and section ("Browser › Previews"), and no page
in the list is lit; "No setting matches" when none does. Clicking a page empties the field and
shows that page; Escape in the field empties it (a second Escape closes Settings). Advanced groups are searched while folded.

## The table

The window is drawn from one table, `ROWS` in `crates/ui/src/settings/settings_rows.rs`: an entry per
setting with its page, its section, its name, its note, the keys it is saved under, and how its
control is drawn. A page shows its entries in the table's order, starting a section where the
section changes. Most entries are one row (a name, a note and a segmented control or a field); a
few are a block that draws several rows or a list (the audio output, the themes, the shortcuts).
Adding a setting to the window is adding its entry under its section; moving one is changing its
page and section. Tests check that every saved setting is on exactly one page.

## Adding a setting

The pages stay tidy only if each new setting follows these rules; a test in `settings_rows.rs`
checks the ones a machine can check and names this section when one fails.

1. **Pick the page by task**, from the table above: the page where someone doing that task would
   look. Do not add to a page because the code that reads the setting is near it. A new kind of task
   gets a new page, added to `Page::ALL` and to this table.
2. **Keep groups small:** at most seven entries in a group outside Advanced. Start a new group
   (with a heading that names the sub-task) rather than extending a full one. Rare or expert
   settings go under the page's **Advanced** group, last on the page.
3. **Settings only.** A readout of what is happening (latency, what is playing) is a status panel
   first on its page (`.status()`); buttons that do something once (Scan) are a row of actions
   first on its page (`.actions()`). Neither sits among the settings.
4. **Two columns, words a newcomer reads at once.** Names sit in the first column and every
   control starts at the second (`CONTROL_COLUMN` in `settings_view.rs`), so both run straight down
   the page; a row is its name and its control, with a second line only for what the setting
   reports now (the last backup). The wording follows (the Microsoft and Toptal guidance in Sources: plain words that
   say what a setting does, short, no jargon):
   - The **name** says exactly what the setting does, in one to four words, so that with its value it
     reads as one line (*Playhead after Stop: Marker*, *Arm selected track: on*, *Warp mode for
     samples: Rhythmic*). A noun for a choice, what it makes happen for a switch; not a sentence cut
     off before its buttons (*Grab a clip by*), not only the topic (*Moving clips*).
   - The **description** is one whole sentence that says what changes when the setting is on or
     chosen. By default it is the name's tooltip; **Setting descriptions** on the Appearance page
     shows it as a ? after the name, or under the name. It holds one thought: no semicolon, no second clause about the off state
     (the switch shows that), no shortcut keys (the Keys page lists them), and it does not end with
     a full stop.
   - **Plain words:** the effect, not the mechanism. Not *underruns* but dropouts, not *IR* but
     speaker cabinet recordings, not *MCP* but AI agent; a technical word may follow in the
     setting's options or help when someone needs it. No file paths into the docs, no ADR or issue
     numbers, no code names.
5. **One control per kind of choice:** a switch (`cx.on_off`) for yes/no, option buttons
   (`cx.choose`) for up to five choices that exclude each other, a drop-down beyond that
   (`cx.choose` switches by itself). No text is cut off.
6. **The stored key never changes** when a setting moves: it keeps its key in `settings.json`
   (`keymap.json` for keys and the mouse switches). A new setting gets a new key, listed in the
   entry's keys.
7. **Application settings apart from project settings.** What belongs to one project (its tempo,
   loop, a project's own defaults) lives with the project, not in Settings.
8. Update the table above and `docs/guide-data/settings-help.json`, and run
   `python scripts/settings_guide.py` for the website guide.
