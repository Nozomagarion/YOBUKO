// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Every setting the Settings window shows, in one table: its page, its section, its name and
//! note, the keys it is saved under and how its control is drawn (`docs/spec/ui-settings.md`).
//! A page shows its entries in this order, a new section heading where the section changes.
//!
//! Adding a setting is one entry under its section, most often a `row` whose control is one call:
//! `cx.on_off(r, |s| &mut s.field)` or `cx.choose(r, &[("Text", value), …], |s| &mut s.field)`.

#[cfg(test)]
use crate::settings::settings_view::{ADVANCED, Role};
use crate::settings::settings_view::{Entry, Page, block, row};
use crate::settings::view_settings::DetailLayout;
use crate::settings::{
    AtStart, DeviceLayout, EditorPlacement, Graphics, MixerControls, OnOpen, PianoRollTop, PlayFrom, Side, StopReturn,
    ToolbarSize, WarpMode,
};
use suhina_update::Channel;

/// The keys of `keymap.json`, where the keys and the mouse switches are saved.
pub(crate) const KEYMAP: &[&str] = &["keymap.json"];

/// Every setting the window shows, page by page.
pub(crate) static ROWS: &[Entry] = &[
    row(
        Page::Projects,
        "Projects",
        "When opening a project",
        "While another is open: ask, open it in a new tab, or replace it",
        &["open_projects"],
        |cx, r| {
            let options = [("Ask", OnOpen::Ask), ("New tab", OnOpen::NewTab), ("Replace", OnOpen::Replace)];
            cx.choose(r, &options, |s| &mut s.on_open);
        },
    ),
    row(
        Page::Projects,
        "Projects",
        "At start",
        "With no project given: the start panel, a new one, one from a template, or the last",
        &["at_start"],
        |cx, r| {
            let options = [
                ("Panel", AtStart::Panel),
                ("New", AtStart::Blank),
                ("Template", AtStart::Template),
                ("Last", AtStart::Last),
            ];
            cx.choose(r, &options, |s| &mut s.at_start);
        },
    ),
    row(
        Page::Projects,
        "Projects",
        "Projects folder",
        "Where a new project is saved first",
        &["projects_folder"],
        |cx, r| {
            let default = cx.about.projects_default.clone();
            cx.folder(r, &default, crate::interaction::pick::PickField::ProjectsFolder, |s| &mut s.projects_folder);
        },
    ),
    row(
        Page::Projects,
        "Projects",
        "Templates folder",
        "Projects listed under New › From a template, and where Save as template puts one",
        &["templates_folder"],
        |cx, r| {
            let default = cx.about.templates_default.clone();
            cx.folder(r, &default, crate::interaction::pick::PickField::TemplatesFolder, |s| &mut s.templates_folder);
        },
    ),
    row(
        Page::Projects,
        "Projects",
        "Start template",
        "The template At start makes a new project from",
        &["start_template"],
        |cx, r| {
            let hint = "A template's file name, or a path";
            cx.folder(r, hint, crate::interaction::pick::PickField::StartTemplate, |s| &mut s.start_template);
        },
    )
    .when(|s| s.at_start == AtStart::Template),
    row(
        Page::Projects,
        "Backups",
        "Backups",
        "When a copy of each changed project is written to .backup beside it",
        &["backup"],
        |cx, r| {
            let options: Vec<(String, suhina_session::backup::Interval)> =
                suhina_session::backup::Interval::ALL.iter().map(|i| (i.title(), *i)).collect();
            let last = cx.about.last_backup.clone();
            cx.note(format!("When a copy of each changed project is written to .backup beside it; last backup: {last}"));
            cx.dropdown(r, &options, |s| &mut s.backup);
        },
    ),
    row(
        Page::Projects,
        "Backups",
        "Clean up backups",
        "Keep the newest 20 backups of each project and delete older ones",
        &["backup_clean_up"],
        |cx, r| cx.on_off(r, |s| &mut s.backup_clean_up),
    ),
    block(
        Page::Audio,
        "Status",
        "Now",
        "What plays now: the output, its latency and underruns, key to sound, the input",
        &[],
        crate::settings::settings_view::audio_now,
    )
    .status(),
    block(
        Page::Audio,
        "Output",
        "Output",
        "Driver, device, sample rate, buffer size, exclusive mode and the input device",
        &["audio_host", "audio_device", "audio_sample_rate", "audio_buffer", "audio_exclusive", "audio_input"],
        crate::settings::settings_view::audio_output,
    ),
    block(
        Page::Audio,
        "Cue",
        "Cue",
        "Where previews and cued tracks play, such as headphones, and how loud",
        &["audio_cue_pair", "audio_cue_volume"],
        crate::settings::settings_view::audio_cue,
    ),
    block(
        Page::Midi,
        "MIDI inputs",
        "MIDI inputs",
        "Each MIDI controller's port, on or off; the ports on play the armed tracks",
        &["midi_input_off"],
        crate::settings::settings_view::midi_inputs,
    ),
    row(
        Page::Midi,
        "Arming",
        "Arm the track you select",
        "Selecting a track arms it for recording, so your MIDI keyboard plays that track's instrument; tracks you armed by hand stay armed",
        &["auto_arm"],
        |cx, r| cx.on_off(r, |s| &mut s.auto_arm),
    ),
    block(
        Page::Plugins,
        "Actions",
        "Plug-in actions",
        "Scan the folders for new or changed plug-ins, scan everything again, or open the plug-in manager that lists them all",
        &[],
        crate::settings::settings_view::plugin_buttons,
    )
    .actions(),
    block(
        Page::Plugins,
        "Folders",
        "Plug-in library",
        "The folders plug-ins are looked for in, each on or off and for a format, scanning them, and the plug-in manager",
        &[],
        crate::settings::settings_view::plugin_library,
    ),
    row(
        Page::Plugins,
        "Editors",
        "Editors",
        "In a window above Suhina, or inside the device panel as part of the chain",
        &["plugin_editors"],
        |cx, r| {
            if !cx.about.editors_embeddable {
                cx.note("Editors need Windows or an X server (DISPLAY); they do not open on this system yet");
            }
            let options = [("Window", EditorPlacement::Window), ("Device view", EditorPlacement::DeviceView)];
            cx.choose(r, &options, |s| &mut s.plugin_editors);
        },
    ),
    row(
        Page::Devices,
        "NAM",
        "NAM models folder",
        "Where the NAM Amp Modeler lists .nam models ([docs/spec/nam-amp-and-cab.md])",
        &["nam_models_folder"],
        |cx, r| {
            let default = cx.about.nam_models_default.clone();
            cx.folder(r, &default, crate::interaction::pick::PickField::NamModelsFolder, |s| &mut s.nam_models_folder);
        },
    ),
    row(
        Page::Devices,
        "NAM",
        "IR folder",
        "Where the NAM Cab Loader lists .wav impulse responses",
        &["irs_folder"],
        |cx, r| {
            let default = cx.about.irs_default.clone();
            cx.folder(r, &default, crate::interaction::pick::PickField::IrFolder, |s| &mut s.irs_folder);
        },
    ),
    row(
        Page::Devices,
        "Device panels",
        "Device chain",
        "In a row under the view, or stacked in a column beside it",
        &["device_chain"],
        |cx, r| {
            let options = [("Row", DeviceLayout::Row), ("Stack", DeviceLayout::Stack)];
            cx.choose(r, &options, |s| &mut s.device_layout);
        },
    ),
    row(
        Page::Devices,
        "Device panels",
        "Detail area layout",
        "Beside the browser, or across the whole window under it",
        &["detail_layout"],
        |cx, r| {
            let options = [("Beside", DetailLayout::Beside), ("Full width", DetailLayout::FullWidth)];
            cx.choose(r, &options, |s| &mut s.view.detail_layout);
        },
    ),
    row(
        Page::Importing,
        "Dropping files",
        "When you drop several files",
        "Put them all on one track or give each its own track; hold Ctrl while dropping to do the other",
        &["drop_files"],
        |cx, r| cx.choose(r, &[("One track", false), ("One each", true)], |s| &mut s.drop_separate),
    ),
    row(
        Page::Importing,
        "Dropping files",
        "Warp mode for dropped samples",
        "How a dropped sample that has a known tempo is fitted to the project's tempo; Re-pitch changes its pitch with the speed, the others keep the pitch",
        &["warp_mode"],
        |cx, r| {
            let options: Vec<(&str, WarpMode)> = WarpMode::ALL.iter().map(|m| (m.title(), *m)).collect();
            cx.choose(r, &options, |s| &mut s.warp_mode);
        },
    ),
    row(
        Page::Arrangement,
        "Clip editing",
        "Let audio clips overlap",
        "Audio clips moved over other clips keep playing together instead of replacing what they cover",
        &["sample_stacking"],
        |cx, r| cx.on_off(r, |s| &mut s.sample_stacking),
    ),
    row(
        Page::Arrangement,
        "Clip editing",
        "Let MIDI clips overlap",
        "MIDI clips moved over other clips keep playing together instead of replacing what they cover",
        &["midi_stacking"],
        |cx, r| cx.on_off(r, |s| &mut s.midi_stacking),
    ),
    row(
        Page::Arrangement,
        "Clip editing",
        "Double-click to make a clip",
        "Double-clicking an empty slot or an empty spot in a lane creates a new clip there",
        &["double_click_clip"],
        |cx, r| cx.on_off(r, |s| &mut s.view.double_click_clip),
    ),
    row(
        Page::Arrangement,
        "Clip editing",
        "Rename in the clip menu",
        "A clip's right-click menu includes its name to edit; off, Rename (Ctrl+R) edits the name on the clip itself",
        &["menu_name_field"],
        |cx, r| cx.on_off(r, |s| &mut s.view.menu_name_field),
    ),
    row(
        Page::Arrangement,
        "Clip editing",
        "Grab a clip by",
        "Choose where you grab a clip to move it, the name bar at its top or its lower half, while the rest of the clip selects time",
        &["clip_drag"],
        |cx, r| cx.choose(r, &[("Name bar", false), ("Mirror", true)], |s| &mut s.view.clip_halves),
    ),
    row(
        Page::Arrangement,
        "Time and play",
        "Select time with Ctrl+drag",
        "On, a plain drag moves clips and Ctrl+drag selects time; off, dragging the upper half of a clip selects time and the lower half moves it",
        &["time_select"],
        |cx, r| cx.on_off(r, |s| &mut s.view.time_select_ctrl),
    ),
    row(
        Page::Arrangement,
        "Time and play",
        "Move the start marker to a clip you select",
        "While stopped, selecting a whole clip puts the play start marker at its start; off, a click puts it where you clicked",
        &["play_from_selected_clip"],
        |cx, r| cx.on_off(r, |s| &mut s.view.play_from_clip),
    ),
    row(
        Page::Arrangement,
        "Time and play",
        "Play starts at",
        "The play start marker, or the start of the clip selected in the Arrangement view",
        &["play_start"],
        |cx, r| {
            let options = [("Marker", PlayFrom::Marker), ("Selected clip", PlayFrom::Clip)];
            cx.choose(r, &options, |s| &mut s.play_from);
        },
    ),
    row(
        Page::Arrangement,
        "Time and play",
        "When you press Stop",
        "Jump back to the play start marker, or stay where playback stopped; a second press goes to the start",
        &["stop"],
        |cx, r| cx.choose(r, &[("Marker", StopReturn::Marker), ("Stay", StopReturn::Here)], |s| &mut s.stop),
    ),
    row(
        Page::Arrangement,
        "View",
        "Show the grid inside clips",
        "The grid's lines show faintly through clips, so you can see where beats and bars fall on them",
        &["clip_grid"],
        |cx, r| cx.on_off(r, |s| &mut s.view.grid_in_clips),
    ),
    row(
        Page::Arrangement,
        "View",
        "Waveform size",
        "Draw audio clips' waveforms larger so quiet parts are easier to see, without changing the sound; Ctrl+Alt+Up and Ctrl+Alt+Down change it too",
        &["waveform_zoom"],
        |cx, r| cx.choose(r, &[("×1", 100), ("×2", 200), ("×4", 400), ("×8", 800)], |s| &mut s.view.waveform_zoom),
    ),
    row(
        Page::Arrangement,
        "Tracks",
        "Colour of new tracks",
        "Alternating gives each new track the next palette colour and fixed gives every new track the first one, while existing tracks keep theirs",
        &["new_track_colour"],
        |cx, r| cx.choose(r, &[("Alternating", false), ("Fixed", true)], |s| &mut s.view.fixed_track_colour),
    ),
    row(
        Page::Arrangement,
        "Tracks",
        "Track headers",
        "Which side of the timeline they are on",
        &["track_headers"],
        |cx, r| cx.choose(r, &[("Left", Side::Left), ("Right", Side::Right)], |s| &mut s.track_headers),
    ),
    row(
        Page::Arrangement,
        "Tracks",
        "Show return tracks",
        "Show the return tracks' rows along the bottom of the Arrangement; the Hide returns button by the ruler does the same",
        &["arrangement_returns"],
        |cx, r| cx.choose(r, &[("Shown", false), ("Hidden", true)], |s| &mut s.view.returns_hidden),
    ),
    row(
        Page::Arrangement,
        "Advanced",
        "Stretch audio by its edge",
        "Dragging an audio clip's edge with Ctrl and Alt held stretches the audio to the new length and keeps its pitch; off, it only moves the edge",
        &["stretch_alt_drag"],
        |cx, r| cx.on_off(r, |s| &mut s.view.stretch_alt),
    ),
    row(
        Page::Arrangement,
        "Advanced",
        "Ctrl+R renames every selected clip",
        "With several clips selected, the name typed after Ctrl+R goes to all of them, as one undo step; off, only the clip clicked last is renamed",
        &["rename_selected_clips"],
        |cx, r| cx.on_off(r, |s| &mut s.view.rename_selected),
    ),
    row(
        Page::Session,
        "Launching",
        "Show the clip you launch",
        "Starting a clip in the Session view also selects it, so its settings open in the detail area below",
        &["select_on_launch"],
        |cx, r| cx.on_off(r, |s| &mut s.select_on_launch),
    ),
    row(
        Page::Session,
        "Mixer",
        "Volume and pan controls",
        "Show each track's volume and pan as number fields or as knobs; the mouse wheel changes either",
        &["mixer_controls"],
        |cx, r| {
            let options = [("Field", MixerControls::Field), ("Knob", MixerControls::Knob)];
            cx.choose(r, &options, |s| &mut s.mixer_controls);
        },
    ),
    row(
        Page::ClipEditor,
        "Piano roll",
        "When the piano roll is pulled to the top",
        "What stays visible above the piano roll when you drag its divider all the way up: only the edited track, or the whole window",
        &["piano_roll_top"],
        |cx, r| {
            let options = [("Edited track", PianoRollTop::Track), ("Whole window", PianoRollTop::Window)];
            cx.choose(r, &options, |s| &mut s.piano_roll_top);
        },
    ),
    row(
        Page::ClipEditor,
        "Piano roll",
        "Piano roll grid size",
        "Use the piano roll's own grid setting, or follow the one in the control bar",
        &["piano_roll_grid"],
        |cx, r| cx.choose(r, &[("Its own", false), ("Control bar", true)], |s| &mut s.piano_grid_follows),
    ),
    row(
        Page::ClipEditor,
        "Piano roll",
        "Toolbar size",
        "Make the piano roll's toolbar buttons and icons smaller or larger without enlarging the rest of the editor",
        &["piano_roll_toolbar"],
        |cx, r| {
            let options: Vec<(&str, ToolbarSize)> = ToolbarSize::ALL.iter().map(|s| (s.name(), *s)).collect();
            cx.choose(r, &options, |s| &mut s.toolbar_size);
        },
    ),
    row(
        Page::ClipEditor,
        "Piano roll",
        "Moved notes replace notes",
        "A note moved onto another of the same pitch and start takes its place; off, both stay and play",
        &["notes_replace"],
        |cx, r| cx.on_off(r, |s| &mut s.view.notes_replace),
    ),
    row(
        Page::Browser,
        "Browser",
        "Browser side",
        "Which side of the window the browser is on",
        &["browser_side"],
        |cx, r| cx.choose(r, &[("Left", Side::Left), ("Right", Side::Right)], |s| &mut s.browser_side),
    ),
    row(
        Page::Browser,
        "Browser",
        "Recent items layout",
        "Show the kinds of recent items (samples, MIDI and so on) under Recent in the sidebar, or as buttons above the list",
        &["browser_recent"],
        |cx, r| cx.choose(r, &[("Indented", false), ("Chips", true)], |s| &mut s.view.recent_chips),
    ),
    row(
        Page::Browser,
        "Previews",
        "Auto-play",
        "Selecting a sample or MIDI file previews it",
        &["preview_autoplay"],
        |cx, r| cx.on_off(r, |s| &mut s.preview_autoplay),
    ),
    row(
        Page::Browser,
        "Previews",
        "Preview volume",
        "How loud browser and sample-map previews play",
        &["preview_volume"],
        |cx, r| {
            let options = [("-18 dB", -18), ("-12 dB", -12), ("-6 dB", -6), ("0 dB", 0)];
            cx.choose(r, &options, |s| &mut s.preview_volume);
        },
    ),
    row(
        Page::Browser,
        "Sample map",
        "Map layout",
        "The sample map as a free cloud or snapped to a grid",
        &["map_layout"],
        |cx, r| cx.choose(r, &[("Cloud", false), ("Grid", true)], |s| &mut s.map_grid),
    ),
    row(
        Page::Browser,
        "Sample map",
        "Map preview on hover",
        "Pointing at a sample on the map previews it",
        &["map_hover_preview"],
        |cx, r| cx.on_off(r, |s| &mut s.map_hover_preview),
    ),
    row(
        Page::Browser,
        "Sample map",
        "Map: longest sample",
        "Samples longer than this are left off the sample map",
        &["map_max_seconds"],
        |cx, r| {
            let options = [("5 s", 5u32), ("10 s", 10), ("30 s", 30), ("60 s", 60)];
            cx.choose(r, &options, |s| &mut s.map_max_seconds);
        },
    ),
    row(Page::Browser, "Tags", "Most colour tags", "How many colour tags you can create in the browser", &[], |cx, r| {
        let options = [("64", 64usize), ("128", 128), ("256", 256), ("1024", 1024)];
        cx.tag_limit(r, &options);
    }),
    block(
        Page::Keys,
        "Mouse and key behaviour",
        "Mouse and key behaviour in the piano roll",
        "How clicks, double-clicks, right-clicks and the duplicate key work when editing notes; changing one switches the workflow preset to Custom",
        KEYMAP,
        crate::settings::settings_view::mouse_switches,
    ),
    block(
        Page::Keys,
        "Shortcuts",
        "Shortcuts",
        "Every action's keys and the preset they come from",
        KEYMAP,
        crate::settings::settings_view::shortcuts_block,
    ),
    row(
        Page::Appearance,
        "Window",
        "Zoom",
        "The size of the whole window; Ctrl and + or -, Ctrl and 0 for 100 %",
        &["zoom"],
        |cx, r| cx.zoom(r),
    ),
    row(
        Page::Appearance,
        "Window",
        "Animations",
        "Short fades and slides when things change",
        &["animations"],
        |cx, r| cx.on_off(r, |s| &mut s.animations),
    ),
    row(
        Page::Appearance,
        "Waveforms",
        "Waveform scale",
        "Linear, or decibels so quiet parts show; clips, previews and the map",
        &["waveform_scale"],
        |cx, r| cx.choose(r, &[("Linear", false), ("dB", true)], |s| &mut s.waveform_db),
    ),
    row(
        Page::Appearance,
        "Advanced",
        "Graphics system",
        "Choose GPU or OpenGL for drawing Suhina; it applies after the next start, and OpenGL can resize more smoothly on some systems",
        &["renderer"],
        |cx, r| {
            if cx.about.graphics_now.is_some_and(|now| now != cx.settings.graphics) {
                cx.note("Applies the next time Suhina starts");
            }
            cx.choose(r, &[("GPU", Graphics::Gpu), ("OpenGL", Graphics::OpenGl)], |s| &mut s.graphics);
        },
    ),
    block(
        Page::Theme,
        "Theme",
        "Theme",
        "The themes to pick from, the theme in use and its colours",
        &["theme"],
        crate::settings::settings_view::theme_block,
    ),
    row(
        Page::Agents,
        "Agents",
        "Let an AI agent edit open projects",
        "An agent started with suhina-cli mcp --attach can work on your open projects once you allow it; you see every change and can undo it",
        &["agents"],
        |cx, r| cx.on_off(r, |s| &mut s.agents),
    ),
    row(
        Page::Agents,
        "MCP write permissions",
        "Allow the project's folder",
        "Includes subfolders; turn off to revoke this default permission",
        &["mcp_project_folder"],
        |cx, r| {
            let note = cx.about.mcp_project_folder.as_deref().unwrap_or("No saved project folder").to_string();
            cx.note(format!("{note} (including subfolders)"));
            cx.on_off(r, |s| &mut s.mcp_project_folder);
        },
    ),
    block(
        Page::Agents,
        "MCP write permissions",
        "MCP allowed write folders",
        "Folders an agent may write to through MCP, each with its subfolders; File > Export is not limited by them",
        &["mcp_write_folders"],
        |cx, y| cx.mcp_write_folders(y),
    ),
    row(
        Page::Agents,
        "Updates",
        "Check for updates",
        "Once per start, asks GitHub whether a newer build is out; sends nothing about you",
        &["check_updates"],
        |cx, r| cx.on_off(r, |s| &mut s.updates.check),
    ),
    row(
        Page::Agents,
        "Updates",
        "Update source",
        "The nightly builds, or in developer mode also a pull request's test build",
        &["update_channel"],
        |cx, r| {
            if cx.settings.updates.test_builds_listed() {
                let options = [("Nightly", Channel::Nightly), ("Test builds", Channel::TestBuilds)];
                cx.choose(r, &options, |s| &mut s.updates.channel);
            } else {
                cx.choose(r, &[("Nightly", Channel::Nightly)], |s| &mut s.updates.channel);
            }
        },
    ),
    row(
        Page::Agents,
        "Advanced",
        "Developer mode",
        "Offer pull requests' test builds as an update source; downloading one needs the GitHub CLI",
        &["developer_mode"],
        |cx, r| cx.on_off(r, |s| &mut s.updates.developer),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    /// Kept in `settings.json` but changed outside the Settings window: the browser and the map
    /// window as they were left.
    const NOT_IN_THE_WINDOW: [&str; 7] = [
        "browser",
        "browser_width",
        "map_dot_size",
        "map_window",
        "map_window_rect",
        "session_mixer_height",
        "session_strip_fader",
    ];

    fn saved_keys() -> Vec<String> {
        let settings = Settings { midi_inputs_off: vec!["Keys".into()], ..Settings::default() };
        settings
            .to_text()
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once('=').map(|(key, _)| key.trim().to_string()))
            .collect()
    }

    #[test]
    fn every_saved_setting_is_on_exactly_one_page() {
        for key in saved_keys() {
            let on: Vec<&str> = ROWS.iter().filter(|e| e.keys.contains(&key.as_str())).map(|e| e.name).collect();
            if NOT_IN_THE_WINDOW.contains(&key.as_str()) {
                assert!(on.is_empty(), "{key} is changed outside the window, yet shown by {on:?}");
            } else {
                assert_eq!(on.len(), 1, "{key} is shown by {on:?}");
            }
        }
        // And every key an entry names is one the settings file has, or the keymap.
        let saved = saved_keys();
        for entry in ROWS {
            for key in entry.keys {
                assert!(saved.iter().any(|k| k == key) || KEYMAP.contains(key), "{}: no saved key {key}", entry.name);
            }
        }
    }

    #[test]
    fn every_entry_has_a_name_of_its_own_and_its_sections_stay_together() {
        let mut names: Vec<&str> = ROWS.iter().map(|e| e.name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two entries share a name");
        // A page's sections and the pages themselves come in one run each, in the list's order.
        let mut seen: Vec<(Page, &str)> = Vec::new();
        for entry in ROWS {
            let here = (entry.page, entry.section);
            if seen.last() != Some(&here) {
                assert!(!seen.contains(&here), "{} › {} is split in two", entry.page.name(), entry.section);
                seen.push(here);
            }
        }
        let order: Vec<Page> = Page::ALL.iter().map(|(page, _)| *page).collect();
        let mut pages: Vec<Page> = seen.iter().map(|(page, _)| *page).collect();
        pages.dedup();
        assert_eq!(pages, order, "every page has settings, in the order of the list");
    }

    const RULES: &str = "docs/spec/ui-settings.md > Adding a setting";

    #[test]
    fn every_entry_has_a_page_a_group_and_a_one_sentence_description() {
        for entry in ROWS {
            assert!(!entry.name.trim().is_empty(), "an entry has no name ({RULES})");
            assert!(!entry.section.trim().is_empty(), "{}: no group ({RULES})", entry.name);
            assert!(!entry.note.trim().is_empty(), "{}: no description ({RULES})", entry.name);
            let sentences = [". ", "? ", "! "].iter().any(|end| entry.note.contains(end)) || entry.note.ends_with('.');
            assert!(!sentences, "{}: the description is more than one sentence ({RULES})", entry.name);
        }
    }

    #[test]
    fn a_group_outside_advanced_has_at_most_seven_entries() {
        for entry in ROWS.iter().filter(|e| e.section != ADVANCED) {
            let count = ROWS.iter().filter(|e| e.page == entry.page && e.section == entry.section).count();
            assert!(
                count <= 7,
                "{} > {} has {count} entries; split it or move the rare ones under {ADVANCED} ({RULES})",
                entry.page.name(),
                entry.section
            );
        }
    }

    #[test]
    fn advanced_is_last_on_its_page_and_status_and_commands_are_set_off_at_the_top() {
        for (page, _) in Page::ALL {
            let on_page: Vec<&Entry> = ROWS.iter().filter(|e| e.page == page).collect();
            if let Some(first) = on_page.iter().position(|e| e.section == ADVANCED) {
                assert!(
                    on_page[first..].iter().all(|e| e.section == ADVANCED),
                    "{}: {ADVANCED} is not last ({RULES})",
                    page.name()
                );
            }
            let mut seen_setting = false;
            for entry in on_page {
                if entry.role == Role::Setting {
                    seen_setting = true;
                } else {
                    assert!(
                        !seen_setting,
                        "{}: {} is a status or a command among settings; put it first in its own group ({RULES})",
                        page.name(),
                        entry.name
                    );
                }
            }
        }
    }

    #[test]
    fn a_search_finds_a_setting_by_a_word_of_its_description_or_its_group() {
        let by = |word: &str| -> Vec<&str> {
            let words = vec![word.to_lowercase()];
            ROWS.iter().filter(|e| e.matches_words(&words)).map(|e| e.name).collect()
        };
        assert!(by("headphones").contains(&"Cue"), "a word of the description");
        assert!(by("previews").contains(&"Auto-play"), "the group's name");
    }
}
