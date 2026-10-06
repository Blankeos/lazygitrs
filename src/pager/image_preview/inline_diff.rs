//! Select inline raster sections and their source identities without loading pixels.

use std::collections::HashSet;
use std::path::Path;

use super::{InlineImagePreview, InlineImageSource};
use crate::pager::side_by_side::ParsedDiff;

/// Attach source metadata, not pixels. Inline sections decode on viewport entry.
pub(crate) fn attach_inline_image_previews(
    parsed: &mut ParsedDiff,
    repo: &Path,
    diff: &str,
    old_revision: &str,
    new_revision: Option<&str>,
    excluded_paths: &HashSet<String>,
) {
    let images = sources_for_diff(diff, old_revision, new_revision, excluded_paths)
        .into_iter()
        .filter_map(|(section, old, new)| {
            InlineImagePreview::new(repo, old, new).map(|image| (section, image))
        })
        .collect();
    parsed.attach_inline_images(images);
}

fn sources_for_diff(
    diff: &str,
    old_revision: &str,
    new_revision: Option<&str>,
    excluded_paths: &HashSet<String>,
) -> Vec<(usize, InlineImageSource, InlineImageSource)> {
    let mut sources = Vec::new();
    for (section, (_, file_diff)) in crate::pager::side_by_side::parse_multi_file_diff(diff)
        .iter()
        .enumerate()
    {
        let Some(paths) = crate::git::diff_paths::paths_from_diff(file_diff) else {
            continue;
        };
        if paths
            .old
            .iter()
            .chain(&paths.new)
            .any(|p| excluded_paths.contains(p))
        {
            continue;
        }
        if !paths.old.iter().chain(&paths.new).any(|p| is_image_path(p)) {
            continue;
        }
        // No textual hunk data may be replaced by graphics (attributes can
        // force a PNG-looking file to be diffed as ordinary source text).
        if file_diff.lines().any(|line| line.starts_with("@@")) {
            continue;
        }
        let old = paths.old.map_or(InlineImageSource::Missing, |path| {
            InlineImageSource::Revision {
                revision: old_revision.to_string(),
                path,
            }
        });
        let new = paths
            .new
            .map_or(InlineImageSource::Missing, |path| match new_revision {
                Some(revision) => InlineImageSource::Revision {
                    revision: revision.to_string(),
                    path,
                },
                None => InlineImageSource::Worktree(path),
            });
        sources.push((section, old, new));
    }
    sources
}

pub(crate) fn is_image_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tif" | "tiff"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revision(revision: &str, path: &str) -> InlineImageSource {
        InlineImageSource::Revision {
            revision: revision.into(),
            path: path.into(),
        }
    }

    #[test]
    fn raster_extensions_are_case_insensitive_and_exclude_svg_and_video() {
        for extension in [
            "png", "JPG", "jpeg", "gif", "webp", "bmp", "ico", "tif", "TIFF",
        ] {
            assert!(is_image_path(&format!("folder/image.{extension}")));
        }
        for path in ["image.svg", "movie.mp4", "image.png.txt", "png", ""] {
            assert!(!is_image_path(path));
        }
    }

    #[test]
    fn renamed_images_use_distinct_paths_and_revision_sources() {
        let diff = "diff --git a/old.png b/new.png\nsimilarity index 100%\nrename from old.png\nrename to new.png\n";
        assert_eq!(
            sources_for_diff(diff, "base", Some("tip"), &HashSet::new()),
            vec![(0, revision("base", "old.png"), revision("tip", "new.png"))]
        );
        assert_eq!(
            sources_for_diff(diff, "HEAD", None, &HashSet::new()),
            vec![(
                0,
                revision("HEAD", "old.png"),
                InlineImageSource::Worktree("new.png".into())
            )]
        );
    }

    #[test]
    fn added_and_deleted_images_keep_missing_sides_and_section_indices() {
        let diff = concat!(
            "diff --git a/readme.txt b/readme.txt\nBinary files a/readme.txt and b/readme.txt differ\n",
            "diff --git a/added.png b/added.png\nnew file mode 100644\nBinary files /dev/null and b/added.png differ\n",
            "diff --git a/deleted.jpg b/deleted.jpg\ndeleted file mode 100644\nBinary files a/deleted.jpg and /dev/null differ\n",
        );
        assert_eq!(
            sources_for_diff(diff, "HEAD", Some(""), &HashSet::new()),
            vec![
                (1, InlineImageSource::Missing, revision("", "added.png")),
                (
                    2,
                    revision("HEAD", "deleted.jpg"),
                    InlineImageSource::Missing
                ),
            ]
        );
    }

    #[test]
    fn excludes_either_side_of_a_rename() {
        let diff = "diff --git a/old.png b/new.png\nrename from old.png\nrename to new.png\n";
        for excluded in ["old.png", "new.png"] {
            let excluded_paths = HashSet::from([excluded.to_string()]);
            assert!(sources_for_diff(diff, "HEAD", None, &excluded_paths).is_empty());
        }
    }

    #[test]
    fn textual_image_hunks_are_never_replaced_and_later_indices_stay_aligned() {
        let diff = concat!(
            "diff --git a/text.png b/text.png\n--- a/text.png\n+++ b/text.png\n@@ -1 +1 @@\n-old\n+new\n",
            "diff --git a/real.png b/real.png\nBinary files a/real.png and b/real.png differ\n",
        );
        assert_eq!(
            sources_for_diff(diff, "HEAD", None, &HashSet::new()),
            vec![(
                1,
                revision("HEAD", "real.png"),
                InlineImageSource::Worktree("real.png".into())
            )]
        );
        assert!(sources_for_diff("", "HEAD", None, &HashSet::new()).is_empty());
    }
}
