//! Build-time fingerprints of the reading code. A stored document remembers the fingerprint of the code that read it,
//! so a reading made by older code can be redone.
//!
//! - `STATEMENT_READER_VERSION`: every adapter and the PDF reader at once.
//! - `reader_versions.rs` (in `OUT_DIR`): one fingerprint per adapter id, over the files its reading depends on (the
//!   file that names it and every module that file uses, then theirs…). A change to one bank's reader leaves the
//!   documents of the other banks up to date.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().map(|x| x == "rs").unwrap_or(false) {
            out.push(p);
        }
    }
}

fn hex(h: Sha256) -> String {
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    hex[..8].to_string()
}

/// The module a file is: `adapters/boursobank/papers_pdf.rs` → adapters::boursobank::papers_pdf, a `mod.rs` → its
/// folder, `lib.rs` → the crate.
fn module_of(rel: &str) -> Vec<String> {
    let mut parts: Vec<String> = rel
        .trim_end_matches(".rs")
        .split('/')
        .map(String::from)
        .collect();
    if parts
        .last()
        .map(|p| p == "mod" || p == "lib")
        .unwrap_or(false)
    {
        parts.pop();
    }
    parts
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The paths a file names that start at `crate`, `super` or `self`, with their `{…}` groups spread out.
fn paths(text: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let starts = ["crate::", "super::", "self::"]
            .iter()
            .any(|p| chars[i..].starts_with(&p.chars().collect::<Vec<_>>()));
        if starts && (i == 0 || !is_ident(chars[i - 1])) {
            i = read_path(&chars, i, Vec::new(), &mut out);
        } else {
            i += 1;
        }
    }
    out
}

/// Reads `a::b::{c, d::e}` from `i` after `prefix`; returns where it ended.
fn read_path(
    chars: &[char],
    mut i: usize,
    prefix: Vec<String>,
    out: &mut Vec<Vec<String>>,
) -> usize {
    let mut path = prefix;
    loop {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i < chars.len() && chars[i] == '{' {
            i += 1;
            loop {
                while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
                    i += 1;
                }
                if i >= chars.len() || chars[i] == '}' {
                    out.push(path.clone());
                    return i + 1;
                }
                i = read_path(chars, i, path.clone(), out);
            }
        }
        let start = i;
        while i < chars.len() && is_ident(chars[i]) {
            i += 1;
        }
        if start == i {
            out.push(path);
            return i;
        }
        path.push(chars[start..i].iter().collect());
        if chars[i..].starts_with(&[':', ':']) {
            i += 2;
        } else {
            out.push(path);
            return i;
        }
    }
}

fn main() {
    let src = Path::new("src");
    println!("cargo:rerun-if-changed=src");

    // the reading code at once, as before
    let mut all = vec![];
    for dir in ["adapters", "pdf"] {
        files(&src.join(dir), &mut all);
    }
    let mut rel: Vec<(String, PathBuf)> = all
        .into_iter()
        .filter_map(|p| {
            p.strip_prefix(src)
                .ok()
                .map(|r| (r.to_string_lossy().replace('\\', "/"), p.clone()))
        })
        .collect();
    rel.sort();
    let mut h = Sha256::new();
    for (name, path) in &rel {
        h.update(name.as_bytes());
        if let Ok(bytes) = std::fs::read(path) {
            h.update(bytes);
        }
    }
    println!("cargo:rustc-env=STATEMENT_READER_VERSION={}", hex(h));

    // every module of the crate but the tests
    let mut every = vec![];
    files(src, &mut every);
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    for p in every {
        let Ok(r) = p.strip_prefix(src) else { continue };
        let r = r.to_string_lossy().replace('\\', "/");
        if r.starts_with("tests/") || r.starts_with("testkit/") {
            continue;
        }
        sources.insert(r, std::fs::read_to_string(&p).unwrap_or_default());
    }
    let modules: BTreeMap<Vec<String>, String> =
        sources.keys().map(|r| (module_of(r), r.clone())).collect();

    // what each file uses
    let mut uses: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (r, text) in &sources {
        let here = module_of(r);
        let mut deps = BTreeSet::new();
        for path in paths(text) {
            let mut at: Vec<String> = match path.first().map(String::as_str) {
                Some("crate") => vec![],
                Some("self") => here.clone(),
                Some("super") => {
                    let mut m = here.clone();
                    m.pop();
                    m
                }
                _ => continue,
            };
            for seg in path.iter().skip(1) {
                if seg == "super" {
                    at.pop();
                } else {
                    at.push(seg.clone());
                }
            }
            // the longest part of the path that is a module
            while !at.is_empty() {
                if let Some(file) = modules.get(&at) {
                    if file != r {
                        deps.insert(file.clone());
                    }
                    break;
                }
                at.pop();
            }
        }
        uses.insert(r.clone(), deps);
    }

    // the adapter ids each file names, "bank.kind"
    let mut named: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (r, text) in &sources {
        if !r.starts_with("adapters/") || r == "adapters/registry.rs" {
            continue;
        }
        let mut rest = text.as_str();
        while let Some(at) = rest.find('"') {
            rest = &rest[at + 1..];
            let Some(end) = rest.find('"') else { break };
            let literal = &rest[..end];
            rest = &rest[end + 1..];
            let mut parts = literal.split('.');
            let (Some(bank), Some(kind), None) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let word = |s: &str| {
                !s.is_empty()
                    && s.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            };
            if word(bank) && word(kind) && r.contains(&format!("/{bank}")) {
                named
                    .entry(literal.to_string())
                    .or_default()
                    .insert(r.clone());
            }
        }
    }

    let mut entries = Vec::new();
    for (id, roots) in &named {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut todo: Vec<String> = roots.iter().cloned().collect();
        while let Some(file) = todo.pop() {
            if !seen.insert(file.clone()) {
                continue;
            }
            if let Some(deps) = uses.get(&file) {
                todo.extend(deps.iter().cloned());
            }
        }
        let mut h = Sha256::new();
        for file in &seen {
            h.update(file.as_bytes());
            h.update(sources.get(file).map(String::as_bytes).unwrap_or_default());
        }
        entries.push(format!("    ({id:?}, {:?}),", hex(h)));
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(
        out.join("reader_versions.rs"),
        format!(
            "/// The fingerprint of each adapter's reading code, by adapter id.\npub static READER_VERSIONS: &[(&str, &str)] = &[\n{}\n];\n",
            entries.join("\n")
        ),
    )
    .expect("write reader_versions.rs");
}
