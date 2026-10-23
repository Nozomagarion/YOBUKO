// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Theme files: one folder per theme in the `themes` folder beside the settings file, named by the
//! theme's id, holding `theme.css` (the whole theme as CSS; ADR 0031), an imported `advanced.css`,
//! its artwork and README. A file saved while Suhina runs is read again within a second, so a
//! theme can be edited in any text editor and seen right away. An older theme written as JSON
//! (`night-blue.json` beside a folder of CSS and artwork) is migrated once: the CSS is written,
//! read back and compared, and only then are the JSON and its old `style.css` moved into the
//! theme's `legacy/` folder; nothing is deleted. Backups and the theme template are zips written to
//! the `exports` folder; theme files and zips dropped on the window are brought in.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use suhina_ui::kit::theme::{self, Theme};
use suhina_ui::settings::is_theme_id;
use suhina_ui::settings::themes::{ThemeEntry, slug};

/// Larger files are not themes; they are skipped instead of read.
const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_EXPANDED: u64 = 32 * 1024 * 1024;
/// A zip with more files than this is not a set of themes.
const MAX_ZIP_FILES: usize = 512;
/// How often the folder is looked at for changed files.
const CHECK_EVERY: Duration = Duration::from_secs(1);
/// The theme template's name in its zip; it is not brought in when the zip is dropped.
const TEMPLATE: &str = "template.json";
/// A theme's one source: `<id>/theme.css`.
const THEME_CSS: &str = "theme.css";
/// Where a migrated theme keeps its JSON and old canonical CSS.
const LEGACY: &str = "legacy";
/// The folder of the template zip that holds the built-in themes as examples.
const EXAMPLES: &str = "examples/";

/// A theme's source text (theme.css and what it imports) as last read or written, so a save
/// never overwrites an edit made outside Suhina since.
#[derive(Clone)]
struct Snapshot {
    sources: String,
}

pub struct ThemeFiles {
    folder: Option<PathBuf>,
    /// When each file was last changed, as last read or written.
    seen: HashMap<String, u64>,
    checked: Option<Instant>,
    snapshots: HashMap<String, Snapshot>,
}

impl ThemeFiles {
    pub fn new(folder: Option<PathBuf>) -> Self {
        Self { folder, seen: HashMap::new(), checked: None, snapshots: HashMap::new() }
    }

    fn refresh_default_assets(&self, warnings: &mut Vec<String>) {
        if let Some(folder) = &self.folder
            && let Err(error) = read_default_assets(folder, warnings)
        {
            warnings.push(format!("Default SVG library: {error}"));
        }
    }

    /// Reads the theme `id` at `path`: its `theme.css`, or a JSON theme, which is migrated.
    fn load_synced(&mut self, id: &str, path: &Path, warnings: &mut Vec<String>) -> Option<ThemeEntry> {
        let result = (|| -> Result<Theme, String> {
            if is_css_theme(path) {
                // Called when the folder changed: its CSS, or only its artwork.
                let sources = read_sources(path)?;
                let (theme, problems) = read_css_theme(path)?;
                warnings.extend(problems.into_iter().map(|p| format!("{id}: {p}")));
                self.snapshots.insert(id.into(), Snapshot { sources });
                return Ok(theme);
            }
            let (theme, problems) = read_loose(path)?;
            warnings.extend(problems.into_iter().map(|p| format!("{id}: {p}")));
            let folder = self.folder.as_ref().ok_or("no themes folder")?;
            match migrate(folder, id, &theme) {
                Ok(css) => {
                    let sources = read_sources(&css)?;
                    self.snapshots.insert(id.into(), Snapshot { sources });
                }
                Err(error) => warnings.push(format!("{id}: kept as JSON, not migrated to CSS: {error}")),
            }
            Ok(theme)
        })();
        match result {
            Ok(theme) => Some(ThemeEntry { id: id.into(), theme }),
            Err(error) => {
                warnings.push(format!("{id}: {error}"));
                None
            }
        }
    }

    /// The folder as the Settings page shows it.
    pub fn folder_text(&self) -> Option<String> {
        self.folder.as_ref().map(|f| f.display().to_string())
    }

    /// Where backups and the template go: `exports` beside the themes folder.
    fn exports(&self) -> Result<PathBuf, String> {
        self.folder
            .as_ref()
            .and_then(|f| f.parent())
            .map(|p| p.join("exports"))
            .ok_or_else(|| "there is no folder to keep themes in".to_string())
    }

    /// Reads every theme file, in the order of their names, with warnings about the ones that
    /// could not be read whole.
    pub fn read_all(&mut self) -> (Vec<ThemeEntry>, Vec<String>) {
        self.seen.clear();
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        self.refresh_default_assets(&mut warnings);
        for (id, path, modified) in self.list() {
            if let Some(entry) = self.load_synced(&id, &path, &mut warnings) {
                entries.push(entry);
            }
            let path = self.current_path(&id, path);
            self.seen.insert(id, fingerprint(&path).unwrap_or(modified));
        }
        (entries, warnings)
    }

    /// Where the theme `id` is now: its `theme.css` once a JSON theme has been migrated.
    fn current_path(&self, id: &str, path: PathBuf) -> PathBuf {
        let css = self.folder.as_ref().map(|folder| folder.join(id).join(THEME_CSS));
        css.filter(|css| css.is_file()).unwrap_or(path)
    }

    /// Files added or changed since they were last read, at most once every second.
    pub fn changed(&mut self) -> (Vec<ThemeEntry>, Vec<String>) {
        let now = Instant::now();
        if self.checked.is_some_and(|at| now.duration_since(at) < CHECK_EVERY) {
            return (Vec::new(), Vec::new());
        }
        self.checked = Some(now);
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        self.refresh_default_assets(&mut warnings);
        for (id, path, modified) in self.list() {
            if self.seen.get(&id) == Some(&modified) {
                continue;
            }
            if let Some(entry) = self.load_synced(&id, &path, &mut warnings) {
                entries.push(entry);
            }
            let path = self.current_path(&id, path);
            self.seen.insert(id, fingerprint(&path).unwrap_or(modified));
        }
        (entries, warnings)
    }

    /// Writes the theme `id` to its file.
    pub fn write(&mut self, id: &str, theme: &Theme) -> Result<(), String> {
        suhina_plugins::trace::timed("theme save", || self.write_inner(id, theme))
    }

    fn write_inner(&mut self, id: &str, theme: &Theme) -> Result<(), String> {
        let folder = self.folder.as_ref().ok_or("there is no folder to keep themes in")?;
        if !is_theme_id(id) {
            return Err(format!("{id} cannot name a theme file"));
        }
        let path = folder.join(id).join(THEME_CSS);
        if let Some(snapshot) = self.snapshots.get(id)
            && path.exists()
        {
            let sources = suhina_plugins::trace::timed("theme save check", || read_sources(&path))?;
            if sources != snapshot.sources {
                return Err(format!(
                    "Theme {id} has external edits. Reload themes before saving; its files were not overwritten."
                ));
            }
        }
        suhina_plugins::trace::timed("theme package persist", || persist_css_theme(folder, id, theme))?;
        let sources = suhina_plugins::trace::timed("theme save readback", || read_sources(&path))?;
        self.snapshots.insert(id.into(), Snapshot { sources });
        if let Ok(modified) = fingerprint(&path) {
            self.seen.insert(id.to_string(), modified);
        }
        Ok(())
    }

