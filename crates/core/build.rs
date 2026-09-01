//! Embeds the keyboard layout files so the WASM client can offer every layout
//! without a round trip. All 77 together are about 10 KB.
//!
//! The practice corpora are deliberately *not* embedded: they are 1.7 MB across
//! 38 languages, and the client only ever needs the one language it is set to,
//! so those are served as static files and fetched on demand.

use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let layouts = manifest.join("../../assets/klavaro-data/layouts");

    let mut files: Vec<PathBuf> = fs::read_dir(&layouts)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", layouts.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "kbd"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .kbd files in {}", layouts.display());

    let mut generated = String::new();
    generated.push_str("/// Every bundled layout as `(name, file body)`, sorted by name.\n");
    generated.push_str("pub static LAYOUT_FILES: &[(&str, &str)] = &[\n");
    for path in &files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| panic!("non-UTF-8 layout name: {}", path.display()));
        let absolute = path.canonicalize().expect("layout path resolves");
        generated.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            stem,
            absolute.display().to_string()
        ));
    }
    generated.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("layouts.rs");
    fs::write(&out, generated).unwrap_or_else(|e| panic!("cannot write {}: {e}", out.display()));

    println!("cargo:rerun-if-changed={}", layouts.display());
    println!("cargo:rerun-if-changed=build.rs");
}
