//! Fail if any workspace member depends on a path outside this repo.
//!
//! Standing invariant (Connor ruling 2026-09-17, Kepler seq9606): sibling path
//! dependencies are dev-only and must never reach naia `dev` or `main`, because
//! naia is public and external consumers clone it with no siblings present.
//! Any `path =` dep resolving outside the repo root breaks `cargo metadata`
//! for those consumers.
//!
//! Usage: cargo run -p naia-ci-checks --bin check_no_escaping_path_deps [-- --root DIR]
//! Exit 0 when clean, 1 listing every escaping dep otherwise; a manifest that
//! cannot be read or parsed also exits 1, with the reason on stderr.
//! Std only. Pattern-based, not a TOML parser: manifests are machine-written
//! TOML and this is a lint, not a resolver.

use std::{
    env, fs,
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(report) => {
            print!("{}", report.text);
            if report.clean {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("check_no_escaping_path_deps: {error}");
            ExitCode::from(1)
        }
    }
}

struct Report {
    clean: bool,
    text: String,
}

fn run(args: Vec<String>) -> Result<Report, String> {
    let root = match args.iter().position(|arg| arg == "--root") {
        Some(index) => PathBuf::from(
            args.get(index + 1)
                .ok_or("--root needs a directory argument")?,
        ),
        // tools/ci -> the repo root.
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    };
    let root = resolve(&root);
    check(&root)
}