    /// Renames the theme `from` to `to`, after the theme was renamed: its folder, and a JSON file
    /// not migrated yet. A theme not saved before is fine: it is written under its new name next.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        let folder = self.folder.as_ref().ok_or("there is no folder to keep themes in")?;
        if !is_theme_id(from) || !is_theme_id(to) {
            return Err(format!("{from} or {to} cannot name a theme file"));
        }
        let pairs = [
            (folder.join(from), folder.join(to)),
            (folder.join(format!("{from}.json")), folder.join(format!("{to}.json"))),
        ];
        if pairs.iter().all(|(old, _)| !old.exists()) {
            return Ok(());
        }
        if !from.eq_ignore_ascii_case(to) && pairs.iter().any(|(old, new)| old.exists() && new.exists()) {
            return Err(format!("there is a theme {to} already"));
        }
        for (old, new) in pairs.iter().filter(|(old, _)| old.exists()) {
            std::fs::rename(old, new).map_err(|e| format!("could not rename {from}: {e}"))?;
        }
        self.seen.remove(from);
        self.snapshots.remove(from);
        if let Some((_, _, modified)) = self.list().into_iter().find(|(id, _, _)| id == to) {
            self.seen.insert(to.to_string(), modified);
        }
        Ok(())
    }

    /// Writes every theme file into `exports/suhina-themes-<date>.zip`. Returns its path.
    pub fn backup(&self) -> Result<PathBuf, String> {
        let files = self.list();
        if files.is_empty() {
            return Err("there are no theme files to back up yet".into());
        }
        let mut contents = vec![("README.md".into(), theme::template_readme().into_bytes())];
        for (id, path, _) in files {
            let (theme, _) = read_any(&path)?;
            contents.extend(portable_package_files(&id, &theme)?);
        }
        let name = format!("suhina-themes-{}.zip", date(SystemTime::now()));
        write_zip(&self.exports()?.join(name), &contents)
    }

    /// Writes `exports/suhina-theme-template.zip`: `template.json` (Suhina Dark with every
    /// colour written out), `README.md` on the format, and the built-in themes in `examples/`.
    /// Returns its path.
    pub fn export_template(&self) -> Result<PathBuf, String> {
        let mut template = Theme { name: "My theme".into(), ..Theme::default() };
        template.skin.assets = suhina_ui::kit::skin::inherited_assets();
        template.skin.toolbar = Some(Default::default());
        let mut contents = vec![
            ("README.md".to_string(), theme::template_readme().into_bytes()),
            (TEMPLATE.to_string(), template.to_json().into_bytes()),
        ];
        for (id, theme) in theme::built_in_themes() {
            for (name, data) in package_files(id, &theme)? {
                if !name.ends_with("/README.md") && !name.contains("/examples/") {
                    contents.push((format!("{EXAMPLES}{name}"), data));
                }
            }
        }
        contents.extend(portable_package_files("my-theme", &template)?);
        write_zip(&self.exports()?.join("suhina-theme-template.zip"), &contents)
    }

    /// Brings in a theme file or every theme in a zip (a backup, say; the template's own files
    /// are left out): each is written to the themes folder under an id no theme has yet, unless
    /// the same theme is there already. Returns the ids written and warnings.
    pub fn import(&mut self, path: &Path) -> Result<(Vec<String>, Vec<String>), String> {
        let is_zip = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
        let mut found = Vec::new();
        let mut warnings = Vec::new();
        if is_zip {
            for (name, theme, problems) in read_zip(path)? {
                warnings.extend(problems.into_iter().map(|p| format!("{name}: {p}")));
                found.push((stem(&name), theme));
            }
        } else {
            let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            if std::fs::metadata(path).map_err(|e| format!("{name}: {e}"))?.len() > MAX_FILE {
                return Err(format!("{name} is larger than a theme can be"));
            }
            let (theme, problems) = read_any(path)?;
            if problems.iter().any(|p| p.contains("not JSON") || p.contains("JSON object")) {
                return Err(format!("{name} is not a theme: {}", problems.join("; ")));
            }
            warnings.extend(problems.into_iter().map(|p| format!("{name}: {p}")));
            found.push((stem(&name), theme));
        }
        let (existing, _) = self.read_all();
        let mut written = Vec::new();
        for (stem, theme) in found {
            if existing.iter().any(|e| e.theme == theme) {
                continue;
            }
            let base = slug(if stem.is_empty() { &theme.name } else { &stem });
            let taken = |id: &str| {
                theme::is_built_in(id)
                    || existing.iter().chain(written.iter()).any(|e: &ThemeEntry| e.id.eq_ignore_ascii_case(id))
            };
            let mut id = base.clone();
            let mut number = 2;
            while taken(&id) {
                id = format!("{base}-{number}");
                number += 1;
            }
            self.write(&id, &theme)?;
            written.push(ThemeEntry { id, theme });
        }
        Ok((written.into_iter().map(|e| e.id).collect(), warnings))
    }

    /// Shows the themes folder in the system's file manager, made first if it is not there.
    pub fn open_folder(&self) -> Result<(), String> {
        let folder = self.folder.as_ref().ok_or("there is no folder to keep themes in")?;
        std::fs::create_dir_all(folder).map_err(|e| format!("could not make {}: {e}", folder.display()))?;
        reveal(folder, false)
    }

    /// Each theme in the folder: a `<id>/theme.css`, or a `<id>.json` not migrated yet; its id,
    /// path and when it last changed, by name. A folder with `theme.css` wins over a JSON file.
    fn list(&self) -> Vec<(String, PathBuf, u64)> {
        let Some(Ok(dir)) = self.folder.as_ref().map(std::fs::read_dir) else { return Vec::new() };
        let mut files: Vec<(String, PathBuf, u64)> = Vec::new();
        for entry in dir.filter_map(Result::ok).take(4096) {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            let found = if meta.is_dir() {
                let css = path.join(THEME_CSS);
                let file = std::fs::symlink_metadata(&css).is_ok_and(|m| m.is_file());
                path.file_name().and_then(|n| n.to_str()).filter(|_| file).map(|id| (id.to_owned(), css))
            } else if meta.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
                path.file_stem().and_then(|n| n.to_str()).map(|id| (id.to_owned(), path.clone()))
            } else {
                None
            };
            let Some((id, path)) = found.filter(|(id, _)| is_theme_id(id)) else { continue };
            let modified = fingerprint(&path).unwrap_or(0);
            match files.iter_mut().find(|(other, _, _)| *other == id) {
                Some(existing) if is_css_theme(&path) => *existing = (id, path, modified),
                Some(_) => {}
                None => files.push((id, path, modified)),
            }
        }
        files.sort_by_key(|f| f.0.to_lowercase());
        files
    }
}

/// Shows `path` in the system's file manager: the folder, or the file selected where the system
/// can do that.
pub fn reveal(path: &Path, select: bool) -> Result<(), String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("explorer");
        if select {
            c.arg(format!("/select,{}", path.display()));
        } else {
            c.arg(path);
        }
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        if select {
            c.arg("-R");
        }
        c.arg(path);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(if select { path.parent().unwrap_or(path) } else { path });
        c
    };
    command.spawn().map(|_| ()).map_err(|e| format!("could not show {}: {e}", path.display()))
}

/// A theme's name in a package path: the file's stem, or the folder of a `theme.css`.
fn stem(name: &str) -> String {
    let mut parts = name.rsplit(['/', '\\']);
    let file = parts.next().unwrap_or(name);
    if file == THEME_CSS {
        return parts.next().unwrap_or_default().to_string();
    }
    file.strip_suffix(".json").unwrap_or(file).to_string()
}

fn is_css_theme(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == THEME_CSS)
}

/// A theme file of either kind: a `theme.css` (or any `.css` theme dropped on the window) or JSON.
fn read_any(path: &Path) -> Result<(Theme, Vec<String>), String> {
    if path.extension().is_some_and(|e| e == "css") { read_css_theme(path) } else { read_loose(path) }
}

/// The text a theme is made of: `theme.css` and the stylesheets it imports, for noticing edits.
fn read_sources(path: &Path) -> Result<String, String> {
    use suhina_ui::kit::theme_css;
    let folder = path.parent().unwrap_or(Path::new("."));
    let text = String::from_utf8(read_bounded(path, theme_css::MAX_CSS as u64)?).map_err(|e| e.to_string())?;
    let mut sources = text.clone();
    for import in theme_css::imports(&text) {
        let css = read_resource(folder, &import, theme_css::MAX_CSS as u64)?;
        sources.push_str("\n/* imported */\n");
        sources.push_str(&String::from_utf8(css).map_err(|e| e.to_string())?);
    }
    Ok(sources)
}

/// A `theme.css` on disk, with its import and artwork relative to its folder.
fn read_css_theme(path: &Path) -> Result<(Theme, Vec<String>), String> {
    let text =
        String::from_utf8(read_bounded(path, suhina_ui::kit::theme_css::MAX_CSS as u64)?).map_err(|e| e.to_string())?;
    let folder = path.parent().unwrap_or(Path::new("."));
    resolve_css(&text, |relative, max| read_resource(folder, relative, max))
}

/// A theme from its `theme.css` text: the canonical rules, at most one imported advanced
/// stylesheet, and artwork and background read through `resource`.
fn resolve_css(
    text: &str,
    mut resource: impl FnMut(&str, u64) -> Result<Vec<u8>, String>,
) -> Result<(Theme, Vec<String>), String> {
    use suhina_ui::kit::theme_css;
    let mut theme = Theme::default();
    let warnings = theme_css::apply(&mut theme, text);
    let imports = theme_css::imports(text);
    if imports.len() > 1 {
        return Err("a theme imports at most one advanced stylesheet".into());
    }
    if let Some(import) = imports.first() {
        let css = String::from_utf8(resource(import, theme_css::MAX_CSS as u64)?).map_err(|e| e.to_string())?;
        suhina_ui::kit::native_css::inspect(&css)?;
        theme.skin.css = (!css.is_empty()).then(|| std::sync::Arc::from(css));
    }
    resolve_resources(&mut theme, &mut resource)?;
    Ok((theme, warnings))
}

