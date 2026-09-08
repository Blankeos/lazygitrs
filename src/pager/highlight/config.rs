use std::sync::OnceLock;

use tree_sitter_highlight::HighlightConfiguration;

/// Every capture name used by our bundled queries, plus the standard
/// nvim-treesitter/Helix set so future grammars highlight without code changes.
/// Captures missing here simply don't highlight; `highlight_color` in `mod.rs`
/// prefix-matches, so `keyword.function`, `markup.heading`, etc. just work.
pub const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "character",
    "character.special",
    "comment",
    "comment.documentation",
    "constant",
    "constant.builtin",
    "constructor",
    "definition.class",
    "definition.constant",
    "definition.function",
    "definition.interface",
    "definition.macro",
    "definition.method",
    "definition.module",
    "definition.type",
    "diff.delta",
    "diff.minus",
    "diff.plus",
    "doc",
    "embedded",
    "error",
    "escape",
    "function",
    "function.builtin",
    "function.call",
    "function.macro",
    "function.method",
    "function.method.call",
    "import",
    "injection.content",
    "injection.language",
    "keyword",
    "keyword.conditional",
    "keyword.control",
    "keyword.debug",
    "keyword.directive",
    "keyword.exception",
    "keyword.function",
    "keyword.import",
    "keyword.operator",
    "keyword.repeat",
    "keyword.return",
    "keyword.storage",
    "label",
    "literal",
    "local.definition",
    "local.reference",
    "local.scope",
    "markup",
    "markup.bold",
    "markup.heading",
    "markup.italic",
    "markup.link",
    "markup.link.label",
    "markup.link.url",
    "markup.list",
    "markup.quote",
    "markup.raw",
    "markup.strikethrough",
    "module",
    "name",
    "namespace",
    "number",
    "number.float",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "reference.call",
    "reference.class",
    "reference.implementation",
    "reference.type",
    "string",
    "string.documentation",
    "string.escape",
    "string.regexp",
    "string.special",
    "string.special.key",
    "string.special.symbol",
    "tag",
    "tag.attribute",
    "tag.delimiter",
    "tag.error",
    "type",
    "type.builtin",
    "type.definition",
    "variable",
    "variable.builtin",
    "variable.member",
    "variable.parameter",
];

/// One built-in language. Adding a new static language is:
/// 1. `cargo add tree-sitter-<lang>`
/// 2. one `static_lang!` + one `StaticEntry` line below. No query writing —
///    queries come bundled with the grammar crate (Helix-style).
pub struct StaticEntry {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
    /// Prefix match on the basename (e.g. `Dockerfile.dev`, `Containerfile.cuda`).
    pub filename_prefixes: &'static [&'static str],
    pub get: fn() -> Option<&'static HighlightConfiguration>,
}

/// Boilerplate for the common case: bundled highlight/injection/locals queries,
/// compiled once per language (not all-at-once like the old `warm_configs`).
macro_rules! static_lang {
    ($func:ident, $lang:expr, $name:literal, $hl:expr, $inj:expr, $loc:expr) => {
        fn $func() -> Option<&'static HighlightConfiguration> {
            static CELL: OnceLock<Option<HighlightConfiguration>> = OnceLock::new();
            CELL.get_or_init(|| {
                HighlightConfiguration::new($lang, $name, $hl, $inj, $loc)
                    .map(|mut c| {
                        c.configure(HIGHLIGHT_NAMES);
                        c
                    })
                    .ok()
            })
            .as_ref()
        }
    };
}

static_lang!(
    cfg_typescript,
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    "typescript",
    tree_sitter_typescript::HIGHLIGHTS_QUERY,
    "",
    tree_sitter_typescript::LOCALS_QUERY
);

static_lang!(
    cfg_javascript,
    tree_sitter_javascript::LANGUAGE.into(),
    "javascript",
    tree_sitter_javascript::HIGHLIGHT_QUERY,
    tree_sitter_javascript::INJECTIONS_QUERY,
    tree_sitter_javascript::LOCALS_QUERY
);

static_lang!(
    cfg_rust,
    tree_sitter_rust::LANGUAGE.into(),
    "rust",
    tree_sitter_rust::HIGHLIGHTS_QUERY,
    tree_sitter_rust::INJECTIONS_QUERY,
    ""
);

static_lang!(
    cfg_python,
    tree_sitter_python::LANGUAGE.into(),
    "python",
    tree_sitter_python::HIGHLIGHTS_QUERY,
    "",
    ""
);

