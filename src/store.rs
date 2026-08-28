use anyhow::{Context, Result, ensure};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub updated: SystemTime,
}

pub struct NotesStore {
    root: PathBuf,
}

impl NotesStore {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)
            .with_context(|| format!("create notes dir {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn default_root() -> Result<PathBuf> {
        let home = std::env::var("HOME").context("HOME is not set")?;
        Ok(PathBuf::from(home).join("Library/Application Support/Claudio Notes"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn seed_from(&self, src: &Path) -> Result<usize> {
        if !src.is_dir() {
            self.write_index()?;
            return Ok(0);
        }
        let mut copied = 0;
        copy_tree(src, src, &self.root, &mut copied)?;
        self.write_index()?;
        Ok(copied)
    }

    pub fn list(&self) -> Result<Vec<NoteSummary>> {
        let mut notes = Vec::new();
        collect_md(&self.root, &self.root, &mut notes)?;
        notes.sort_by(|a, b| b.updated.cmp(&a.updated).then(a.id.cmp(&b.id)));
        Ok(notes)
    }

    pub fn read(&self, id: &str) -> Result<String> {
        let path = self.resolve(id)?;
        let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        Ok(strip_rook_footer(&raw))
    }

    pub fn write(&self, id: &str, content: &str) -> Result<()> {
        self.write_atomic(id, content)?;
        self.write_index()
    }

    pub fn create(&self, title: &str) -> Result<NoteSummary> {
        let id = unique_id(&self.root, title)?;
        self.write_atomic(&id, "")?;
        self.write_index()?;
        Ok(NoteSummary {
            title: title_from_id(&id),
            id,
            updated: SystemTime::now(),
        })
    }

    /// Deletes a note with `remove_file` (not Trash). Empty parent folders are left in place.
    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.resolve(id)?;
        if path.exists() {
            fs::remove_file(&path).with_context(|| format!("delete {}", path.display()))?;
        }
        self.write_index()
    }

    pub fn path_of(&self, id: &str) -> Result<PathBuf> {
        self.resolve(id)
    }

    /// Writes `INDEX.md` at the vault root listing every note agents should read.
    pub fn write_index(&self) -> Result<()> {
        let mut notes = self.list()?;
        notes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut body = String::from("# Index\n\n");
        if notes.is_empty() {
            body.push_str("_No notes._\n");
        } else {
            for note in &notes {
                let title = filename_stem(&note.id);
                let mtime = format_mtime(note.updated);
                let _ = writeln!(body, "- [{title}]({}) — {mtime}", note.id);
            }
        }
        let path = self.root.join("INDEX.md");
        let tmp = self.root.join("INDEX.md.tmp");
        fs::write(&tmp, body).with_context(|| format!("write {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| format!("rename into {}", path.display()))?;
        Ok(())
    }

    fn write_atomic(&self, id: &str, content: &str) -> Result<()> {
        let path = self.resolve(id)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("md.tmp");
        fs::write(&tmp, content).with_context(|| format!("write {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| format!("rename into {}", path.display()))?;
        Ok(())
    }

    fn resolve(&self, id: &str) -> Result<PathBuf> {
        assert_safe_id(id)?;
        let path = self.root.join(id);
        let root = self.root.canonicalize().unwrap_or_else(|_| self.root.clone());
        if path.exists() {
            let canon = path.canonicalize().with_context(|| format!("canonicalize {}", path.display()))?;
            ensure!(canon.starts_with(&root), "note path escapes notes directory");
            return Ok(canon);
        }
        ensure!(
            path.starts_with(&self.root),
            "note path escapes notes directory"
        );
        Ok(path)
    }
}

/// First path segment of a note id, or `""` for vault-root notes.
///
/// `dsa-python/two-pointers.md` → `dsa-python`
/// `git.md` → `""`
pub fn folder_of(id: &str) -> String {
    match id.split_once('/') {
        Some((folder, _)) => folder.to_string(),
        None => String::new(),
    }
}

/// Filename without `.md`, ignoring parent folders.
///
/// `dsa-python/two-pointers.md` → `two-pointers`
/// `git.md` → `git`
pub fn filename_stem(id: &str) -> String {
    let name = id.rsplit('/').next().unwrap_or(id);
    let lower = name.to_ascii_lowercase();
    if let Some(stripped) = lower.strip_suffix(".md") {
        name[..stripped.len()].to_string()
    } else {
        name.to_string()
    }
}

fn format_mtime(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(t)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn is_skipped_md_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("readme.md") || name.eq_ignore_ascii_case("index.md")
}

pub fn sanitize_title(raw: &str) -> String {
    let trimmed = raw.trim();
    let base = if trimmed.is_empty() { "Untitled" } else { trimmed };
    let mut out = String::new();
    for ch in base.chars() {
        if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (ch as u32) <= 0x1F {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let sliced: String = collapsed.chars().take(160).collect();
    if sliced.is_empty() {
        "Untitled".into()
    } else {
        sliced
    }
}

fn title_from_id(id: &str) -> String {
    id.strip_suffix(".md").unwrap_or(id).to_string()
}

fn id_from_title(title: &str) -> String {
    format!("{}.md", sanitize_title(title))
}

fn unique_id(root: &Path, title: &str) -> Result<String> {
    let base = sanitize_title(title);
    let mut candidate = id_from_title(&base);
    let mut n = 2;
    loop {
        if !root.join(&candidate).exists() {
            return Ok(candidate);
        }
        candidate = id_from_title(&format!("{base} {n}"));
        n += 1;
        ensure!(n < 10_000, "could not allocate a unique note id");
    }
}

fn assert_safe_id(id: &str) -> Result<()> {
    let id = id.trim();
    ensure!(!id.is_empty(), "invalid note id");
    ensure!(!id.contains(".."), "invalid note id");
    ensure!(!id.starts_with('/') && !id.starts_with('\\'), "invalid note id");
    ensure!(id.to_ascii_lowercase().ends_with(".md"), "invalid note id");
    Ok(())
}

fn collect_md(root: &Path, dir: &Path, out: &mut Vec<NoteSummary>) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("list {}", dir.display())),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_md(root, &path, out)?;
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".md") || is_skipped_md_name(name) {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let id = rel.to_string_lossy().replace('\\', "/");
        if assert_safe_id(&id).is_err() {
            continue;
        }
        let updated = fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        out.push(NoteSummary {
            title: title_from_id(&id),
            id,
            updated,
        });
    }
    Ok(())
}

fn copy_tree(src_root: &Path, from: &Path, to_root: &Path, copied: &mut usize) -> Result<()> {
    for entry in fs::read_dir(from).with_context(|| format!("seed list {}", from.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            copy_tree(src_root, &path, to_root, copied)?;
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".md") || is_skipped_md_name(name) {
            continue;
        }
        let rel = path.strip_prefix(src_root).unwrap_or(&path);
        let dest = to_root.join(rel);
        if dest.exists() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let raw = fs::read_to_string(&path)?;
        fs::write(&dest, strip_rook_footer(&raw))?;
        *copied += 1;
    }
    Ok(())
}

fn strip_rook_footer(input: &str) -> String {
    const START: &str = "<!-- ROOK:FOOTER -->";
    const END: &str = "<!-- /ROOK:FOOTER -->";
    let Some(from) = input.find(START) else {
        return input.to_string();
    };
    let Some(end_at) = input[from..].find(END) else {
        return input.to_string();
    };
    let to = from + end_at + END.len();
    let mut out = String::with_capacity(input.len());
    out.push_str(&input[..from]);
    out.push_str(&input[to..]);
    out.trim_end().to_string() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::UNIX_EPOCH;

    static N: AtomicU64 = AtomicU64::new(0);

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "claudio-notes-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sanitize_and_unique() {
        assert_eq!(sanitize_title("  a/b:c  "), "a b c");
        assert_eq!(sanitize_title(""), "Untitled");
        let dir = tmp();
        let store = NotesStore::open(dir.clone()).unwrap();
        let a = store.create("Untitled").unwrap();
        let b = store.create("Untitled").unwrap();
        assert_eq!(a.id, "Untitled.md");
        assert_eq!(b.id, "Untitled 2.md");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn write_read_list_delete() {
        let dir = tmp();
        let store = NotesStore::open(dir.clone()).unwrap();
        let note = store.create("Git").unwrap();
        store.write(&note.id, "# Git\n\n```bash\ngit status\n```\n").unwrap();
        let body = store.read(&note.id).unwrap();
        assert!(body.contains("git status"));
        let listed = store.list().unwrap();
        assert_eq!(listed[0].id, "Git.md");
        store.delete(&note.id).unwrap();
        assert!(store.list().unwrap().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_path_escape() {
        let dir = tmp();
        let store = NotesStore::open(dir.clone()).unwrap();
        assert!(store.read("../secret.md").is_err());
        assert!(store.read("/tmp/x.md").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn strips_rook_footer() {
        let raw = "# Git\n\nbody\n\n<!-- ROOK:FOOTER -->\n> Rook ads\n<!-- /ROOK:FOOTER -->\n";
        let out = strip_rook_footer(raw);
        assert_eq!(out, "# Git\n\nbody\n");
        assert!(!out.contains("Rook"));
    }

    #[test]
    fn seed_copies_nested_md() {
        let src = tmp();
        fs::create_dir_all(src.join("dsa")).unwrap();
        fs::write(src.join("git.md"), "# git\n<!-- ROOK:FOOTER -->x<!-- /ROOK:FOOTER -->\n").unwrap();
        fs::write(src.join("dsa/two.md"), "# two\n").unwrap();
        fs::write(src.join("README.md"), "skip me").unwrap();
        fs::write(src.join("INDEX.md"), "stale index").unwrap();
        let dest = tmp();
        let store = NotesStore::open(dest.clone()).unwrap();
        let n = store.seed_from(&src).unwrap();
        assert_eq!(n, 2);
        assert_eq!(store.read("git.md").unwrap().trim(), "# git");
        assert!(store.read("dsa/two.md").unwrap().contains("# two"));
        assert!(!dest.join("README.md").exists());
        let index = fs::read_to_string(dest.join("INDEX.md")).unwrap();
        assert!(index.contains("[git](git.md)"));
        assert!(index.contains("[two](dsa/two.md)"));
        assert!(!index.contains("stale index"));
        let _ = fs::remove_dir_all(src);
        let _ = fs::remove_dir_all(dest);
    }

    #[test]
    fn folder_and_stem_helpers() {
        assert_eq!(folder_of("dsa-python/two-pointers.md"), "dsa-python");
        assert_eq!(filename_stem("dsa-python/two-pointers.md"), "two-pointers");
        assert_eq!(folder_of("solid-principles/liskov-substitution.md"), "solid-principles");
        assert_eq!(filename_stem("solid-principles/liskov-substitution.md"), "liskov-substitution");
        assert_eq!(folder_of("git.md"), "");
        assert_eq!(filename_stem("git.md"), "git");
        assert_eq!(folder_of("a/b/c.md"), "a");
        assert_eq!(filename_stem("a/b/c.md"), "c");
    }

    #[test]
    fn list_skips_index_and_write_index_tracks_notes() {
        let dir = tmp();
        let store = NotesStore::open(dir.clone()).unwrap();
        store.create("Git").unwrap();
        store.write("nested/two-pointers.md", "# two\n").unwrap();
        fs::write(dir.join("README.md"), "skip").unwrap();

        let listed: Vec<_> = store.list().unwrap().into_iter().map(|n| n.id).collect();
        assert!(listed.contains(&"Git.md".into()));
        assert!(listed.contains(&"nested/two-pointers.md".into()));
        assert!(!listed.iter().any(|id| id.eq_ignore_ascii_case("INDEX.md")));
        assert!(!listed.iter().any(|id| id.eq_ignore_ascii_case("README.md")));

        let index = fs::read_to_string(dir.join("INDEX.md")).unwrap();
        assert!(index.contains("- [Git](Git.md) — "));
        assert!(index.contains("- [two-pointers](nested/two-pointers.md) — "));

        store.delete("Git.md").unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        let index = fs::read_to_string(dir.join("INDEX.md")).unwrap();
        assert!(!index.contains("Git.md"));
        assert!(index.contains("two-pointers"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn format_mtime_is_isoish() {
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        assert_eq!(format_mtime(t), "2023-11-14 22:13");
    }
}
