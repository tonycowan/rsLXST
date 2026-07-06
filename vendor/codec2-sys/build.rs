extern crate bindgen;

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/wrapper.h");
    println!("cargo:rerun-if-env-changed=CODEC2_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=CODEC2_LIBRARY_DIR");

    let mut builder = bindgen::Builder::default().header("src/wrapper.h");

    if let Ok(include_dir) = env::var("CODEC2_INCLUDE_DIR") {
        builder = builder.clang_arg(format!("-I{include_dir}"));
    } else if let Some(include_dir) = discover_include_dir() {
        builder = builder.clang_arg(format!("-I{include_dir}"));
    } else {
        builder = builder
            .clang_arg("-I/usr/include")
            .clang_arg("-I/usr/local/include");
    }

    if let Ok(library_dir) = env::var("CODEC2_LIBRARY_DIR") {
        println!("cargo:rustc-link-search={library_dir}");
    } else if let Some(library_dir) = discover_library_dir() {
        println!("cargo:rustc-link-search={library_dir}");
    } else {
        println!("cargo:rustc-link-search=/usr/lib");
        println!("cargo:rustc-link-search=/usr/local/lib");
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
    pkg_config_value(&["--cflags-only-I", "codec2"])
        .and_then(|flags| parse_prefixed_flag(&flags, "-I"))
        .or_else(|| brew_prefix("codec2").map(|prefix| format!("{prefix}/include")))
}

fn discover_library_dir() -> Option<String> {
    pkg_config_value(&["--libs-only-L", "codec2"])
        .and_then(|flags| parse_prefixed_flag(&flags, "-L"))
        .or_else(|| brew_prefix("codec2").map(|prefix| format!("{prefix}/lib")))
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

fn brew_prefix(formula: &str) -> Option<String> {
    let output = Command::new("brew")
        .args(["--prefix", formula])
        .output()
        .ok()?;
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
