use crate::types::*;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn start_scan(path: PathBuf, state: SharedScanState, max_depth: usize, skip_hidden: bool) {
    {
        let mut s = state.lock().unwrap();
        s.status = ScanStatus::Scanning;
        s.progress = ScanProgress::default();
        s.result = None;
    }

    let state_clone = state.clone();

    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<ScanProgress>();

        // Spawn a thread that forwards progress updates into the shared state.
        let state_progress = state_clone.clone();
        let progress_thread = std::thread::spawn(move || {
            let mut last_update = Instant::now();
            let debounce = Duration::from_millis(100);
            let mut latest: Option<ScanProgress> = None;

            loop {
                match rx.recv_timeout(debounce) {
                    Ok(p) => {
                        latest = Some(p);
                        if last_update.elapsed() >= debounce {
                            if let Some(ref prog) = latest {
                                let mut s = state_progress.lock().unwrap();
                                s.progress = prog.clone();
                            }
                            last_update = Instant::now();
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Some(ref prog) = latest {
                            let mut s = state_progress.lock().unwrap();
                            s.progress = prog.clone();
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            // Flush final progress.
            if let Some(prog) = latest {
                let mut s = state_progress.lock().unwrap();
                s.progress = prog;
            }
        });

        let result = std::panic::catch_unwind(|| {
            scan_dir(&path, 0, max_depth, skip_hidden, &tx)
        });

        // Drop the sender so the progress thread exits cleanly.
        drop(tx);
        let _ = progress_thread.join();

        match result {
            Ok(mut root) => {
                sort_children_recursive(&mut root);
                let mut s = state_clone.lock().unwrap();
                s.status = ScanStatus::Complete;
                s.result = Some(root);
            }
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic during scan".to_string()
                };
                let mut s = state_clone.lock().unwrap();
                s.status = ScanStatus::Error(msg);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Core recursive scanner
// ---------------------------------------------------------------------------

fn scan_dir(
    path: &Path,
    depth: usize,
    max_depth: usize,
    skip_hidden: bool,
    progress_tx: &mpsc::Sender<ScanProgress>,
) -> FileNode {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    // Pre-compute so we can use `name` in early returns without borrow issues.
    let my_color = dir_color(&name);

    // Collect direct children via read_dir.
    let entries: Vec<PathBuf> = match std::fs::read_dir(path) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                let fname = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if skip_hidden && fname.starts_with('.') {
                    return false;
                }
                if fname == "node_modules" {
                    return false;
                }
                true
            })
            .collect(),
        Err(_) => {
            // Permission denied or other error — return empty node.
            return FileNode {
                name,
                path: path.to_path_buf(),
                size: 0,
                kind: NodeKind::Directory,
                extension: None,
                children: vec![],
                item_count: 0,
                color: my_color,
            };
        }
    };

    // Skip /proc on Linux (safety; on macOS this is a no-op).
    let path_str = path.to_string_lossy();
    if path_str == "/proc" {
        return FileNode {
            name,
            path: path.to_path_buf(),
            size: 0,
            kind: NodeKind::Directory,
            extension: None,
            children: vec![],
            item_count: 0,
            color: my_color,
        };
    }

    // Counter shared across this call for progress reporting.
    let items_seen = std::sync::atomic::AtomicUsize::new(0);

    // Closure to build a child FileNode for a single entry.
    let build_child = |entry_path: &PathBuf| -> FileNode {
        let fname = entry_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let meta = match std::fs::symlink_metadata(entry_path) {
            Ok(m) => m,
            Err(_) => {
                return file_error_node(entry_path, &fname);
            }
        };

        if meta.file_type().is_symlink() {
            // Record symlinks as 0-byte files, don't follow.
            let ext = extension_of(&fname);
            let color = ext
                .as_deref()
                .map(|e| crate::file_types::color_for_extension(e))
                .unwrap_or([180, 180, 180]);
            return FileNode {
                name: fname,
                path: entry_path.clone(),
                size: 0,
                kind: NodeKind::File,
                extension: ext,
                children: vec![],
                item_count: 1,
                color,
            };
        }

        if meta.is_dir() {
            if depth + 1 > max_depth {
                // Hit depth limit — return empty dir node.
                return FileNode {
                    name: fname.clone(),
                    path: entry_path.clone(),
                    size: 0,
                    kind: NodeKind::Directory,
                    extension: None,
                    children: vec![],
                    item_count: 0,
                    color: dir_color(&fname),
                };
            }
            scan_dir(entry_path, depth + 1, max_depth, skip_hidden, progress_tx)
        } else {
            // Regular file.
            let size = disk_usage(&meta);
            let ext = extension_of(&fname);
            let color = ext
                .as_deref()
                .map(|e| crate::file_types::color_for_extension(e))
                .unwrap_or([200, 200, 200]);
            FileNode {
                name: fname,
                path: entry_path.clone(),
                size,
                kind: NodeKind::File,
                extension: ext,
                children: vec![],
                item_count: 1,
                color,
            }
        }
    };

    // Use rayon parallel iteration for shallow levels, sequential for deep.
    let children: Vec<FileNode> = if depth < 3 {
        entries
            .par_iter()
            .map(|entry_path| {
                let node = build_child(entry_path);
                // Atomic progress tick (best-effort, ignore send errors).
                let count = items_seen
                    .fetch_add(node.item_count.max(1), std::sync::atomic::Ordering::Relaxed);
                if count % 500 == 0 {
                    let _ = progress_tx.send(ScanProgress {
                        current_path: entry_path.to_string_lossy().into_owned(),
                        items_scanned: count,
                        total_size: node.size,
                    });
                }
                node
            })
            .collect()
    } else {
        entries
            .iter()
            .map(|entry_path| {
                let node = build_child(entry_path);
                let count = items_seen
                    .fetch_add(node.item_count.max(1), std::sync::atomic::Ordering::Relaxed);
                if count % 500 == 0 {
                    let _ = progress_tx.send(ScanProgress {
                        current_path: entry_path.to_string_lossy().into_owned(),
                        items_scanned: count,
                        total_size: node.size,
                    });
                }
                node
            })
            .collect()
    };

    let total_size: u64 = children.iter().map(|c| c.size).sum();
    let item_count: usize = children.iter().map(|c| c.item_count).sum::<usize>() + 1;

    // Send one final progress update for this directory.
    let _ = progress_tx.send(ScanProgress {
        current_path: path.to_string_lossy().into_owned(),
        items_scanned: item_count,
        total_size,
    });

    FileNode {
        name,
        path: path.to_path_buf(),
        size: total_size,
        kind: NodeKind::Directory,
        extension: None,
        children,
        item_count,
        color: my_color,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sort children by size descending, recursively.
fn sort_children_recursive(node: &mut FileNode) {
    node.children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    for child in &mut node.children {
        if child.kind == NodeKind::Directory {
            sort_children_recursive(child);
        }
    }
}

/// Return a zero-size file node for entries we can't read.
fn file_error_node(path: &PathBuf, name: &str) -> FileNode {
    FileNode {
        name: name.to_string(),
        path: path.clone(),
        size: 0,
        kind: NodeKind::File,
        extension: None,
        children: vec![],
        item_count: 1,
        color: [100, 100, 100],
    }
}

/// Extract lowercase extension from a filename.
fn extension_of(name: &str) -> Option<String> {
    let dot_pos = name.rfind('.')?;
    if dot_pos == 0 || dot_pos == name.len() - 1 {
        return None;
    }
    Some(name[dot_pos + 1..].to_lowercase())
}

/// Actual disk usage: on macOS/Unix use st_blocks * 512, fallback to len().
#[cfg(unix)]
fn disk_usage(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    // st_blocks is in 512-byte units.
    meta.blocks() * 512
}

#[cfg(not(unix))]
fn disk_usage(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

/// Derive a stable, visually distinct color for a directory from its name.
/// Hashes the name to a hue, then converts HSL(hue, 0.55, 0.55) to RGB.
pub fn dir_color(name: &str) -> [u8; 3] {
    // FNV-1a 32-bit hash for speed.
    let mut hash: u32 = 2_166_136_261u32;
    for byte in name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    let hue = (hash % 360) as f32;
    hsl_to_rgb(hue, 0.55, 0.55)
}

/// Convert HSL (h in [0,360), s and l in [0,1]) to RGB [u8; 3].
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    [
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    ]
}
