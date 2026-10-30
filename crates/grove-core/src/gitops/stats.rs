//! Per-file line counts for [`GitStatusFile`](super::GitStatusFile).
//!
//! `git diff` never lists untracked files, so the two halves are counted differently: tracked changes
//! come from one `git diff --numstat` for the whole checkout, and untracked files are read and counted
//! here. One call for the checkout, no call per file — a dirty tree with hundreds of untracked files
//! used to cost a process each.
//!
//! Counting is bounded: a file larger than [`MAX_COUNTED_FILE_BYTES`], or one containing a NUL byte, is
//! reported as `0` added rather than streamed into memory. The count is what the changes list shows, not
//! a hash, so a bound that never blocks the UI is the right trade.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use super::{git_failure, IpcResult};
use super::{Git, GitStatusFile};

/// Untracked files above this size are not counted. Chrome-cache blobs and logs live here; reading
/// them would cost more than the `+N` badge is worth.
pub const MAX_COUNTED_FILE_BYTES: u64 = 1024 * 1024;

/// The binary sniff window: a NUL in the first chunk means "not text", the same rule the diff
/// inspector uses.
const BINARY_SNIFF_BYTES: usize = 8 * 1024;

impl Git {
    /// Fill `added`/`removed` on every entry: one `--numstat` read for tracked paths, a local read for
    /// untracked ones.
    ///
    /// `has_head` selects the base: with a commit, tracked counts cover staged and unstaged changes
    /// together (`HEAD`); before the first commit only the index exists, and everything is untracked.
    pub(crate) fn attach_line_stats(
        &self,
        root: &Path,
        has_head: bool,
        files: &mut [GitStatusFile],
    ) -> IpcResult<()> {
        if files.is_empty() {
            return Ok(());
        }
        let stats = if files.iter().any(|file| !file.untracked) {
            self.numstat(root, has_head)?
        } else {
            HashMap::new()
        };
        for file in files.iter_mut() {
            if file.untracked {
                file.added = count_untracked_lines(&root.join(&file.path));
                file.removed = 0;
                continue;
            }
            // Status and diff detect renames independently; try the new path first, then the old one.
            let counted = stats
                .get(&file.path)
                .or_else(|| file.from.as_deref().and_then(|from| stats.get(from)));
            if let Some((added, removed)) = counted {
                file.added = *added;
                file.removed = *removed;
            }
        }
        Ok(())
    }

    fn numstat(&self, root: &Path, has_head: bool) -> IpcResult<HashMap<String, (u32, u32)>> {
        let mut args = vec![
            "diff",
            "--numstat",
            "-z",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
        ];
        if has_head {
            args.push("HEAD");
        }
        args.push("--");
        let output = self.run(root, &args)?;
        if !output.success() {
            return Err(git_failure("diff --numstat", &output));
        }
        Ok(parse_numstat(&output.stdout))
    }
}

/// `--numstat -z` records. A change is `added\tremoved\tpath\0`; a rename or copy omits the path and
/// appends the old and new paths as records of their own: `added\tremoved\t\0old\0new\0`.
/// Binary files report `-` for both counts.
fn parse_numstat(raw: &str) -> HashMap<String, (u32, u32)> {
    let mut stats = HashMap::new();
    let mut records = raw.split('\0');
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let added = added.parse().unwrap_or(0);
        let removed = removed.parse().unwrap_or(0);
        if path.is_empty() {
            // A rename: the old path is irrelevant, the counts belong to the new one.
            let _old = records.next();
            let Some(new) = records.next() else { break };
            stats.insert(new.to_string(), (added, removed));
        } else {
            stats.insert(path.to_string(), (added, removed));
        }
    }
    stats
}

/// Added lines in one untracked file, bounded by [`MAX_COUNTED_FILE_BYTES`]. A trailing line without a
/// newline counts, matching what the diff inspector shows.
fn count_untracked_lines(path: &Path) -> u32 {
    let Ok(metadata) = std::fs::metadata(path) else {
        return 0;
    };
    if !metadata.is_file() || metadata.len() > MAX_COUNTED_FILE_BYTES {
        return 0;
    }
    let Ok(file) = std::fs::File::open(path) else {
        return 0;
    };
    let mut reader = std::io::BufReader::new(file);
    let mut buffer = [0u8; 64 * 1024];
    let mut lines: u32 = 0;
    let mut last = 0u8;
    let mut sniffed = false;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => return 0,
        };
        if !sniffed {
            if buffer[..read.min(BINARY_SNIFF_BYTES)].contains(&0) {
                return 0;
            }
            sniffed = true;
        }
        lines += buffer[..read].iter().filter(|byte| **byte == b'\n').count() as u32;
        last = buffer[read - 1];
    }
    if metadata.len() > 0 && last != b'\n' {
        lines += 1;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_reads_changes_renames_and_binaries() {
        let raw = "2\t0\tadded.txt\0-\t-\tbin.dat\0-\t-\tbin2.dat\0\
                   1\t0\t\0old.txt\0new.txt\0";
        let stats = parse_numstat(raw);
        assert_eq!(stats.get("added.txt"), Some(&(2, 0)));
        assert_eq!(stats.get("bin.dat"), Some(&(0, 0)));
        assert_eq!(stats.get("bin2.dat"), Some(&(0, 0)));
        assert_eq!(
            stats.get("new.txt"),
            Some(&(1, 0)),
            "a rename belongs to its new path"
        );
        assert!(!stats.contains_key("old.txt"));
    }

    #[test]
    fn numstat_ignores_a_truncated_record() {
        assert!(parse_numstat("3\t1\tok.txt\0broken").contains_key("ok.txt"));
        assert!(!parse_numstat("3\t1\tok.txt\0broken").contains_key("broken"));
    }

    #[test]
    fn untracked_counting_handles_text_binary_and_missing_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let text = dir.path().join("text.txt");
        std::fs::write(&text, "one\ntwo\nthree").expect("write");
        assert_eq!(
            count_untracked_lines(&text),
            3,
            "no trailing newline counts"
        );

        let binary = dir.path().join("blob.bin");
        std::fs::write(&binary, b"head\0tail").expect("write");
        assert_eq!(count_untracked_lines(&binary), 0);

        assert_eq!(count_untracked_lines(&dir.path().join("gone.txt")), 0);
    }

    #[test]
    fn untracked_counting_skips_oversized_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let big = dir.path().join("big.log");
        std::fs::write(&big, vec![b'a'; MAX_COUNTED_FILE_BYTES as usize + 1]).expect("write");
        assert_eq!(count_untracked_lines(&big), 0);
    }

    #[test]
    fn an_empty_untracked_file_has_no_lines() {
        let dir = tempfile::tempdir().expect("temp dir");
        let empty = dir.path().join("empty.txt");
        std::fs::write(&empty, "").expect("write");
        assert_eq!(count_untracked_lines(&empty), 0);
    }
}
