use tree_sitter_highlight::HighlightConfiguration;

use super::queries::*;

pub const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "function",
    "function.builtin",
    "function.method",
    "function.macro",
    "keyword",
    "label",
    "module",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "variable.member",
];

pub struct LanguageConfig {
    pub config: HighlightConfiguration,
}

/// Compiled-once highlight configuration for one file extension.
///
/// Each language's tree-sitter query compiles on first use (~20-40ms) instead
/// of all 15 compiling at startup — the old `warm_configs` burned ~0.4s of CPU
/// before the first frame. Extensions never viewed never pay the cost.
pub type LanguageEntry = std::sync::LazyLock<Option<LanguageConfig>>;

fn load_config(
    language: tree_sitter::Language,
    name: &str,
    highlights: &str,
) -> Option<LanguageConfig> {
    match HighlightConfiguration::new(language, name, highlights, "", "") {
        Ok(mut config) => {
            config.configure(HIGHLIGHT_NAMES);
            Some(LanguageConfig { config })
        }
        Err(_e) => {
            #[cfg(debug_assertions)]
            eprintln!("[WARN] Failed to load {} highlight config: {:?}", name, _e);
            None
        }
    }
}

macro_rules! entry {
    ($language:expr, $name:literal, $highlights:expr) => {{
        // A nested `fn` item coerces to the `fn() -> T` pointer that
        // `LazyLock<T>` defaults to — a closure would give every entry a
        // distinct type and the array wouldn't unify.
        fn init() -> Option<LanguageConfig> {
            load_config($language, $name, $highlights)
        }
        std::sync::LazyLock::new(init as fn() -> Option<LanguageConfig>)
    }};
}

/// Extension → lazily-compiled highlight config. The map builds once (cheap);
/// each entry compiles its tree-sitter query on first access.
pub static CONFIGS: std::sync::LazyLock<std::collections::HashMap<&'static str, LanguageEntry>> =
    std::sync::LazyLock::new(|| {
        [
            (
                "ts",
                entry!(
                    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                    "typescript",
                    TS_HIGHLIGHTS
                ),
            ),
            (
                "tsx",
                entry!(
                    tree_sitter_typescript::LANGUAGE_TSX.into(),
                    "tsx",
                    TSX_HIGHLIGHTS
                ),
            ),
            (
                "js",
                entry!(
                    tree_sitter_javascript::LANGUAGE.into(),
                    "javascript",
                    JS_HIGHLIGHTS
                ),
            ),
            (
                "jsx",
                entry!(
                    tree_sitter_javascript::LANGUAGE.into(),
                    "javascript",
                    JS_HIGHLIGHTS
                ),
            ),
            (
                "rs",
                entry!(tree_sitter_rust::LANGUAGE.into(), "rust", RUST_HIGHLIGHTS),
            ),
            (
                "json",
                entry!(tree_sitter_json::LANGUAGE.into(), "json", JSON_HIGHLIGHTS),
            ),
            (
                "py",
                entry!(
                    tree_sitter_python::LANGUAGE.into(),
                    "python",
                    PYTHON_HIGHLIGHTS
                ),
            ),
            (
                "go",
                entry!(tree_sitter_go::LANGUAGE.into(), "go", GO_HIGHLIGHTS),
            ),
            (
                "css",
                entry!(tree_sitter_css::LANGUAGE.into(), "css", CSS_HIGHLIGHTS),
            ),
            (
                "html",
                entry!(tree_sitter_html::LANGUAGE.into(), "html", HTML_HIGHLIGHTS),
            ),
            (
                "toml",
                entry!(
                    tree_sitter_toml_ng::LANGUAGE.into(),
                    "toml",
                    TOML_HIGHLIGHTS
                ),
            ),
            (
                "sh",
                entry!(tree_sitter_bash::LANGUAGE.into(), "bash", BASH_HIGHLIGHTS),
            ),
            (
                "bash",
                entry!(tree_sitter_bash::LANGUAGE.into(), "bash", BASH_HIGHLIGHTS),
            ),
            (
                "md",
                entry!(tree_sitter_md::LANGUAGE.into(), "markdown", MD_HIGHLIGHTS),
            ),
            (
                "mdx",
                entry!(tree_sitter_md::LANGUAGE.into(), "markdown", MD_HIGHLIGHTS),
            ),
        ]
        .into_iter()
        .collect()
    });
