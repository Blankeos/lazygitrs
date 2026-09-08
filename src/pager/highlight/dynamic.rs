//! User-installed grammars — new languages without rebuilding.
//!
//! Layout under `<config>/syntax/` (usually `~/.config/lazygitrs/syntax/`):
//! ```text
//! grammars/<lang>.so              compiled grammar exporting `tree_sitter_<lang>`
//! queries/<lang>/highlights.scm   required (copy from nvim-treesitter or Helix)
//! queries/<lang>/injections.scm   optional
//! queries/<lang>/locals.scm       optional
//! languages.yml                   optional extension/filename/shebang overrides
//! ```
//! Drop files in, open a diff — they load automatically on next highlight.
//! `?` → `Syntax highlighting health…` shows status; `Reload syntax grammars`
//! retries after fixing errors.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use tree_sitter_highlight::HighlightConfiguration;

use super::config::HIGHLIGHT_NAMES;

const README: &str = r#"# Extra syntax highlighting

Most languages already work. These need extra files:
Swift, C#, C++, PHP, Ruby.

Easiest way: open `?` in lazygitrs, pick "Syntax highlighting...",
then press Enter on the language. It downloads and sets it up.

Any other language: add 2 files per language under this folder:

  grammars/<name>.so
  queries/<name>/highlights.scm

Example for Zig:
  grammars/zig.so
  queries/zig/highlights.scm

Where do the files come from?
- The .so is a compiled tree-sitter grammar for that language.
  Search "<language> tree-sitter grammar", build it, and copy
  the .so here. It must export a `tree_sitter_<name>` symbol.
- The highlights.scm is a color file. Copy it from the
  nvim-treesitter or Helix project for that language.

It then shows up in `?` → "Syntax highlighting..." under Ready.
If the extension is unusual (not just .<name>), add languages.yml:

  zig:
    extensions: [zig, zon]
"#;

const EXAMPLE_YML: &str = r#"# Only needed when the extension is not just the language name.
# zig:
#   extensions: [zig, zon]
"#;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct LangOverride {
    extensions: Option<Vec<String>>,
    filenames: Option<Vec<String>>,
    filename_prefixes: Option<Vec<String>>,
    shebang: Option<Vec<String>>,
}

/// Built-in detection defaults for the heavy grammars we ship as dynamic.
/// (Static langs are matched first, so `.h` stays with C.)
struct KnownDynamic {
    name: &'static str,
    extensions: &'static [&'static str],
    filenames: &'static [&'static str],
    filename_prefixes: &'static [&'static str],
    shebang: &'static [&'static str],
}

const KNOWN_DYNAMIC: &[KnownDynamic] = &[
    KnownDynamic {
        name: "cpp",
        extensions: &["cpp", "hpp", "cc", "hh", "cxx", "hxx"],
        filenames: &[],
        filename_prefixes: &[],
        shebang: &[],
    },
    KnownDynamic {
        name: "c-sharp",
        extensions: &["cs", "csx"],
        filenames: &[],
        filename_prefixes: &[],
        shebang: &[],
    },
    KnownDynamic {
        name: "php",
        extensions: &["php", "phtml"],
        filenames: &[],
        filename_prefixes: &[],
        shebang: &["php"],
    },
    KnownDynamic {
        name: "ruby",
        extensions: &["rb", "rake", "gemspec"],
        filenames: &["Gemfile", "Rakefile", "Vagrantfile", "Brewfile", "Podfile"],
        filename_prefixes: &[],
        shebang: &["ruby"],
    },
    KnownDynamic {
        name: "swift",
        extensions: &["swift"],
        filenames: &[],
        filename_prefixes: &[],
        shebang: &[],
    },
];

fn known(name: &str) -> Option<&'static KnownDynamic> {
    KNOWN_DYNAMIC.iter().find(|k| k.name == name)
}

/// All extra languages installable from the `?` menu (id + file extensions).
pub fn known_langs() -> Vec<(&'static str, Vec<String>)> {
    KNOWN_DYNAMIC
        .iter()
        .map(|k| (k.name, k.extensions.iter().map(|s| s.to_string()).collect()))
        .collect()
}