fn check(root: &Path) -> Result<Report, String> {
    let members = members_of(&read(&root.join("Cargo.toml"))?)?;
    let mut offenders = Vec::new();
    for member in &members {
        let manifest = root.join(member).join("Cargo.toml");
        if !manifest.is_file() {
            offenders.push(format!("{member}: manifest missing"));
            continue;
        }
        for (index, line) in split_lines(&read(&manifest)?).into_iter().enumerate() {
            let Some(dep) = path_dep(strip_comment(line)) else {
                continue;
            };
            let resolved = resolve(&root.join(member).join(dep));
            if !resolved.starts_with(root) {
                offenders.push(format!(
                    "{member}/Cargo.toml:{}: path {} escapes repo",
                    index + 1,
                    quoted(dep)
                ));
            }
        }
    }
    let mut text = String::new();
    if offenders.is_empty() {
        text.push_str(&format!(
            "OK: {} workspace members, no path dep escapes {}\n",
            members.len(),
            root.display()
        ));
    } else {
        text.push_str(&format!(
            "FAIL: {} repo-escaping path dep(s) in {} workspace members:\n",
            offenders.len(),
            members.len()
        ));
        for offender in &offenders {
            text.push_str(&format!("  {offender}\n"));
        }
    }
    Ok(Report {
        clean: offenders.is_empty(),
        text,
    })
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

/// The quoted entries of the `[...]` block that follows `members` in the
/// `[workspace]` table. A commented-out entry (e.g. `# "bench/iai"`) is not a
/// member.
fn members_of(root_manifest: &str) -> Result<Vec<String>, String> {
    let workspace = root_manifest
        .find("[workspace]")
        .ok_or("root manifest has no [workspace] table")?;
    let members = workspace
        + root_manifest[workspace..]
            .find("members")
            .ok_or("root manifest has no workspace members")?;
    let start = members
        + root_manifest[members..]
            .find('[')
            .ok_or("could not parse members block")?;
    let mut depth = 0usize;
    let mut end = None;
    for (offset, byte) in root_manifest.as_bytes()[start..].iter().enumerate() {
        match byte {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + offset + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let block = &root_manifest[start..end.ok_or("could not parse members block")?];
    let mut found = Vec::new();
    for line in split_lines(block) {
        let line = strip_comment(line);
        let mut rest = line;
        while let Some((value, after)) = next_quoted(rest) {
            found.push(value.to_string());
            rest = after;
        }
    }
    Ok(found)
}

/// Everything before the first `#`.
fn strip_comment(line: &str) -> &str {
    line.split('#').next().unwrap_or("")
}

/// The leftmost non-empty `"..."` in `text`, and the text after it.
fn next_quoted(text: &str) -> Option<(&str, &str)> {
    let mut from = 0;
    while let Some(open) = text[from..].find('"').map(|i| from + i) {
        let body = open + 1;
        match text[body..].find('"') {
            Some(0) => from = body,
            Some(len) => return Some((&text[body..body + len], &text[body + len + 1..])),
            None => return None,
        }
    }
    None
}

/// The value of the leftmost `path = "..."` in `line`, matched anywhere in
/// the line with optional whitespace around `=`.
fn path_dep(line: &str) -> Option<&str> {
    let mut from = 0;
    while let Some(at) = line[from..].find("path").map(|i| from + i) {
        from = at + "path".len();
        let rest = line[from..].trim_start_matches(is_space);
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let Some(rest) = rest.trim_start_matches(is_space).strip_prefix('"') else {
            continue;
        };
        if let Some(len) = rest.find('"').filter(|len| *len > 0) {
            return Some(&rest[..len]);
        }
    }
    None
}

fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// Lines split on every line boundary a manifest editor might write: `\n`,
/// `\r\n`, `\r`, and the rarer vertical-tab, form-feed, file/group/record
/// separator, NEL, and Unicode line/paragraph separators.
fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let end = match c {
            '\r' => {
                if let Some((_, '\n')) = chars.peek() {
                    chars.next();
                    at + 2
                } else {
                    at + 1
                }
            }
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => at + c.len_utf8(),
            _ => continue,
        };
        lines.push(&text[start..at]);
        start = end;
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// A path dep as it appears in the report: single-quoted, or double-quoted
/// when it contains a single quote, with backslashes and control characters
/// escaped.
fn quoted(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Absolute, symlink-resolved form of `path`. Existing prefixes are resolved
/// on disk; a missing tail is kept, with `..` applied lexically, so a dep
/// that points at an absent sibling still resolves to where it would be.
fn resolve(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    let mut out = PathBuf::from("/");
    for component in absolute.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(name) => {
                out.push(name);
                if let Ok(real) = fs::canonicalize(&out) {
                    out = real;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = env::temp_dir().join(format!(
                "naia-ci-checks-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(resolve(&dir))
        }

        fn write(&self, rel: &str, text: &str) {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const ROOT: &str = "[workspace]\nmembers = [\n    \"a\",\n    # \"disabled\",\n    \"b\",\n]\n";

    #[test]
    fn commented_members_are_not_members() {
        assert_eq!(members_of(ROOT).unwrap(), vec!["a", "b"]);
    }

    #[test]
    fn nested_brackets_close_the_members_block() {
        let manifest = "[workspace]\nmembers = [\"a\", [\"x\"], \"b\"]\nexclude = [\"c\"]\n";
        assert_eq!(members_of(manifest).unwrap(), vec!["a", "x", "b"]);
    }

    #[test]
    fn unparseable_root_manifests_refuse() {
        assert!(members_of("[package]\nname = \"x\"\n").is_err());
        assert!(members_of("[workspace]\nmembers = [\"a\"\n").is_err());
    }

    #[test]
    fn path_dep_matches_the_leftmost_spaced_or_unspaced_form() {
        assert_eq!(path_dep(r#"x = { path = "../y" }"#), Some("../y"));
        assert_eq!(
            path_dep(r#"x = {path="../y", version = "1"}"#),
            Some("../y")
        );
        assert_eq!(path_dep(r#"x = { path = "" , path = "z" }"#), Some("z"));
        assert_eq!(path_dep(r#"x = "1""#), None);
        assert_eq!(strip_comment(r#"# x = { path = "../y" }"#), "");
    }

    #[test]
    fn clean_workspace_passes() {
        let s = Scratch::new();
        s.write("Cargo.toml", ROOT);
        s.write("a/Cargo.toml", "[dependencies]\nb = { path = \"../b\" }\n");
        s.write("b/Cargo.toml", "[dependencies]\n");
        let report = check(&s.0).unwrap();
        assert!(report.clean);
        assert_eq!(
            report.text,
            format!(
                "OK: 2 workspace members, no path dep escapes {}\n",
                s.0.display()
            )
        );
    }

    #[test]
    fn escaping_and_missing_members_fail_with_every_offender() {
        let s = Scratch::new();
        s.write("Cargo.toml", ROOT);
        s.write(
            "a/Cargo.toml",
            "[dependencies]\nn = { path = \"../../namako\" }\n# o = { path = \"../../o\" }\nb = { path = \"../b\" }\n",
        );
        let report = check(&s.0).unwrap();
        assert!(!report.clean);
        assert_eq!(
            report.text,
            "FAIL: 2 repo-escaping path dep(s) in 2 workspace members:\n  \
             a/Cargo.toml:2: path '../../namako' escapes repo\n  \
             b: manifest missing\n"
        );
    }

    #[test]
    fn missing_root_manifest_refuses() {
        let s = Scratch::new();
        assert!(check(&s.0).is_err());
    }

    #[test]
    fn root_flag_needs_a_value() {
        assert!(run(vec!["--root".to_string()]).is_err());
    }

    #[test]
    fn resolve_keeps_a_missing_tail_and_applies_parent_lexically() {
        let s = Scratch::new();
        assert_eq!(resolve(&s.0.join("gone/../kept")), s.0.join("kept"));
        assert_eq!(resolve(&s.0.join("./a/./b")), s.0.join("a/b"));
    }

    #[test]
    fn quoted_matches_report_quoting() {
        assert_eq!(quoted("../x"), "'../x'");
        assert_eq!(quoted("it's"), "\"it's\"");
        assert_eq!(quoted(r"a\b"), r"'a\\b'");
    }

    #[test]
    fn split_lines_splits_every_boundary() {
        assert_eq!(split_lines("a\nb\r\nc\rd"), vec!["a", "b", "c", "d"]);
        assert_eq!(split_lines("a\n"), vec!["a"]);
    }
}
