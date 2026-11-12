# Vendored crates

Copies of crates that lumen builds with changes of its own. Each one is
used in place of the crates.io release through `[patch.crates-io]` in
the top-level `Cargo.toml`.

## gpui-fast-wgpu

GPUI's renderer (wgpu), from [gpui-fast](https://github.com/longbridge/gpui-fast)
0.1.14, under the Apache License 2.0 (`gpui-fast-wgpu/LICENSE`). The first
commit that adds it is the crate as published on crates.io (without its
`Cargo.lock`); lumen's changes come in the commits after it, and every
file they touch says so at the top.
