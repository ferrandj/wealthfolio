//! Build-time fingerprint of the reading code (every adapter and the PDF reader). A stored document remembers the
//! fingerprint of the code that read it, so a reading made by older code can be redone.

use sha2::{Digest, Sha256};
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

fn main() {
    let src = Path::new("src");
    let mut all = vec![];
    for dir in ["adapters", "pdf"] {
        files(&src.join(dir), &mut all);
        println!("cargo:rerun-if-changed=src/{dir}");
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
    for (name, path) in rel {
        h.update(name.as_bytes());
        if let Ok(bytes) = std::fs::read(path) {
            h.update(bytes);
        }
    }
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    println!("cargo:rustc-env=STATEMENT_READER_VERSION={}", &hex[..8]);
}