/// Upstream grammar repo for an extra language. `None` = not installable
/// in-app (user drops files in manually instead).
pub fn repo_for(lang: &str) -> Option<&'static str> {
    match lang {
        "swift" => Some("https://github.com/alex-pinkus/tree-sitter-swift"),
        "c-sharp" => Some("https://github.com/tree-sitter/tree-sitter-c-sharp"),
        "cpp" | "c++" => Some("https://github.com/tree-sitter/tree-sitter-cpp"),
        "php" => Some("https://github.com/tree-sitter/tree-sitter-php"),
        "ruby" => Some("https://github.com/tree-sitter/tree-sitter-ruby"),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallState {
    Installed,
    Failed(String),
    Missing,
}

/// Where an extra language stands: working, broken (with raw loader error),
/// or nothing installed yet.
pub fn install_state(lang: &str) -> InstallState {
    scan(false);
    match state().read() {
        Ok(st) => {
            if st.entries.iter().any(|e| e.name == lang) {
                InstallState::Installed
            } else if let Some(err) = st.errors.iter().find(|e| e.starts_with(lang)) {
                InstallState::Failed(err.clone())
            } else {
                InstallState::Missing
            }
        }
        Err(_) => InstallState::Missing,
    }
}

fn run(cmd: &mut std::process::Command) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("could not run: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Download + build an extra language into `syntax/` (same files the manual
/// flow uses), then rescan so it lights up immediately.
/// All errors are already plain-language for direct display.
pub fn install_lang(lang: &str) -> Result<(), String> {
    let repo =
        repo_for(lang).ok_or_else(|| "That language cannot be installed yet.".to_string())?;
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return Err("Could not find git.".to_string());
    }
    if std::process::Command::new("cc")
        .arg("--version")
        .output()
        .is_err()
    {
        return Err("Could not find a C compiler.".to_string());
    }

    let dir = ensure_skeleton();
    let work =
        std::env::temp_dir().join(format!("lazygitrs-grammar-{lang}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|_| "Could not use a temporary folder.".to_string())?;
    let cleanup = || {
        let _ = std::fs::remove_dir_all(&work);
    };

    let src = work.join("src");
    let clone_err = run(std::process::Command::new("git")
        .args(["clone", "--depth", "1", "--quiet", repo])
        .arg(&src));
    if let Err(log) = clone_err {
        cleanup();
        let _ = log;
        return Err("Could not download it. Check your internet connection.".to_string());
    }

    let parser = walk_find(&src, "parser.c").ok_or_else(|| {
        cleanup();
        "Downloaded, but it did not look like a grammar.".to_string()
    })?;
    let pdir = parser.parent().unwrap_or(&src).to_path_buf();
    let mut sources = vec![parser];
    for s in ["scanner.c", "scanner.cc"] {
        let p = pdir.join(s);
        if p.is_file() {
            sources.push(p);
        }
    }
    let out_so = dir.join("grammars").join(format!("{lang}.so"));
    let mut cc = std::process::Command::new("cc");
    cc.args(["-shared", "-fPIC", "-O2", "-I"]);
    cc.arg(&pdir);
    cc.args(&sources);
    cc.args(["-o"]);
    cc.arg(&out_so);
    if run(&mut cc).is_err() {
        cleanup();
        return Err("Downloaded, but it would not build.".to_string());
    }

    // Queries: top-level queries/ first, else first highlights.scm found (php).
    let top = src.join("queries").join("highlights.scm");
    let qsrc = if top.is_file() {
        src.join("queries")
    } else {
        match walk_find(&src, "highlights.scm") {
            Some(p) => p.parent().unwrap_or(&src).to_path_buf(),
            None => {
                cleanup();
                return Err("Built it, but its color file was missing.".to_string());
            }
        }
    };
    let dest = dir.join("queries").join(lang);
    std::fs::create_dir_all(&dest).map_err(|_| "Could not save the files.".to_string())?;
    for q in ["highlights.scm", "injections.scm", "locals.scm"] {
        let from = qsrc.join(q);
        if from.is_file() && std::fs::copy(&from, dest.join(q)).is_err() {
            cleanup();
            return Err("Could not save the files.".to_string());
        }
    }
    cleanup();

    reload();
    match install_state(lang) {
        InstallState::Installed => Ok(()),
        InstallState::Failed(_) => {
            Err("Installed, but its color file could not be read.".to_string())
        }
        InstallState::Missing => Err("Installed, but it did not load. Try reloading.".to_string()),
    }
}

fn walk_find(root: &Path, file: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip test corpora (large, and php keeps per-variant queries
                // we don't want to mistake for the top-level one).
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name == "test" || name == ".git" {
                        continue;
                    }
                }
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some(file) {
                return Some(path);
            }
        }
    }
    None
}

pub struct DynamicEntry {
    pub name: String,
    pub extensions: Vec<String>,
    pub filenames: Vec<String>,
    pub filename_prefixes: Vec<String>,
    pub shebang: Vec<String>,
    pub config: &'static HighlightConfiguration,
}

struct State {
    entries: Vec<DynamicEntry>,
    errors: Vec<String>,
    attempted: HashSet<String>,
}

static STATE: OnceLock<RwLock<State>> = OnceLock::new();

fn state() -> &'static RwLock<State> {
    STATE.get_or_init(|| {
        RwLock::new(State {
            entries: Vec::new(),
            errors: Vec::new(),
            attempted: HashSet::new(),
        })
    })
}