static_lang!(
    cfg_go,
    tree_sitter_go::LANGUAGE.into(),
    "go",
    tree_sitter_go::HIGHLIGHTS_QUERY,
    "",
    ""
);

static_lang!(
    cfg_json,
    tree_sitter_json::LANGUAGE.into(),
    "json",
    tree_sitter_json::HIGHLIGHTS_QUERY,
    "",
    ""
);

static_lang!(
    cfg_bash,
    tree_sitter_bash::LANGUAGE.into(),
    "bash",
    tree_sitter_bash::HIGHLIGHT_QUERY,
    "",
    ""
);

static_lang!(
    cfg_css,
    tree_sitter_css::LANGUAGE.into(),
    "css",
    tree_sitter_css::HIGHLIGHTS_QUERY,
    "",
    ""
);

static_lang!(
    cfg_html,
    tree_sitter_html::LANGUAGE.into(),
    "html",
    tree_sitter_html::HIGHLIGHTS_QUERY,
    tree_sitter_html::INJECTIONS_QUERY,
    ""
);

static_lang!(
    cfg_toml,
    tree_sitter_toml_ng::LANGUAGE.into(),
    "toml",
    tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
    "",
    ""
);

static_lang!(
    cfg_markdown,
    tree_sitter_md::LANGUAGE.into(),
    "markdown",
    tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
    tree_sitter_md::INJECTION_QUERY_BLOCK,
    ""
);

static_lang!(
    cfg_c,
    tree_sitter_c::LANGUAGE.into(),
    "c",
    tree_sitter_c::HIGHLIGHT_QUERY,
    "",
    ""
);

static_lang!(
    cfg_java,
    tree_sitter_java::LANGUAGE.into(),
    "java",
    tree_sitter_java::HIGHLIGHTS_QUERY,
    "",
    ""
);

static_lang!(
    cfg_dockerfile,
    arborium_dockerfile::language().into(),
    "dockerfile",
    arborium_dockerfile::HIGHLIGHTS_QUERY,
    arborium_dockerfile::INJECTIONS_QUERY,
    arborium_dockerfile::LOCALS_QUERY
);

static_lang!(
    cfg_lua,
    tree_sitter_lua::LANGUAGE.into(),
    "lua",
    tree_sitter_lua::HIGHLIGHTS_QUERY,
    tree_sitter_lua::INJECTIONS_QUERY,
    tree_sitter_lua::LOCALS_QUERY
);

static_lang!(
    cfg_yaml,
    tree_sitter_yaml::LANGUAGE.into(),
    "yaml",
    tree_sitter_yaml::HIGHLIGHTS_QUERY,
    "",
    ""
);

/// TSX ships no dedicated query upstream; combine the TypeScript highlights
/// with JavaScript's JSX highlights (same pattern jsr/vim-clap use).
fn cfg_tsx() -> Option<&'static HighlightConfiguration> {
    static CELL: OnceLock<Option<HighlightConfiguration>> = OnceLock::new();
    CELL.get_or_init(|| {
        let combined = format!(
            "{}\n{}",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
        );
        HighlightConfiguration::new(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "tsx",
            &combined,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        )
        .or_else(|_| {
            HighlightConfiguration::new(
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                "tsx",
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
                "",
                tree_sitter_typescript::LOCALS_QUERY,
            )
        })
        .map(|mut c| {
            c.configure(HIGHLIGHT_NAMES);
            c
        })
        .ok()
    })
    .as_ref()
}

