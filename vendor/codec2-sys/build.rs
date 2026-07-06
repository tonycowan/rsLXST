extern crate bindgen;

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/wrapper.h");
    println!("cargo:rerun-if-env-changed=CODEC2_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=CODEC2_LIBRARY_DIR");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    let include_dir = env::var("CODEC2_INCLUDE_DIR")
        .ok()
        .or_else(discover_include_dir)
        .unwrap_or_else(|| {
            panic!(
                "Unable to locate libcodec2 headers for bindgen; install libcodec2 \
                 development packages or set CODEC2_INCLUDE_DIR to the directory that \
                 contains codec2/codec2.h"
            );
        });

    let mut builder = bindgen::Builder::default()
        .header("src/wrapper.h")
        .clang_arg(format!("-I{include_dir}"));

    if let Ok(library_dir) = env::var("CODEC2_LIBRARY_DIR") {
        println!("cargo:rustc-link-search={library_dir}");
    } else if let Some(library_dir) = discover_library_dir(&include_dir) {
        println!("cargo:rustc-link-search={}", library_dir.display());
    } else {
        println!("cargo:rustc-link-search=/usr/lib");
        println!("cargo:rustc-link-search=/usr/local/lib");
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
        println!("cargo:rustc-link-search=/opt/homebrew/opt/codec2/lib");
        println!("cargo:rustc-link-search=/usr/local/opt/codec2/lib");
    }

    println!("cargo:rustc-link-lib=codec2");

    let bindings = builder
        .blocklist_item("__bool_true_false_are_defined")
        .blocklist_item("true_")
        .blocklist_item("false_")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings; install libcodec2 development headers");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

fn discover_include_dir() -> Option<String> {
    let mut candidates = Vec::new();

    if let Some(flags) = pkg_config_value(&["--cflags-only-I", "codec2"]) {
        if let Some(dir) = parse_prefixed_flag(&flags, "-I") {
            candidates.push(normalize_include_dir(&dir));
        }
    }

    for brew in brew_bins() {
        if let Some(prefix) = brew_prefix_with(brew, "codec2") {
            candidates.push(format!("{prefix}/include"));
        }
    }

    candidates.extend([
        "/opt/homebrew/opt/codec2/include".to_string(),
        "/usr/local/opt/codec2/include".to_string(),
        "/opt/homebrew/include".to_string(),
        "/usr/local/include".to_string(),
        "/usr/include".to_string(),
    ]);

    candidates
        .into_iter()
        .find(|dir| codec2_header_in(dir))
}

fn discover_library_dir(include_dir: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(flags) = pkg_config_value(&["--libs-only-L", "codec2"]) {
        if let Some(dir) = parse_prefixed_flag(&flags, "-L") {
            candidates.push(PathBuf::from(dir));
        }
    }

    let include_path = Path::new(include_dir);
    if let Some(parent) = include_path.parent() {
        candidates.push(parent.join("lib"));
    }

    for prefix in [
        "/opt/homebrew/opt/codec2",
        "/usr/local/opt/codec2",
        "/opt/homebrew",
        "/usr/local",
        "/usr",
    ] {
        candidates.push(PathBuf::from(prefix).join("lib"));
    }

    candidates.into_iter().find(|dir| library_present(dir))
}

fn codec2_header_in(include_dir: &str) -> bool {
    Path::new(include_dir)
        .join("codec2/codec2.h")
        .is_file()
}

fn library_present(library_dir: &Path) -> bool {
    ["libcodec2.dylib", "libcodec2.so", "libcodec2.a"]
        .iter()
        .any(|name| library_dir.join(name).exists())
}

fn normalize_include_dir(dir: &str) -> String {
    let path = Path::new(dir);
    if path.file_name().is_some_and(|name| name == "codec2") {
        path.parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.to_string())
    } else {
        dir.to_string()
    }
}

fn pkg_config_value(args: &[&str]) -> Option<String> {
    let output = Command::new("pkg-config").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn parse_prefixed_flag(flags: &str, prefix: &str) -> Option<String> {
    flags
        .split_whitespace()
        .find_map(|flag| flag.strip_prefix(prefix).map(str::to_string))
}

fn brew_bins() -> [&'static str; 3] {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew", "brew"]
}

fn brew_prefix_with(brew: &str, formula: &str) -> Option<String> {
    let output = Command::new(brew).args(["--prefix", formula]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_homebrew_pkg_config_include_dir() {
        assert_eq!(
            normalize_include_dir("/opt/homebrew/Cellar/codec2/1.2.0/include/codec2"),
            "/opt/homebrew/Cellar/codec2/1.2.0/include"
        );
    }
}