/// Always the lazygitrs dir (never legacy lazygit) so we don't pollute it.
pub fn syntax_dir() -> PathBuf {
    crate::config::config_dir_candidates()
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("syntax")
}

pub fn ensure_skeleton() -> PathBuf {
    let dir = syntax_dir();
    let _ = std::fs::create_dir_all(dir.join("grammars"));
    let _ = std::fs::create_dir_all(dir.join("queries"));
    if !dir.join("README.md").exists() {
        let _ = std::fs::write(dir.join("README.md"), README);
    }
    if !dir.join("languages.yml").exists() {
        let _ = std::fs::write(dir.join("languages.yml"), EXAMPLE_YML);
    }
    dir
}

fn sanitize(name: &str) -> String {
    name.replace('-', "_")
}

/// C symbol suffix for a language id (`tree_sitter_<suffix>`).
/// `c++` is never a valid C identifier — upstream exports `tree_sitter_cpp`.
pub fn symbol_suffix(lang: &str) -> String {
    match lang {
        "c++" | "cpp" => "cpp".to_string(),
        _ => sanitize(lang),
    }
}

fn load_language(
    lib_path: &Path,
    lang: &str,
) -> Result<(tree_sitter::Language, libloading::Library), String> {
    let lib =
        unsafe { libloading::Library::new(lib_path) }.map_err(|e| format!("open .so: {e}"))?;
    let sym_name = format!("tree_sitter_{}", symbol_suffix(lang));
    unsafe {
        let func: libloading::Symbol<unsafe extern "C" fn() -> *const ()> = lib
            .get(sym_name.as_bytes())
            .map_err(|_| format!("missing symbol `{sym_name}`"))?;
        let ptr = func();
        if ptr.is_null() {
            return Err("null language pointer".into());
        }
        let lang_fn = tree_sitter_language::LanguageFn::from_raw(*func);
        Ok((tree_sitter::Language::new(lang_fn), lib))
    }
}

