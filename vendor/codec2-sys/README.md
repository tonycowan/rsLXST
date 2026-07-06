# codec2-sys (vendored)

Vendored from [codec2-sys 1.0.0](https://crates.io/crates/codec2-sys) with a
portable `build.rs` that discovers libcodec2 include and library paths via
`pkg-config`, Homebrew, or standard system locations.

Override discovery with:

```bash
export CODEC2_INCLUDE_DIR=/path/to/include
export CODEC2_LIBRARY_DIR=/path/to/lib
```

Licensed under LGPL-2.1 (see upstream crate).
