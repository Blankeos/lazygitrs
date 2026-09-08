#!/usr/bin/env bash
# Fetch + build heavy tree-sitter grammars as dynamic .so files.
#
# Usage:
#   scripts/fetch-grammars.sh swift c-sharp c++ php ruby   # what you need
#   scripts/fetch-grammars.sh all                          # all five
#
# Output (auto-created, auto-loaded on next diff):
#   <config>/syntax/grammars/<lang>.so
#   <config>/syntax/queries/<lang>/highlights.scm (+ injections/locals if upstream ships them)
#
# Why dynamic: these five grammars are ~19MB (swift 5.2 + c-sharp 5.5 +
# cpp 3.7 + php 2.7 + ruby 2.3MB debug). Colors don't need an LSP or a
# bigger binary — a .so loaded on demand highlights identically.
set -euo pipefail

ALL="swift c-sharp c++ php ruby"

if [ $# -eq 0 ]; then
  echo "usage: $0 <lang>... | all"
  echo "langs: $ALL"
  exit 1
fi

if [ "$1" = "all" ]; then
  set -- $ALL
fi

repo_for() {
  case "$1" in
    swift) echo "https://github.com/alex-pinkus/tree-sitter-swift" ;;
    c-sharp) echo "https://github.com/tree-sitter/tree-sitter-c-sharp" ;;
    c++) echo "https://github.com/tree-sitter/tree-sitter-cpp" ;;
    php) echo "https://github.com/tree-sitter/tree-sitter-php" ;;
    ruby) echo "https://github.com/tree-sitter/tree-sitter-ruby" ;;
    *) echo "" ;;
  esac
}

# Query source per lang, relative to the cloned repo.
# Most grammars ship queries/ at top level; php keeps per-variant dirs.
queries_for() {
  case "$1" in
    php) echo "queries" ;; # handled specially below (php/ subdir fallback)
    *) echo "queries" ;;
  esac
}

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/lazygitrs/syntax"
GRAMMARS="$CONFIG_DIR/grammars"
QUERIES="$CONFIG_DIR/queries"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: need '$1' (git/cc)" >&2
    exit 1
  }
}
need git
need cc

mkdir -p "$GRAMMARS" "$QUERIES"

build_one() {
  lang="$1"
  # Normalize display names to install ids (C++ -> cpp file/symbol).
  case "$lang" in
    c++) lang="cpp" ;;
  esac
  repo="$(repo_for "$lang")"
  if [ -z "$repo" ]; then
    echo "! unknown lang '$lang' (try: $ALL, or add it manually — see syntax/README.md)"
    return 1
  fi
  echo "== $lang =="
  src="$WORK/$lang"
  git clone --depth 1 --quiet "$repo" "$src"

  # Find parser.c (+ optional scanner.c / scanner.cc).
  parser="$(find "$src" -maxdepth 4 -name parser.c | head -1)"
  if [ -z "$parser" ]; then
    echo "! $lang: no parser.c found in $repo"
    return 1
  fi
  pdir="$(dirname "$parser")"
  sources="$parser"
  for s in "$pdir/scanner.c" "$pdir/scanner.cc"; do
    [ -f "$s" ] && sources="$sources $s"
  done

  # shellcheck disable=SC2086
  cc -shared -fPIC -O2 -I"$pdir" $sources -o "$GRAMMARS/$lang.so"
  echo "  built grammars/$lang.so"

  # Queries: prefer top-level queries/, fall back to first subdir with highlights.scm (php).
  qsrc="$src/$(queries_for "$lang")"
  if [ ! -f "$qsrc/highlights.scm" ]; then
    qsrc="$(dirname "$(find "$src" -name highlights.scm -not -path "*/test/*" | head -1)")"
  fi
  if [ -z "$qsrc" ] || [ ! -f "$qsrc/highlights.scm" ]; then
    echo "! $lang: no highlights.scm in $repo — copy one from nvim-treesitter:"
    echo "  $QUERIES/$lang/highlights.scm"
    return 1
  fi
  mkdir -p "$QUERIES/$lang"
  for q in highlights injections locals; do
    [ -f "$qsrc/$q.scm" ] && cp "$qsrc/$q.scm" "$QUERIES/$lang/$q.scm"
  done
  echo "  copied queries/$lang/*.scm"
}

failed=0
for lang in "$@"; do
  build_one "$lang" || failed=1
done

if [ "$failed" = 0 ]; then
  echo "done → $CONFIG_DIR"
  echo "open any diff, or check \`?\` → Syntax highlighting health…"
else
  echo "some langs failed — see messages above (manual steps in $CONFIG_DIR/README.md)"
  exit 1
fi