fn find_library(grammars: &Path, lang: &str) -> Option<PathBuf> {
    let flat = sanitize(lang);
    [
        grammars.join(format!("{lang}.so")),
        grammars.join(format!("{flat}.so")),
        grammars.join(format!("libtree-sitter-{lang}.so")),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

fn read_opt(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn attempt_lang(
    dir: &Path,
    lang: &str,
    ov: &LangOverride,
) -> (Option<DynamicEntry>, Option<String>) {
    let qdir = dir.join("queries").join(lang);
    let hl_path = qdir.join("highlights.scm");
    if !hl_path.is_file() {
        return (None, None); // not a language dir, ignore silently
    }
    let highlights = read_opt(&hl_path);
    if highlights.trim().is_empty() {
        return (None, Some(format!("{lang}: highlights.scm is empty")));
    }
    let injections = read_opt(&qdir.join("injections.scm"));
    let locals = read_opt(&qdir.join("locals.scm"));
    let lib_path = match find_library(&dir.join("grammars"), lang) {
        Some(p) => p,
        None => {
            return (None, Some(format!("{lang}: no grammars/{lang}.so")));
        }
    };
    let (language, lib) = match load_language(&lib_path, lang) {
        Ok(v) => v,
        Err(e) => return (None, Some(format!("{lang}: {e}"))),
    };
    let name_static: &'static str = Box::leak(lang.to_string().into_boxed_str());
    match HighlightConfiguration::new(language, name_static, &highlights, &injections, &locals) {
        Ok(mut config) => {
            config.configure(HIGHLIGHT_NAMES);
            // The grammar's machine code lives in `lib`; never unload it.
            Box::leak(Box::new(lib));
            let k = known(lang);
            let strs = |v: Option<&Vec<String>>, known: &[&str], fallback: Vec<String>| {
                v.cloned().unwrap_or_else(|| {
                    if known.is_empty() {
                        fallback
                    } else {
                        known.iter().map(|s| s.to_string()).collect()
                    }
                })
            };
            (
                Some(DynamicEntry {
                    name: lang.to_string(),
                    extensions: strs(
                        ov.extensions.as_ref(),
                        k.map(|k| k.extensions).unwrap_or(&[]),
                        vec![lang.to_string()],
                    ),
                    filenames: strs(
                        ov.filenames.as_ref(),
                        k.map(|k| k.filenames).unwrap_or(&[]),
                        Vec::new(),
                    ),
                    filename_prefixes: strs(
                        ov.filename_prefixes.as_ref(),
                        k.map(|k| k.filename_prefixes).unwrap_or(&[]),
                        Vec::new(),
                    ),
                    shebang: strs(
                        ov.shebang.as_ref(),
                        k.map(|k| k.shebang).unwrap_or(&[]),
                        Vec::new(),
                    )
                    .iter()
                    .map(|s| s.to_ascii_lowercase())
                    .collect(),
                    config: Box::leak(Box::new(config)),
                }),
                None,
            )
        }
        Err(e) => (None, Some(format!("{lang}: bad query: {e:?}"))),
    }
}

/// Idempotent scan: loads language dirs never attempted before.
/// Cheap (one dir listing) so misses can trigger it on demand.
fn scan(force: bool) {
    let dir = ensure_skeleton();
    let overrides: HashMap<String, LangOverride> =
        std::fs::read_to_string(dir.join("languages.yml"))
            .ok()
            .and_then(|s| serde_yaml::from_str(&s).ok())
            .unwrap_or_default();
    let Ok(rd) = std::fs::read_dir(dir.join("queries")) else {
        return;
    };
    let mut langs: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().join("highlights.scm").is_file())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    langs.sort();
    if langs.is_empty() {
        return;
    }
    let Ok(mut st) = state().write() else {
        return;
    };
    if force {
        st.entries.clear();
        st.errors.clear();
        st.attempted.clear();
    }
    for lang in langs {
        if st.attempted.contains(&lang) {
            continue;
        }
        st.attempted.insert(lang.clone());
        let ov = overrides.get(&lang).cloned().unwrap_or_default();
        let (entry, err) = attempt_lang(&dir, &lang, &ov);
        if let Some(entry) = entry {
            st.entries.push(entry);
        }
        if let Some(err) = err {
            st.errors.push(err);
        }
    }
}

fn basename_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn find_in<'a>(
    entries: &'a [DynamicEntry],
    filename: &str,
    first_line: Option<&str>,
) -> Option<&'a DynamicEntry> {
    let base = basename_of(filename);
    if let Some(e) = entries.iter().find(|e| {
        e.filenames.iter().any(|f| f == base)
            || e.filename_prefixes.iter().any(|p| base.starts_with(p))
    }) {
        return Some(e);
    }
    if let Some(ext) = Path::new(base).extension().and_then(|e| e.to_str()) {
        let ext = ext.to_ascii_lowercase();
        if let Some(e) = entries
            .iter()
            .find(|e| e.extensions.iter().any(|x| x.to_ascii_lowercase() == ext))
        {
            return Some(e);
        }
    }
    if let Some(line) = first_line {
        if let Some(rest) = line.strip_prefix("#!") {
            let rest = rest.to_ascii_lowercase();
            if let Some(e) = entries
                .iter()
                .find(|e| e.shebang.iter().any(|s| !s.is_empty() && rest.contains(s)))
            {
                return Some(e);
            }
        }
    }
    None
}

pub fn get_config_for_file(
    filename: &str,
    first_line: Option<&str>,
) -> Option<&'static HighlightConfiguration> {
    if let Ok(st) = state().read() {
        if let Some(e) = find_in(&st.entries, filename, first_line) {
            return Some(e.config);
        }
    }
    scan(false);
    state()
        .read()
        .ok()
        .and_then(|st| find_in(&st.entries, filename, first_line).map(|e| e.config))
}