/// Replaces artwork and background paths by their contents, within the package limits.
fn resolve_resources(
    theme: &mut Theme,
    resource: &mut impl FnMut(&str, u64) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    use suhina_ui::kit::skin;
    if let Some(background) = &theme.skin.background
        && !background.starts_with("data:image/png;base64,")
    {
        let bytes = resource(background, skin::MAX_PNG as u64)?;
        theme.skin.background = Some(skin::encode_png(&bytes)?);
    }
    let mut artwork_bytes = 0usize;
    for (role, source) in &mut theme.skin.assets {
        if !source.trim_start().starts_with('<') && !skin::png_artwork(source) {
            let png = skin::artwork_extension(source) == "png";
            let data = resource(source, if png { skin::MAX_PNG as u64 } else { skin::MAX_SVG as u64 })?;
            *source = if png {
                skin::encode_png(&data)?.to_string()
            } else {
                String::from_utf8(data).map_err(|e| e.to_string())?
            };
        }
        artwork_bytes = artwork_bytes
            .checked_add(source.len())
            .filter(|total| *total <= skin::MAX_ARTWORK_BYTES)
            .ok_or("combined artwork exceeds 16 MiB")?;
        skin::validate_artwork(source).map_err(|e| format!("{role}: {e}"))?;
    }
    Ok(())
}

/// Migrates the JSON theme `id`, already read as `theme`, to `<id>/theme.css`, keeping the
/// comments of its old `style.css`. The CSS is read back and must give the same theme; only then
/// are the JSON and old `style.css` moved into `<id>/legacy/`. On a failure the new `theme.css`
/// is removed and the JSON stays as it was. Returns the new `theme.css`.
fn migrate(folder: &Path, id: &str, theme: &Theme) -> Result<PathBuf, String> {
    use suhina_ui::kit::theme_css;
    let css = folder.join(id).join(THEME_CSS);
    if css.exists() {
        return Err("the theme already has a theme.css".into());
    }
    let style = folder.join(id).join("style.css");
    let existing = if style.is_file() {
        let bytes = read_resource(folder, &format!("{id}/style.css"), theme_css::MAX_CSS as u64)?;
        Some(String::from_utf8(bytes).map_err(|e| e.to_string())?)
    } else {
        None
    };
    write_files(folder, css_files(id, theme, existing.as_deref())?)?;
    let checked = read_css_theme(&css).and_then(|(migrated, problems)| {
        if migrated != *theme {
            Err("the CSS does not read back as the same theme".to_string())
        } else if !problems.is_empty() {
            Err(problems.join("; "))
        } else {
            Ok(())
        }
    });
    if let Err(error) = checked {
        let _ = std::fs::remove_file(&css);
        return Err(error);
    }
    let legacy = folder.join(id).join(LEGACY);
    std::fs::create_dir_all(&legacy).map_err(|e| e.to_string())?;
    let keep = |from: &Path, name: &str| -> Result<(), String> {
        let mut to = legacy.join(name);
        let mut number = 2;
        while to.exists() {
            to = legacy.join(format!("{number}-{name}"));
            number += 1;
        }
        std::fs::rename(from, to).map_err(|e| e.to_string())
    };
    if style.is_file() {
        keep(&style, "style.css")?;
    }
    keep(&folder.join(format!("{id}.json")), &format!("{id}.json"))?;
    Ok(css)
}