pub static STATIC_LANGS: &[StaticEntry] = &[
    StaticEntry {
        name: "typescript",
        extensions: &["ts", "mts", "cts"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_typescript,
    },
    StaticEntry {
        name: "tsx",
        extensions: &["tsx"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_tsx,
    },
    StaticEntry {
        name: "javascript",
        extensions: &["js", "mjs", "cjs"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_javascript,
    },
    StaticEntry {
        name: "jsx",
        extensions: &["jsx"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_javascript,
    },
    StaticEntry {
        name: "rust",
        extensions: &["rs"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_rust,
    },
    StaticEntry {
        name: "json",
        extensions: &["json", "jsonc"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_json,
    },
    StaticEntry {
        name: "python",
        extensions: &["py", "pyi"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_python,
    },
    StaticEntry {
        name: "go",
        extensions: &["go"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_go,
    },
    StaticEntry {
        name: "css",
        extensions: &["css"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_css,
    },
    StaticEntry {
        name: "html",
        extensions: &["html", "htm"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_html,
    },
    StaticEntry {
        name: "toml",
        extensions: &["toml"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_toml,
    },
    StaticEntry {
        name: "bash",
        extensions: &["sh", "bash", "zsh"],
        filenames: &[".bashrc", ".bash_profile", ".zshrc", ".profile", "PKGBUILD"],
        filename_prefixes: &[],
        get: cfg_bash,
    },
    StaticEntry {
        name: "markdown",
        extensions: &["md", "mdx", "markdown"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_markdown,
    },
    StaticEntry {
        name: "c",
        extensions: &["c", "h"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_c,
    },
    StaticEntry {
        name: "java",
        extensions: &["java"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_java,
    },
    StaticEntry {
        name: "dockerfile",
        extensions: &["dockerfile"],
        filenames: &["Dockerfile", "Containerfile"],
        filename_prefixes: &["Dockerfile.", "Containerfile."],
        get: cfg_dockerfile,
    },
    StaticEntry {
        name: "lua",
        extensions: &["lua"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_lua,
    },
    StaticEntry {
        name: "yaml",
        extensions: &["yml", "yaml"],
        filenames: &[],
        filename_prefixes: &[],
        get: cfg_yaml,
    },
];

/// Friendly display name for the `?` panel. Internal ids like
/// `c-sharp` / `cpp` mean nothing to someone without the codebase.
pub fn pretty_name(name: &str) -> &'static str {
    match name {
        "typescript" => "TypeScript",
        "tsx" => "TSX",
        "javascript" => "JavaScript",
        "jsx" => "JSX",
        "rust" => "Rust",
        "json" => "JSON",
        "python" => "Python",
        "go" => "Go",
        "css" => "CSS",
        "html" => "HTML",
        "toml" => "TOML",
        "bash" => "Shell",
        "markdown" => "Markdown",
        "c" => "C",
        "java" => "Java",
        "lua" => "Lua",
        "yaml" => "YAML",
        "dockerfile" => "Dockerfile",
        "cpp" => "C++",
        "c-sharp" => "C#",
        "php" => "PHP",
        "ruby" => "Ruby",
        "swift" => "Swift",
        _ => "Unknown",
    }
}

fn basename_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn config_for_shebang(first_line: &str) -> Option<&'static StaticEntry> {
    let line = first_line.strip_prefix("#!")?.to_ascii_lowercase();
    let find = |names: &[&str]| STATIC_LANGS.iter().find(|e| names.contains(&e.name));
    if line.contains("python") {
        return find(&["python"]);
    }
    if line.contains("bash") || line.contains("zsh") || line.contains("/sh") {
        return find(&["bash"]);
    }
    if line.contains("lua") && !line.contains("evaluate") {
        return find(&["lua"]);
    }
    if line.contains("node") || line.contains("deno") || line.contains("bun") {
        return find(&["javascript"]);
    }
    None
}

/// Static lookup: exact filename → extension → `#!` shebang.
pub fn config_for_file(
    filename: &str,
    first_line: Option<&str>,
) -> Option<(&'static StaticEntry, &'static HighlightConfiguration)> {
    let base = basename_of(filename);
    if let Some(e) = STATIC_LANGS.iter().find(|e| {
        e.filenames.contains(&base) || e.filename_prefixes.iter().any(|p| base.starts_with(p))
    }) {
        if let Some(c) = (e.get)() {
            return Some((e, c));
        }
    }
    if let Some(ext) = std::path::Path::new(base)
        .extension()
        .and_then(|e| e.to_str())
    {
        let ext = ext.to_ascii_lowercase();
        if let Some(e) = STATIC_LANGS
            .iter()
            .find(|e| e.extensions.iter().any(|x| *x == ext))
        {
            if let Some(c) = (e.get)() {
                return Some((e, c));
            }
        }
    }
    if let Some(line) = first_line {
        if let Some(e) = config_for_shebang(line) {
            if let Some(c) = (e.get)() {
                return Some((e, c));
            }
        }
    }
    None
}

/// Touch every static config. Called from a background thread (see
/// `Gui::new`), so the first diff never pays the compile cost.
pub fn warm_all() {
    for e in STATIC_LANGS {
        let _ = (e.get)();
    }
}