pub fn preload() {
    scan(false);
}

pub fn reload() {
    scan(true);
}

/// Installed dynamic languages + raw loader errors. Used by the `?` menu
/// to list user-added grammars (anything in `syntax/queries/` with a .so).
pub fn installed() -> (Vec<String>, Vec<String>) {
    scan(false);
    match state().read() {
        Ok(st) => (
            st.entries.iter().map(|e| e.name.clone()).collect(),
            st.errors.clone(),
        ),
        Err(_) => (Vec::new(), Vec::new()),
    }
}

#[cfg(test)]
mod dynamic_tests {
    use super::*;

    #[test]
    fn known_defaults_cover_heavy_languages() {
        // Extension mapping works out-of-box once the .so + queries exist —
        // no languages.yml needed for these five.
        let cases = [
            ("c-sharp", "cs"),
            ("cpp", "cpp"),
            ("php", "php"),
            ("ruby", "rb"),
            ("swift", "swift"),
        ];
        for (lang, ext) in cases {
            let k = known(lang).unwrap_or_else(|| panic!("no defaults for {lang}"));
            assert!(
                k.extensions.contains(&ext),
                "{lang} defaults missing .{ext}"
            );
        }
        // Ruby filenames + shebangs, PHP shebang.
        let ruby = known("ruby").expect("ruby defaults");
        assert!(ruby.filenames.contains(&"Gemfile"));
        assert!(ruby.shebang.contains(&"ruby"));
        let php = known("php").expect("php defaults");
        assert!(php.shebang.contains(&"php"));
    }

    #[test]
    fn find_in_matches_known_shapes() {
        // Build entries the way attempt_lang would with defaults.
        let entries: Vec<DynamicEntry> = KNOWN_DYNAMIC
            .iter()
            .map(|k| DynamicEntry {
                name: k.name.to_string(),
                extensions: k.extensions.iter().map(|s| s.to_string()).collect(),
                filenames: k.filenames.iter().map(|s| s.to_string()).collect(),
                filename_prefixes: k.filename_prefixes.iter().map(|s| s.to_string()).collect(),
                shebang: k.shebang.iter().map(|s| s.to_string()).collect(),
                config: Box::leak(Box::new(
                    HighlightConfiguration::new(
                        tree_sitter_bash::LANGUAGE.into(),
                        Box::leak(k.name.to_string().into_boxed_str()),
                        tree_sitter_bash::HIGHLIGHT_QUERY,
                        "",
                        "",
                    )
                    .expect("bash query builds"),
                )),
            })
            .collect();
        assert_eq!(
            find_in(&entries, "main.cpp", None).map(|e| e.name.as_str()),
            Some("cpp")
        );
        assert_eq!(
            find_in(&entries, "A.cs", None).map(|e| e.name.as_str()),
            Some("c-sharp")
        );
        assert_eq!(
            find_in(&entries, "Gemfile", None).map(|e| e.name.as_str()),
            Some("ruby")
        );
        assert_eq!(
            find_in(&entries, "run.php", None).map(|e| e.name.as_str()),
            Some("php")
        );
        assert_eq!(
            find_in(&entries, "x", Some("#!/usr/bin/env ruby")).map(|e| e.name.as_str()),
            Some("ruby")
        );
    }
}

#[cfg(test)]
mod install_tests {
    use super::*;

    #[test]
    fn symbol_suffix_maps_cpp() {
        assert_eq!(symbol_suffix("cpp"), "cpp");
        assert_eq!(symbol_suffix("c++"), "cpp");
        assert_eq!(symbol_suffix("c-sharp"), "c_sharp");
        assert_eq!(symbol_suffix("swift"), "swift");
    }

    #[test]
    fn repo_for_covers_known_langs() {
        for (id, _) in known_langs() {
            assert!(
                repo_for(id).is_some_and(|u| u.starts_with("https://")),
                "{id} has no install repo"
            );
        }
        assert!(repo_for("zig").is_none());
    }

    #[test]
    fn install_unknown_lang_fails_fast_without_network() {
        let err = install_lang("klingon").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn missing_lang_reports_missing_state() {
        assert_eq!(
            install_state("definitely-not-a-language"),
            InstallState::Missing
        );
    }
}
