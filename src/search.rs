use crate::types::{FileNode, NodeKind, SearchMode, SearchOptions, FileTypeFilter};

pub fn file_type_category(ext: &str) -> FileTypeFilter {
    let ext = ext.to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "tif" | "webp" | "svg" |
        "ico" | "heic" | "heif" | "raw" | "cr2" | "nef" | "arw" | "dng" | "avif" => FileTypeFilter::Images,

        "mp4" | "mkv" | "mov" | "avi" | "wmv" | "flv" | "webm" | "m4v" | "mpg" |
        "mpeg" | "3gp" | "ts" | "mts" | "m2ts" | "vob" | "ogv" | "rmvb" => FileTypeFilter::Videos,

        "mp3" | "flac" | "aac" | "ogg" | "wav" | "m4a" | "wma" | "opus" | "aiff" |
        "aif" | "ape" | "dsf" | "dsd" | "mka" | "mid" | "midi" => FileTypeFilter::Audio,

        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" |
        "odp" | "txt" | "rtf" | "md" | "markdown" | "epub" | "pages" | "numbers" |
        "key" | "csv" | "tsv" => FileTypeFilter::Documents,

        "rs" | "py" | "js" | "jsx" | "tsx" | "go" | "c" | "cpp" | "cc" |
        "cxx" | "h" | "hpp" | "java" | "kt" | "swift" | "rb" | "php" | "cs" |
        "html" | "htm" | "css" | "scss" | "sass" | "less" | "sh" | "bash" | "zsh" |
        "fish" | "ps1" | "lua" | "r" | "m" | "f" | "f90" | "zig" | "v" | "nim" |
        "ex" | "exs" | "erl" | "hrl" | "clj" | "cljs" | "scala" | "groovy" |
        "toml" | "yaml" | "yml" | "json" | "xml" | "sql" | "graphql" | "proto" |
        "cmake" | "makefile" | "dockerfile" | "vue" | "svelte" => FileTypeFilter::Code,

        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "tgz" | "tbz2" |
        "txz" | "lz4" | "zst" | "cab" | "iso" | "dmg" | "pkg" | "deb" | "rpm" |
        "apk" | "ipa" | "appimage" => FileTypeFilter::Archives,

        "" => FileTypeFilter::Other,
        _ => FileTypeFilter::Other,
    }
}

fn matches_query(node: &FileNode, opts: &SearchOptions) -> bool {
    let q = opts.query.to_lowercase();
    if q.is_empty() {
        return true;
    }
    match opts.mode {
        SearchMode::Name => node.name.to_lowercase().contains(&q),
        SearchMode::Extension => {
            node.extension
                .as_deref()
                .map(|e| e.to_lowercase().contains(&q))
                .unwrap_or(false)
        }
        SearchMode::Both => {
            node.name.to_lowercase().contains(&q)
                || node
                    .extension
                    .as_deref()
                    .map(|e| e.to_lowercase().contains(&q))
                    .unwrap_or(false)
        }
    }
}

fn matches_size(node: &FileNode, opts: &SearchOptions) -> bool {
    let size_mb = node.size as f64 / (1024.0 * 1024.0);
    if let Some(min) = opts.min_size_mb {
        if size_mb < min {
            return false;
        }
    }
    if let Some(max) = opts.max_size_mb {
        if size_mb > max {
            return false;
        }
    }
    true
}

fn matches_type(node: &FileNode, opts: &SearchOptions) -> bool {
    if opts.type_filter == FileTypeFilter::All {
        return true;
    }
    let category = match &node.extension {
        Some(ext) => file_type_category(ext),
        None => FileTypeFilter::Other,
    };
    category == opts.type_filter
}

fn is_trivial(opts: &SearchOptions) -> bool {
    opts.query.is_empty()
        && opts.type_filter == FileTypeFilter::All
        && opts.min_size_mb.is_none()
        && opts.max_size_mb.is_none()
}

pub fn search_descendants(
    node: &FileNode,
    opts: &SearchOptions,
    dirs: &mut Vec<FileNode>,
    files: &mut Vec<FileNode>,
) {
    for child in &node.children {
        if matches_query(child, opts) && matches_size(child, opts) && matches_type(child, opts) {
            match child.kind {
                NodeKind::Directory => dirs.push(child.clone()),
                NodeKind::File => files.push(child.clone()),
            }
        }
        if matches!(child.kind, NodeKind::Directory) {
            search_descendants(child, opts, dirs, files);
        }
    }
}

pub fn apply_search(
    node: &FileNode,
    opts: &SearchOptions,
) -> (Vec<FileNode>, Vec<FileNode>) {
    if is_trivial(opts) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for child in &node.children {
            match child.kind {
                NodeKind::Directory => dirs.push(child.clone()),
                NodeKind::File => files.push(child.clone()),
            }
        }
        return (dirs, files);
    }

    let mut dirs = Vec::new();
    let mut files = Vec::new();
    search_descendants(node, opts, &mut dirs, &mut files);
    (dirs, files)
}