fn write_zip(path: &Path, contents: &[(String, Vec<u8>)]) -> Result<PathBuf, String> {
    let failed = |e: &dyn std::fmt::Display| format!("could not write {}: {e}", path.display());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| failed(&e))?;
    }
    let file = std::fs::File::create(path).map_err(|e| failed(&e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in contents {
        zip.start_file(name.as_str(), options).map_err(|e| failed(&e))?;
        zip.write_all(data).map_err(|e| failed(&e))?;
    }
    zip.finish().map_err(|e| failed(&e))?;
    Ok(path.to_path_buf())
}

/// The theme files in a zip, as names and text: every `.json` but the template's own, each at
/// most [`MAX_FILE`] long.
type LoadedTheme = (String, Theme, Vec<String>);

fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 240
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > max {
        return Err(format!("{} exceeds its theme size limit", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max {
        return Err("theme file grew beyond its size limit".into());
    }
    Ok(bytes)
}

fn read_loose(path: &Path) -> Result<(Theme, Vec<String>), String> {
    let text = String::from_utf8(read_bounded(path, MAX_FILE)?).map_err(|e| e.to_string())?;
    let root = path.parent().unwrap_or(Path::new("."));
    resolve_theme(&text, |relative, max| {
        if !safe_relative(relative) {
            return Err("invalid relative theme resource path".into());
        }
        let mut file = root.to_path_buf();
        for part in relative.split('/') {
            file.push(part);
            let meta = std::fs::symlink_metadata(&file).map_err(|e| format!("{relative}: {e}"))?;
            if meta.file_type().is_symlink() {
                return Err("theme resources may not be symlinks".into());
            }
        }
        read_bounded(&file, max)
    })
}

fn resolve_theme(
    text: &str,
    mut resource: impl FnMut(&str, u64) -> Result<Vec<u8>, String>,
) -> Result<(Theme, Vec<String>), String> {
    use suhina_ui::kit::theme_css;
    let root: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("invalid theme JSON: {e}"))?;
    if !root.is_object() {
        return Err("a theme must be a JSON object".into());
    }
    let (mut theme, mut warnings) = Theme::from_json(text);
    if let Some(path) = root.get("advanced-stylesheet") {
        let path = path
            .as_str()
            .filter(|p| safe_relative(p) && p.ends_with(".css"))
            .ok_or("advanced-stylesheet must name a relative CSS file")?;
        let css = String::from_utf8(resource(path, theme_css::MAX_CSS as u64)?).map_err(|e| e.to_string())?;
        suhina_ui::kit::native_css::inspect(&css)?;
        theme.skin.css = (!css.is_empty()).then(|| std::sync::Arc::from(css));
    }
    if let Some(path) = root.get("stylesheet") {
        let path = path
            .as_str()
            .filter(|p| safe_relative(p) && p.ends_with(".css"))
            .ok_or("stylesheet must name a relative CSS file")?;
        let css = String::from_utf8(resource(path, theme_css::MAX_CSS as u64)?).map_err(|e| e.to_string())?;
        warnings.extend(theme_css::apply(&mut theme, &css));
        if let Some((folder, _)) = path.rsplit_once('/') {
            let mut css_assets = Theme::default();
            theme_css::apply(&mut css_assets, &css);
            if let Some(background) = css_assets.skin.background {
                theme.skin.background = Some(std::sync::Arc::from(format!("{folder}/{background}")));
            }
            for role in css_assets.skin.assets.keys() {
                if let Some(source) = theme.skin.assets.get_mut(role) {
                    *source = format!("{folder}/{source}");
                }
            }
        }
    }
    resolve_resources(&mut theme, &mut resource)?;
    Ok((theme, warnings))
}

/// The import line that brings a theme's advanced stylesheet into its `theme.css`.
const ADVANCED_IMPORT: &str = "@import \"advanced.css\";";

/// The files of the theme `id` as CSS: `<id>/theme.css`, its advanced stylesheet, background,
/// artwork, README and examples. `existing` is the theme's current `theme.css` (or, while
/// migrating, its old `style.css`), whose comments and formatting are kept outside the values
/// that changed.
fn css_files(id: &str, theme: &Theme, existing: Option<&str>) -> Result<Vec<(String, Vec<u8>)>, String> {
    use suhina_ui::kit::{skin, theme_css};
    theme
        .skin
        .assets
        .values()
        .try_fold(0usize, |total, source| total.checked_add(source.len()))
        .filter(|total| *total <= skin::MAX_ARTWORK_BYTES)
        .ok_or("combined artwork exceeds 16 MiB")?;
    let mut files = Vec::new();
    if let Some(css) = &theme.skin.css {
        suhina_ui::kit::native_css::inspect(css)?;
        if css.len() > theme_css::MAX_CSS {
            return Err("advanced CSS exceeds 128 KiB".into());
        }
        files.push((format!("{id}/advanced.css"), css.as_bytes().to_vec()));
    }
    if let Some(background) = &theme.skin.background {
        files.push((format!("{id}/background.png"), skin::decode_png(background)?));
    }
    for (role, source) in &theme.skin.assets {
        if !skin::valid_role(role) {
            return Err(format!("unknown SVG role {role}"));
        }
        skin::validate_artwork(source)?;
        let relative = format!("assets/{role}.{}", skin::artwork_extension(source));
        let data = if skin::png_artwork(source) { skin::decode_png(source)? } else { source.as_bytes().to_vec() };
        files.push((format!("{id}/{relative}"), data));
    }
    let generated = theme_css::export(theme);
    let mut text = match existing {
        Some(source) => {
            let source = source.replace(ADVANCED_IMPORT, "");
            let mut previous = Theme::default();
            let warnings = theme_css::apply(&mut previous, &source);
            if !warnings.is_empty() {
                return Err(format!("Theme CSS was preserved: {}", warnings.join("; ")));
            }
            theme_css::source::update(&source, &theme_css::export(&previous), &generated)?
        }
        None => generated,
    };
    if theme.skin.css.is_some() {
        text = format!("{ADVANCED_IMPORT}\n{}", text.trim_start_matches('\n'));
    }
    files.push((format!("{id}/{THEME_CSS}"), text.into_bytes()));
    files.push((format!("{id}/README.md"), theme::template_readme().into_bytes()));
    files.push((format!("{id}/examples/dark-gloss.css"), theme::GLOSS_EXAMPLE.as_bytes().to_vec()));
    files.push((format!("{id}/examples/compact-session.css"), theme::COMPACT_SESSION_EXAMPLE.as_bytes().to_vec()));
    Ok(files)
}

/// A theme as a package: the same files a theme folder holds.
fn package_files(id: &str, theme: &Theme) -> Result<Vec<(String, Vec<u8>)>, String> {
    css_files(id, theme, None)
}

/// Writes the theme `id` to `<id>/theme.css` and the files beside it, keeping the comments and
/// formatting of the `theme.css` there and an installed README.
fn persist_css_theme(folder: &Path, id: &str, theme: &Theme) -> Result<(), String> {
    use suhina_ui::kit::theme_css;
    let relative = format!("{id}/{THEME_CSS}");
    let existing = if folder.join(&relative).is_file() {
        let bytes = read_resource(folder, &relative, theme_css::MAX_CSS as u64)?;
        Some(String::from_utf8(bytes).map_err(|e| e.to_string())?)
    } else {
        None
    };
    write_files(folder, css_files(id, theme, existing.as_deref())?)
}

/// Writes `files` under `folder` atomically, `theme.css` last, leaving unchanged files and an
/// installed README alone.
fn write_files(folder: &Path, mut files: Vec<(String, Vec<u8>)>) -> Result<(), String> {
    // theme.css commits the snapshot last. Temp files live beside their destinations for atomic renames.
    files.sort_by_key(|(name, _)| name.ends_with(THEME_CSS));
    for (name, data) in files {
        if !safe_relative(&name) {
            return Err("invalid theme destination".into());
        }
        let path = folder.join(&name);
        let mut current = folder.to_path_buf();
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        for component in name.split('/') {
            current.push(component);
            if std::fs::symlink_metadata(&current).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("theme destination may not be a symlink".into());
            }
        }
        // Installed author guides belong to their package version. New packages
        // get the current guide; editing a colour must not replace a user's copy.
        if name.ends_with("/README.md") && path.is_file() {
            continue;
        }
        // Keep unchanged artwork and documentation in place. In particular, forcing
        // dozens of identical SVGs to disk made each theme slider tick take 100+ ms.
        if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file() && meta.len() == data.len() as u64)
            && read_bounded(&path, data.len() as u64).is_ok_and(|existing| existing == data)
        {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        if temporary.exists() {
            return Err(format!("unfinished theme write: {}", temporary.display()));
        }
        use std::fs::OpenOptions;
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&data).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// A portable theme carries its effective artwork; local files retain sparse overrides.
fn portable_package_files(id: &str, theme: &Theme) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut portable = theme.clone();
    let mut assets = suhina_ui::kit::skin::inherited_assets();
    for role in suhina_ui::kit::skin::ROLES {
        if theme.skin.assets.contains_key(*role) {
            for suffix in [".hover", ".active", ".disabled"] {
                assets.remove(&format!("{role}{suffix}"));
            }
        }
    }
    assets.extend(theme.skin.assets.clone());
    portable.skin.assets = assets;
    package_files(id, &portable)
}

fn read_default_assets(folder: &Path, warnings: &mut Vec<String>) -> Result<(), String> {
    use suhina_ui::kit::skin;
    let mut assets = skin::default_assets();
    let mut edited = Vec::new();
    let mut invalid = Vec::new();
    let mut directory = folder.to_path_buf();
    for component in ["_defaults", "assets"] {
        directory.push(component);
        if std::fs::symlink_metadata(&directory).is_ok_and(|meta| meta.file_type().is_symlink() || !meta.is_dir()) {
            return Err("default SVG directories must be ordinary directories".into());
        }
    }
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let backup = folder.join("_defaults/suhina-default.zip");
    let mut pristine = Theme { name: "Suhina Default".into(), ..Default::default() };
    pristine.skin.assets = assets.clone();
    match std::fs::symlink_metadata(&backup) {
        Ok(meta) if !meta.file_type().is_file() => return Err("default backup must be an ordinary file".into()),
        Ok(_) => {
            if !read_zip(&backup).is_ok_and(|entries| entries.len() == 1 && entries[0].1 == pristine) {
                let saved = (1..=9999)
                    .map(|n| folder.join(format!("_defaults/suhina-default-backup-recovery-{n}.zip")))
                    .find(|p| std::fs::symlink_metadata(p).is_err())
                    .ok_or("too many backup recovery files")?;
                std::fs::rename(&backup, &saved).map_err(|e| e.to_string())?;
                write_zip(&backup, &package_files("suhina-default", &pristine)?)?;
                warnings
                    .push(format!("Changed default backup preserved in {}; pristine backup restored", saved.display()));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_zip(&backup, &package_files("suhina-default", &pristine)?)?;
        }
        Err(error) => return Err(error.to_string()),
    }
    for (role, source) in skin::DEFAULT_SVGS {
        let path = directory.join(format!("{role}.svg"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => file.write_all(source.as_bytes()).map_err(|e| e.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    for role in skin::ROLES {
        for suffix in ["", ".hover", ".active", ".disabled"] {
            let key = format!("{role}{suffix}");
            let path = directory.join(format!("{key}.svg"));
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    warnings.push(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            let result: Result<String, String> = (|| {
                if !metadata.file_type().is_file() {
                    return Err("default SVG must be an ordinary file".into());
                }
                let source =
                    String::from_utf8(read_bounded(&path, skin::MAX_SVG as u64)?).map_err(|e| e.to_string())?;
                skin::parse_svg(&source)?;
                Ok(source)
            })();
            match result {
                Ok(source) => {
                    if assets.get(&key) != Some(&source) {
                        edited.push((key.clone(), path, source.clone()));
                        assets.insert(key, source);
                    }
                }
                Err(error) => {
                    warnings.push(format!("{}: {error}; using embedded default", path.display()));
                    if metadata.file_type().is_file()
                        && let Ok(bytes) = read_bounded(&path, skin::MAX_SVG as u64)
                    {
                        invalid.push((key, path, bytes));
                    }
                }
            }
        }
    }
    if !invalid.is_empty() {
        let exports = folder.parent().ok_or("no theme recovery folder")?.join("exports");
        let recovery = (1..=9999)
            .map(|n| exports.join(format!("suhina-default-recovery-{n}.zip")))
            .find(|p| std::fs::symlink_metadata(p).is_err())
            .ok_or("too many default recovery archives")?;
        let contents: Vec<_> =
            invalid.iter().map(|(key, _, bytes)| (format!("assets/{key}.svg"), bytes.clone())).collect();
        write_zip(&recovery, &contents)?;
        for (key, path, bytes) in invalid {
            if !restore_default_asset(&path, &key, &bytes)? {
                warnings.push("Default SVG changed again during recovery; newer edit was kept".into());
            }
        }
        warnings
            .push(format!("Malformed default SVG edits preserved in {} before restoring defaults", recovery.display()));
    }
    if !edited.is_empty() {
        let number = (1..=9999)
            .find(|number| {
                let id = format!("suhina-default-modified-{number}");
                std::fs::symlink_metadata(folder.join(format!("{id}.json"))).is_err()
                    && std::fs::symlink_metadata(folder.join(&id)).is_err()
            })
            .ok_or("too many modified default themes")?;
        let id = format!("suhina-default-modified-{number}");
        let mut modified = Theme { name: format!("Suhina Default (modified {number})"), ..Default::default() };
        modified.skin.assets = assets;
        // Commit the user's copy first. A failure leaves every edited original intact.
        persist_css_theme(folder, &id, &modified)?;
        for (key, path, source) in edited {
            if !restore_default_asset(&path, &key, source.as_bytes())? {
                warnings.push("Default SVG changed again during recovery; newer edit was kept".into());
            }
        }
        warnings.push(format!("Default SVG edits saved as {} ({id}); original defaults restored", modified.name));
    }
    Ok(())
}

fn restore_default_asset(path: &Path, key: &str, expected: &[u8]) -> Result<bool, String> {
    use suhina_ui::kit::skin;
    if !std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file()) {
        return Ok(false);
    }
    if read_bounded(path, skin::MAX_SVG as u64)? != expected {
        return Ok(false);
    }
    if let Some((_, original)) = skin::DEFAULT_SVGS.iter().find(|(role, _)| *role == key) {
        let temporary = path.with_extension(format!("restore-{}", std::process::id()));
        let mut file =
            std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|e| e.to_string())?;
        file.write_all(original.as_bytes()).map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(temporary, path).map_err(|e| e.to_string())?;
    } else {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(true)
}

fn read_zip(path: &Path) -> Result<Vec<LoadedTheme>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if archive.len() > MAX_ZIP_FILES {
        return Err("too many theme package files".into());
    }
    let mut files = std::collections::BTreeMap::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().map_err(|e| e.to_string())?.to_string();
        if !safe_relative(&name) || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(format!("unsafe theme package path {name}"));
        }
        let limit = if name.ends_with(".svg") {
            suhina_ui::kit::skin::MAX_SVG as u64
        } else if name.ends_with(".png") {
            suhina_ui::kit::skin::MAX_PNG as u64
        } else if name.ends_with(".css") {
            suhina_ui::kit::theme_css::MAX_CSS as u64
        } else {
            MAX_FILE
        };
        total = total.checked_add(entry.size()).ok_or("theme package size overflow")?;
        if entry.size() > limit || total > MAX_EXPANDED {
            return Err("theme package exceeds expanded size limit".into());
        }
        let mut bytes = Vec::new();
        (&mut entry).take(limit + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > limit {
            return Err("theme package entry exceeds size limit".into());
        }
        if files.insert(name, bytes).is_some() {
            return Err("duplicate theme package path".into());
        }
    }
    let mut themes = Vec::new();
    // A package holds CSS themes (`<id>/theme.css`) or, from older versions, JSON themes; a JSON
    // theme in a folder that also has a theme.css is that theme's legacy copy.
    let css_folders: std::collections::BTreeSet<&str> = files
        .keys()
        .filter_map(|name| name.strip_suffix(THEME_CSS))
        .map(|folder| folder.trim_end_matches('/'))
        .collect();
    for (name, bytes) in &files {
        let css = name.ends_with(&format!("/{THEME_CSS}")) || name == THEME_CSS;
        let json = name.ends_with(".json")
            && name != TEMPLATE
            && !name.contains(&format!("/{LEGACY}/"))
            && !css_folders.contains(name.strip_suffix(".json").unwrap_or_default());
        if (!css && !json) || name.starts_with(EXAMPLES) {
            continue;
        }
        let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        let folder = name.rsplit_once('/').map_or("", |(folder, _)| folder);
        let resource = |relative: &str, max: u64| {
            if !safe_relative(relative) {
                return Err("invalid relative resource path".to_string());
            }
            let path = if folder.is_empty() { relative.to_string() } else { format!("{folder}/{relative}") };
            let data = files.get(&path).ok_or_else(|| format!("missing theme resource {path}"))?;
            if data.len() as u64 > max {
                return Err(format!("{path} exceeds resource size limit"));
            }
            Ok(data.clone())
        };
        let (theme, warnings) = if css { resolve_css(text, resource)? } else { resolve_theme(text, resource)? };
        themes.push((name.clone(), theme, warnings));
    }
    if themes.is_empty() {
        return Err("there are no themes in this package".into());
    }
    Ok(themes)
}

/// `YYYY-MM-DD` of `time`, in UTC.
fn date(time: SystemTime) -> String {
    let days = time.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400) as i64;
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("suhina-theme-files-{name}-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&dir);
        dir
    }

    fn night() -> Theme {
        let mut theme = Theme { name: "Night".into(), ..Theme::default() };
        theme.colors.accent = suhina_ui::kit::theme::parse_hex("#123456").unwrap_or_default();
        theme
    }

    #[test]
    fn themes_are_written_read_and_seen_when_they_change() {
        let dir = folder("round-trip");
        let mut files = ThemeFiles::new(Some(dir.join("themes")));
        assert_eq!(files.read_all(), (Vec::new(), Vec::new()));
        assert_eq!(files.write("night", &night()), Ok(()));
        assert!(files.write("../night", &night()).is_err());
        // Written here, so not read back as a change.
        assert!(files.changed().0.is_empty());

        let themes = dir.join("themes");
        std::fs::write(themes.join("b-bad.json"), "{ \"colors\": { \"accent\": \"red\" } }").unwrap_or_default();
        std::fs::write(themes.join("notes.txt"), "not a theme").unwrap_or_default();
        let (entries, warnings) = files.read_all();
        let ids: Vec<_> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["b-bad", "night"]);
        assert_eq!(entries.get(1).map(|e| &e.theme), Some(&night()));
        assert_eq!(warnings.len(), 1, "{warnings:?}");

        files.checked = None;
        std::fs::write(themes.join("c-new.json"), "{ \"name\": \"New\" }").unwrap_or_default();
        let (changed, _) = files.changed();
        assert_eq!(changed.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["c-new"]);
        let _clean = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_renamed_theme_keeps_its_file() {
        let dir = folder("rename");
        let mut files = ThemeFiles::new(Some(dir.join("themes")));
        assert_eq!(files.rename("never-saved", "other"), Ok(()));
        assert_eq!(files.write("night", &night()), Ok(()));
        assert_eq!(files.write("day", &Theme::default()), Ok(()));
        assert!(files.rename("night", "day").is_err());
        assert_eq!(files.rename("night", "deep-sea"), Ok(()));
        let ids: Vec<_> = files.read_all().0.into_iter().map(|e| e.id).collect();
        assert_eq!(ids, ["day", "deep-sea"]);
        let _clean = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_backup_brings_the_themes_back() {
        let dir = folder("backup");
        let mut files = ThemeFiles::new(Some(dir.join("themes")));
        assert!(files.backup().is_err(), "nothing to back up yet");
        assert_eq!(files.write("night", &night()), Ok(()));
        let backup = files.backup().unwrap_or_default();
        assert!(backup.starts_with(dir.join("exports")), "{}", backup.display());

        // Portable backups materialize inherited artwork so another machine looks the same.
        let mut other = ThemeFiles::new(Some(dir.join("other")));
        assert_eq!(other.import(&backup), Ok((vec!["night".to_string()], Vec::new())));
        let mut restored = night();
        restored.skin.assets = suhina_ui::kit::skin::default_assets();
        assert_eq!(other.read_all().0, vec![ThemeEntry { id: "night".into(), theme: restored }]);
        // The same themes again are not doubled; a different one with a taken id gets its own.
        assert_eq!(other.import(&backup), Ok((Vec::new(), Vec::new())));
        let changed = Theme { name: "Night 2".into(), ..night() };
        let file = dir.join("night.json");
        std::fs::write(&file, changed.to_json()).unwrap_or_default();
        assert_eq!(other.import(&file).map(|r| r.0), Ok(vec!["night-2".to_string()]));
        let _clean = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_template_holds_a_theme_a_readme_and_examples() {
        let dir = folder("template");
        let files = ThemeFiles::new(Some(dir.join("themes")));
        let zip = files.export_template().unwrap_or_default();
        let file = std::fs::File::open(&zip).unwrap_or_else(|e| panic!("{e}"));
        let mut archive = zip::ZipArchive::new(file).unwrap_or_else(|e| panic!("{e}"));
        let mut names: Vec<String> = archive.file_names().filter_map(Result::ok).map(|n| n.to_string()).collect();
        names.sort();
        assert!(names.contains(&"README.md".to_string()) && names.contains(&TEMPLATE.to_string()));
        assert!(names.contains(&"examples/nord/theme.css".to_string()), "{names:?}");
        let mut template = String::new();
        archive
            .by_name(TEMPLATE)
            .unwrap_or_else(|e| panic!("{e}"))
            .read_to_string(&mut template)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(Theme::from_json(&template).1, Vec::<String>::new());
        // The editable example is importable; reference palettes remain examples only.
        let mut into = ThemeFiles::new(Some(dir.join("into")));
        let (ids, warnings) = into.import(&zip).unwrap();
        assert_eq!(ids, ["my-theme"]);
        assert!(warnings.is_empty());
        let _clean = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_is_not_a_theme_is_refused() {
        let dir = folder("refused");
        std::fs::create_dir_all(&dir).unwrap_or_default();
        let mut files = ThemeFiles::new(Some(dir.join("themes")));
        let huge = dir.join("huge.json");
        std::fs::write(&huge, vec![b' '; MAX_FILE as usize + 1]).unwrap_or_default();
        assert!(files.import(&huge).is_err());
        let list = dir.join("list.json");
        std::fs::write(&list, "[1, 2]").unwrap_or_default();
        assert!(files.import(&list).is_err());
        let fake = dir.join("fake.zip");
        std::fs::write(&fake, "not a zip").unwrap_or_default();
        assert!(files.import(&fake).is_err());
        assert!(files.read_all().0.is_empty());
        let _clean = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn huge_files_in_the_folder_are_skipped() {
        let dir = folder("huge");
        std::fs::create_dir_all(&dir).unwrap_or_default();
        std::fs::write(dir.join("huge.json"), vec![b' '; MAX_FILE as usize + 1]).unwrap_or_default();
        let (entries, warnings) = ThemeFiles::new(Some(dir.clone())).read_all();
        assert!(entries.is_empty());
        assert_eq!(warnings.len(), 1);
        let _clean = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_folder_nothing_is_read_or_written() {
        let mut files = ThemeFiles::new(None);
        assert_eq!(files.read_all(), (Vec::new(), Vec::new()));
        assert!(files.write("night", &Theme::default()).is_err());
        assert!(files.backup().is_err() && files.export_template().is_err() && files.open_folder().is_err());
        assert_eq!(files.folder_text(), None);
    }

    #[test]
    fn dates_are_written_year_first() {
        let day = |d: u64| date(SystemTime::UNIX_EPOCH + Duration::from_secs(d * 86_400));
        assert_eq!(day(0), "1970-01-01");
        assert_eq!(day(11_016), "2000-02-29");
        assert_eq!(day(20_738), "2026-10-12");
    }
}

fn read_resource(folder: &Path, relative: &str, max: u64) -> Result<Vec<u8>, String> {
    if !safe_relative(relative) {
        return Err("invalid relative theme path".to_string());
    }
    let mut path = folder.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        if std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?.file_type().is_symlink() {
            return Err("theme resources may not be symlinks".into());
        }
    }
    read_bounded(&path, max)
}

/// A bounded resource fingerprint catches CSS and SVG edits as well as JSON edits.
fn fingerprint(path: &Path) -> Result<u64, String> {
    use std::hash::{Hash, Hasher};
    let mut paths = vec![path.to_path_buf()];
    // A CSS theme is its folder; a JSON theme has its resources in the folder of its name.
    let folder = if is_css_theme(path) {
        path.parent().map_or_else(|| path.with_extension(""), Path::to_path_buf)
    } else {
        path.with_extension("")
    };
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.take(MAX_ZIP_FILES).filter_map(Result::ok) {
            let meta = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                continue;
            }
            paths.push(entry.path());
            if meta.is_dir()
                && let Ok(children) = std::fs::read_dir(entry.path())
            {
                paths.extend(
                    children
                        .take(suhina_ui::kit::skin::MAX_PACKAGE_ASSETS)
                        .filter_map(Result::ok)
                        .map(|child| child.path()),
                );
            }
        }
    }
    paths.sort();
    let mut hash = std::hash::DefaultHasher::new();
    for path in paths {
        let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        path.hash(&mut hash);
        meta.len().hash(&mut hash);
        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH).hash(&mut hash);
    }
    Ok(hash.finish())
}

#[cfg(test)]
mod skin_package_tests {
    #[test]
    fn editor_save_preserves_authored_css_comments_and_installed_guide() {
        let root = folder("authored-css-preservation");
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        let mut theme = Theme::default();
        files.write("authored", &theme).unwrap();
        let css_path = root.join("themes/authored/theme.css");
        let source = format!("/* Revontuli — keep my notes */\n{}", std::fs::read_to_string(&css_path).unwrap());
        std::fs::write(&css_path, &source).unwrap();
        let guide = root.join("themes/authored/README.md");
        std::fs::write(&guide, "Author's own guide").unwrap();
        let (_, warnings) = files.read_all();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(std::fs::read_to_string(&css_path).unwrap(), source);
        let before = theme::hex_rgba(theme.rgba("accent", theme.colors.accent));
        theme.colors.accent = eframe::egui::Color32::from_rgb(18, 52, 86);
        files.write("authored", &theme).unwrap();
        assert_eq!(
            std::fs::read_to_string(&css_path).unwrap(),
            source.replacen(&format!("--accent: {before}"), "--accent: #123456", 1)
        );
        assert_eq!(std::fs::read_to_string(&guide).unwrap(), "Author's own guide");
        let _clean = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn palette_edits_do_not_rewrite_unchanged_package_resources() {
        let root = folder("palette-resources");
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        let mut theme = theme::built_in_themes().into_iter().find(|(id, _)| *id == "suhina-sheet").unwrap().1;
        files.write("sheet", &theme).unwrap();
        let paths = ["themes/sheet/assets/button.svg", "themes/sheet/README.md", "themes/sheet/advanced.css"];
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(946_684_800);
        for path in paths {
            std::fs::File::options()
                .write(true)
                .open(root.join(path))
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(old))
                .unwrap();
        }
        theme.colors.accent = eframe::egui::Color32::RED;
        files.write("sheet", &theme).unwrap();
        for path in paths {
            assert_eq!(std::fs::metadata(root.join(path)).unwrap().modified().unwrap(), old, "rewrote {path}");
        }
        assert_eq!(files.read_all().0[0].theme.colors.accent, eframe::egui::Color32::RED);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "manual disk and parsing timing for the theme slider reproduction"]
    fn measure_theme_slider_saves() {
        let root = folder("slider-timing");
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        for (id, mut theme) in theme::built_in_themes()
            .into_iter()
            .filter(|(id, _)| ["suhina-dark", "suhina-sheet", "suhina-blocks", "suhina-meadow"].contains(id))
        {
            files.write(id, &theme).unwrap();
            for value in 30..35 {
                theme.colors.accent = eframe::egui::Color32::from_rgb(value, 60, 90);
                let start = Instant::now();
                files.write(id, &theme).unwrap();
                eprintln!("slider-save {id} {value}: {:.3} ms", start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNQ6nD5DwADrAHuJHH6EQAAAABJRU5ErkJggg==";

    #[test]
    fn png_control_artwork_round_trips_and_live_corruption_preserves_saved_theme() {
        let root = folder("png-artwork");
        let mut theme = styled();
        theme.skin.assets.insert("track.name.color-00ff00.frame-0".into(), PNG.into());
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        files.write("portable", &theme).unwrap();
        let png = root.join("themes/portable/assets/track.name.color-00ff00.frame-0.png");
        assert_eq!(std::fs::read(&png).unwrap(), skin::decode_png(PNG).unwrap());
        assert_eq!(files.read_all().0[0].theme, theme);
        let package = portable_package_files("portable", &theme).unwrap();
        assert!(package.iter().any(|(name, _)| name.ends_with("frame-0.png")));
        let zip = root.join("portable.zip");
        write_zip(&zip, &package).unwrap();
        let mut imported = ThemeFiles::new(Some(root.join("imported")));
        imported.import(&zip).unwrap();
        assert_eq!(imported.read_all().0[0].theme.skin.assets["track.name.color-00ff00.frame-0"], PNG);
        let replacement = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4H6r0HwAGQwJ299oQdAAAAABJRU5ErkJggg==";
        std::fs::write(&png, skin::decode_png(replacement).unwrap()).unwrap();
        files.checked = None;
        let (changed, warnings) = files.changed();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(changed[0].theme.skin.assets["track.name.color-00ff00.frame-0"], replacement);
        let css = root.join("themes/portable/theme.css");
        let saved = std::fs::read(&css).unwrap();
        std::fs::write(&png, b"\x89PNG\r\n\x1fcorrupt").unwrap();
        files.checked = None;
        let (changed, warnings) = files.changed();
        assert!(changed.is_empty());
        assert!(!warnings.is_empty());
        assert_eq!(std::fs::read(css).unwrap(), saved);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_animation_assets_leave_room_for_inherited_base_roles() {
        use super::*;
        let mut theme = Theme::default();
        let source = suhina_ui::kit::skin::default_assets()["button"].clone();
        for role in ["track.name", "track.body"] {
            for frame in 0..32 {
                theme.skin.assets.insert(format!("{role}.frame-{frame}"), source.clone());
            }
        }
        let files = portable_package_files("frames", &theme).unwrap();
        let (_, css) = files.iter().find(|(name, _)| name == "frames/theme.css").unwrap();
        let (restored, warnings) = resolve_css(std::str::from_utf8(css).unwrap(), |relative, _| {
            files
                .iter()
                .find(|(name, _)| name == &format!("frames/{relative}"))
                .map(|(_, data)| data.clone())
                .ok_or_else(|| format!("missing {relative}"))
        })
        .unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(restored.skin.assets.len(), suhina_ui::kit::skin::MAX_PACKAGE_ASSETS);
    }
    use super::*;
    use suhina_ui::kit::{skin, theme_css};
    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("suhina-theme-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }
    fn styled() -> Theme {
        let mut theme = Theme { name: "Portable".into(), ..Default::default() };
        theme.skin.assets = skin::example_assets();
        theme.skin.toolbar = Some(skin::Toolbar::default());
        theme.skin.toolbar_offsets.insert("transport".into(), 80);
        theme.skin.meter = skin::MeterStyle::Segmented;
        assert!(theme_css::apply(&mut theme, ".browser { color: #123456; background-color: #111222; }").is_empty());
        theme
    }
    #[test]
    fn deleted_default_library_and_backup_are_recreated_from_embedded_svg() {
        let root = folder("default-delete");
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        assert_eq!(files.read_all(), (Vec::new(), Vec::new()));
        let library = root.join("themes/_defaults");
        assert!(library.join("assets/fader.cap.svg").is_file());
        let backup = read_zip(&library.join("suhina-default.zip")).unwrap();
        assert_eq!(backup.len(), 1);
        assert_eq!(backup[0].1.skin.assets, skin::default_assets());
        std::fs::remove_dir_all(&library).unwrap();
        assert_eq!(files.read_all(), (Vec::new(), Vec::new()));
        assert!(library.join("suhina-default.zip").is_file());
        assert_eq!(std::fs::read_to_string(library.join("assets/play.svg")).unwrap(), skin::default_assets()["play"]);
        std::fs::write(library.join("suhina-default.zip"), "damaged backup").unwrap();
        let (_, warnings) = files.read_all();
        assert!(warnings.iter().any(|w| w.contains("pristine backup restored")));
        assert_eq!(std::fs::read(library.join("suhina-default-backup-recovery-1.zip")).unwrap(), b"damaged backup");
        assert_eq!(read_zip(&library.join("suhina-default.zip")).unwrap()[0].1.skin.assets, skin::default_assets());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn default_edits_fork_numbered_themes_before_restoring_original_artwork() {
        let root = folder("default-fork");
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        files.read_all();
        let play = root.join("themes/_defaults/assets/play.svg");
        let svg = |color| {
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"20\"><rect width=\"20\" height=\"20\" fill=\"{color}\"/></svg>"
            )
        };
        for (number, color) in [(1, "#123456"), (2, "#abcdef")] {
            std::fs::write(&play, svg(color)).unwrap();
            files.checked = None;
            let (entries, warnings) = files.changed();
            assert_eq!(entries.len(), 1, "{warnings:?}");
            assert_eq!(entries[0].id, format!("suhina-default-modified-{number}"));
            assert_eq!(entries[0].theme.name, format!("Suhina Default (modified {number})"));
            assert_eq!(entries[0].theme.skin.assets["play"], svg(color));
            assert_eq!(std::fs::read_to_string(&play).unwrap(), skin::default_assets()["play"]);
            assert!(warnings.iter().any(|w| w.contains("original defaults restored")));
        }
        files.checked = None;
        assert!(files.changed().0.is_empty(), "restoration must not create reload loops");
        assert_eq!(files.read_all().0.len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_default_svg_is_backed_up_before_restoring_the_embedded_source() {
        let root = folder("default-invalid");
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        files.read_all();
        let play = root.join("themes/_defaults/assets/play.svg");
        std::fs::write(&play, "unfinished SVG artwork").unwrap();
        files.checked = None;
        let (entries, warnings) = files.changed();
        assert!(entries.is_empty());
        assert!(warnings.iter().any(|w| w.contains("preserved in")));
        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(root.join("exports/suhina-default-recovery-1.zip")).unwrap())
                .unwrap();
        let mut saved = String::new();
        archive.by_name("assets/play.svg").unwrap().read_to_string(&mut saved).unwrap();
        assert_eq!(saved, "unfinished SVG artwork");
        assert_eq!(std::fs::read_to_string(play).unwrap(), skin::default_assets()["play"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_packages_include_the_shared_ai_author_guide_and_effective_svg() {
        let theme = Theme::default();
        let files = portable_package_files("plain", &theme).unwrap();
        let guide = files.iter().find(|(name, _)| name == "plain/README.md").unwrap();
        assert_eq!(guide.1, theme::template_readme().as_bytes());
        let text = std::str::from_utf8(&guide.1).unwrap();
        for heading in [
            "What this package is",
            "why they are separate",
            "How and when",
            "What's possible now",
            "Limits",
            "For AI-assisted edits",
        ] {
            assert!(text.contains(heading), "{heading}");
        }
        assert!(files.iter().any(|(name, _)| name == "plain/assets/fader.cap.svg"));
        assert!(
            files
                .iter()
                .any(|(name, data)| name == "plain/examples/dark-gloss.css" && data == theme::GLOSS_EXAMPLE.as_bytes())
        );
        assert!(theme.skin.assets.is_empty(), "export must not modify sparse local overrides");
        let css = files.iter().find(|(name, _)| name == "plain/theme.css").unwrap();
        assert!(std::str::from_utf8(&css.1).unwrap().contains("play { image: url(\"assets/play.svg\"); }"));
        assert!(!files.iter().any(|(name, _)| name.ends_with(".json")), "packages are CSS");
    }
    #[test]
    fn advanced_css_source_is_imported_verbatim_and_survives_packages() {
        let root = folder("advanced");
        let mut theme = styled();
        let source = "/* Keep my comment */\n@layer custom { :root { --ink: oklch(70% .1 240); } .browser { color:var(--ink); } }\n@media (width < 800px) { #performance { display:none; } }\n";
        theme.skin.css = Some(std::sync::Arc::from(source));
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        files.write("portable", &theme).unwrap();
        let css = root.join("themes/portable/advanced.css");
        assert_eq!(std::fs::read_to_string(&css).unwrap(), source);
        let main = std::fs::read_to_string(root.join("themes/portable/theme.css")).unwrap();
        assert!(main.starts_with("@import \"advanced.css\";\n"), "{main}");
        assert!(!root.join("themes/portable.json").exists(), "no JSON is written");
        let edited = source.replace("70%", "80%");
        std::fs::write(&css, &edited).unwrap();
        files.checked = None;
        let (changed, warnings) = files.changed();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(changed[0].theme.skin.css.as_deref(), Some(edited.as_str()));
        files.checked = None;
        assert!(files.changed().0.is_empty(), "reading must not trigger its own reload");
        let zip = files.backup().unwrap();
        let mut target = ThemeFiles::new(Some(root.join("imported")));
        assert!(target.import(&zip).unwrap().1.is_empty());
        assert_eq!(target.read_all().0[0].theme.skin.css.as_deref(), Some(edited.as_str()));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn external_theme_css_edits_load_and_editor_saves_keep_them_without_reload_loops() {
        let root = folder("css-source");
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        let original = styled();
        files.write("portable", &original).unwrap();
        let css = root.join("themes/portable/theme.css");
        let css_text = format!(
            "/* My notes stay. */\n{}",
            std::fs::read_to_string(&css)
                .unwrap()
                .replace("--accent: #A6A3FF;", "--accent: #12AB34;")
                .replace("intensity: 100%", "intensity: 65%")
                .replace("offset-transport: 80px", "offset-transport: 96px")
        );
        assert!(css_text.contains("--accent: #12AB34;"));
        std::fs::write(&css, &css_text).unwrap();
        files.checked = None;
        let (changed, warnings) = files.changed();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].theme.colors.accent, theme::parse_hex("#12AB34").unwrap());
        assert_eq!(changed[0].theme.skin.meter_intensity, 65);
        assert_eq!(changed[0].theme.skin.toolbar_offsets["transport"], 96);
        files.checked = None;
        assert!(files.changed().0.is_empty(), "reading must not trigger its own reload");
        let mut edited = changed[0].theme.clone();
        edited.colors.accent = theme::parse_hex("#112233").unwrap();
        edited.skin.meter_intensity = 80;
        files.write("portable", &edited).unwrap();
        let saved = std::fs::read_to_string(&css).unwrap();
        assert!(saved.starts_with("/* My notes stay. */\n"), "{saved}");
        assert!(saved.contains("--accent: #112233;") && saved.contains("intensity: 80%"));
        assert!(saved.contains("offset-transport: 96px"));
        assert_eq!(files.read_all().0[0].theme, edited);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn a_save_over_unread_external_edits_is_refused_and_keeps_the_files() {
        let root = folder("conflict");
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        let original = styled();
        files.write("portable", &original).unwrap();
        let css = root.join("themes/portable/theme.css");
        let external = std::fs::read_to_string(&css).unwrap().replace("--accent: #A6A3FF;", "--accent: #445566;");
        std::fs::write(&css, &external).unwrap();
        let mut ours = original.clone();
        ours.colors.accent = theme::parse_hex("#112233").unwrap();
        let error = files.write("portable", &ours).unwrap_err();
        assert!(error.contains("external edits"), "{error}");
        assert_eq!(std::fs::read_to_string(&css).unwrap(), external);
        // Once read, the external edit is the theme, and saving works again.
        let (entries, _) = files.read_all();
        assert_eq!(entries[0].theme.colors.accent, theme::parse_hex("#445566").unwrap());
        assert!(files.write("portable", &ours).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn a_json_theme_migrates_to_css_keeping_its_comments_and_its_originals() {
        let root = folder("migrate");
        let themes = root.join("themes");
        let mut theme = styled();
        theme.skin.css = Some(std::sync::Arc::from(".browser { color: #abcdef; }\n"));
        theme.liquid = Some(suhina_ui::kit::liquid::Options { panels: 30, tracks: 0, clips: 10 });
        // A theme as earlier versions wrote it: JSON beside a folder with style.css and artwork.
        std::fs::create_dir_all(themes.join("old/assets")).unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&theme.to_json()).unwrap();
        json["stylesheet"] = serde_json::json!("old/style.css");
        json["advanced-stylesheet"] = serde_json::json!("old/advanced.css");
        if let Some(skin) = json["skin"].as_object_mut() {
            skin.remove("css");
        }
        for (role, source) in &theme.skin.assets {
            std::fs::write(themes.join(format!("old/assets/{role}.svg")), source).unwrap();
            json["skin"]["assets"][role] = serde_json::json!(format!("old/assets/{role}.svg"));
        }
        let json = serde_json::to_vec_pretty(&json).unwrap();
        std::fs::write(themes.join("old.json"), &json).unwrap();
        let style = format!("/* Revontuli notes */\n{}", theme_css::export(&theme));
        std::fs::write(themes.join("old/style.css"), &style).unwrap();
        std::fs::write(themes.join("old/advanced.css"), ".browser { color: #abcdef; }\n").unwrap();
        let mut files = ThemeFiles::new(Some(themes.clone()));
        let (entries, warnings) = files.read_all();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].theme, theme);
        let css = std::fs::read_to_string(themes.join("old/theme.css")).unwrap();
        assert!(css.starts_with("@import \"advanced.css\";\n/* Revontuli notes */"), "{css}");
        assert!(!themes.join("old.json").exists() && !themes.join("old/style.css").exists());
        assert_eq!(std::fs::read(themes.join("old/legacy/old.json")).unwrap(), json);
        assert_eq!(std::fs::read_to_string(themes.join("old/legacy/style.css")).unwrap(), style);
        // Read again: the CSS theme is the same, and nothing migrates twice.
        let (again, warnings) = ThemeFiles::new(Some(themes.clone())).read_all();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(again[0].theme, theme);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn a_json_theme_that_cannot_round_trip_stays_json() {
        let root = folder("migrate-refused");
        let themes = root.join("themes");
        std::fs::create_dir_all(&themes).unwrap();
        // A JSON theme whose stylesheet has an invalid rule: CSS would lose it, so it is kept.
        let mut json: serde_json::Value = serde_json::from_str(&styled().to_json()).unwrap();
        json["stylesheet"] = serde_json::json!("kept/style.css");
        std::fs::create_dir_all(themes.join("kept")).unwrap();
        std::fs::write(themes.join("kept/style.css"), "meter { segments: 999; }").unwrap();
        std::fs::write(themes.join("kept.json"), serde_json::to_vec_pretty(&json).unwrap()).unwrap();
        let (entries, warnings) = ThemeFiles::new(Some(themes.clone())).read_all();
        assert_eq!(entries.len(), 1, "the theme still loads from JSON");
        assert!(warnings.iter().any(|w| w.contains("kept as JSON")), "{warnings:?}");
        assert!(themes.join("kept.json").exists() && !themes.join("kept/theme.css").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn example_builtins_export_their_css_svg_controls_and_background() {
        for id in ["suhina-sheet", "suhina-meadow", "suhina-blocks"] {
            let root = folder(id);
            let (_, theme) = theme::built_in_themes().into_iter().find(|(key, _)| *key == id).unwrap();
            let mut source = ThemeFiles::new(Some(root.join("source")));
            source.write("sheet-copy", &theme).unwrap();
            let zip = source.backup().unwrap();
            let mut target = ThemeFiles::new(Some(root.join("target")));
            let (ids, warnings) = target.import(&zip).unwrap();
            assert_eq!(ids, ["sheet-copy"]);
            assert!(warnings.is_empty(), "{warnings:?}");
            assert_eq!(target.read_all().0[0].theme, theme);
            let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
            for role in theme.skin.assets.keys() {
                assert!(archive.by_name(&format!("sheet-copy/assets/{role}.svg")).is_ok(), "{role}");
            }
            assert!(archive.by_name("sheet-copy/advanced.css").is_ok());
            assert!(archive.by_name("sheet-copy/theme.css").is_ok());
            assert!(archive.by_name("sheet-copy/README.md").is_ok());
            assert_eq!(archive.by_name("sheet-copy/background.png").is_ok(), id == "suhina-meadow");
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn package_round_trip_contains_svg_css_and_png() {
        let root = folder("package");
        let mut theme = styled();
        let image = suhina_ui::kit::skin::encode_png(b"\x89PNG\r\n\x1a\nfake-data-for-portability-only").unwrap();
        theme.skin.background = Some(image);
        // Package storage validates the PNG signature; the image decoder separately validates its full contents.
        let mut source = ThemeFiles::new(Some(root.join("source")));
        source.write("portable", &theme).unwrap();
        let zip = source.backup().unwrap();
        let mut target = ThemeFiles::new(Some(root.join("target")));
        let (ids, warnings) = target.import(&zip).unwrap();
        assert_eq!(ids, ["portable"]);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(target.read_all().0[0].theme, theme);
        let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        assert!(archive.by_name("portable/theme.css").is_ok());
        assert!(archive.by_name("portable/assets/fader.cap.svg").is_ok());
        assert!(archive.by_name("portable/background.png").is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn invalid_package_does_not_write_any_themes() {
        let root = folder("invalid");
        let zip = root.join("bad.zip");
        write_zip(
            &zip,
            &[("good/good.json".into(), styled().to_json().into_bytes()), ("bad/bad.json".into(), b"{".to_vec())],
        )
        .unwrap();
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        assert!(files.import(&zip).is_err());
        assert!(files.read_all().0.is_empty());
        let zip = root.join("traversal.zip");
        write_zip(&zip, &[("../escape.json".into(), styled().to_json().into_bytes())]).unwrap();
        assert!(files.import(&zip).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn live_svg_edit_is_loaded_and_missing_svg_does_not_overwrite_the_theme() {
        let root = folder("svg");
        let mut files = ThemeFiles::new(Some(root.join("themes")));
        files.write("portable", &styled()).unwrap();
        let svg = root.join("themes/portable/assets/fader.cap.svg");
        let replacement = skin::example_assets()["stop"].clone();
        std::fs::write(&svg, &replacement).unwrap();
        files.checked = None;
        let (changed, warnings) = files.changed();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(changed[0].theme.skin.assets["fader.cap"], replacement);
        let css = root.join("themes/portable/theme.css");
        let saved = std::fs::read(&css).unwrap();
        std::fs::remove_file(svg).unwrap();
        files.checked = None;
        let (changed, warnings) = files.changed();
        assert!(changed.is_empty());
        assert!(!warnings.is_empty());
        assert_eq!(std::fs::read(&css).unwrap(), saved);
        std::fs::remove_dir_all(root).unwrap();
    }
}
