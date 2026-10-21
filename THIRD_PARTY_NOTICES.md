# Third-party notices

Rust dependencies are locked in [Cargo.lock](Cargo.lock). Direct dependencies are pinned in the crate manifests: wgpu 24.0.5 (MIT OR Apache-2.0), winit 0.30.12 (Apache-2.0 OR MIT), pollster 0.4.0 (MIT OR Apache-2.0), bytemuck 1.24.0 (MIT OR Apache-2.0), glam 0.29.3 (MIT OR Apache-2.0), and png 0.17.16 (MIT OR Apache-2.0). Transitive dependency versions are fixed by Cargo.lock; consult each package's license and notice files when redistributing a built application.

`crates/lhr-assets/src/lib.rs` adapts the MIT-licensed read-only 4DS layout algorithm from mafia-workbench, revision `9f1edbfbc20ab2ac57e66d4f8a444127e92d602e`. The full upstream license and copyright notice are preserved in [crates/lhr-assets/UPSTREAM_LICENSE](crates/lhr-assets/UPSTREAM_LICENSE). The adaptation adds bounded reads, Rust data structures, strict unsupported-feature rejection and synthetic tests; it does not include editing or proprietary source code.

REA and Ghidra are external reverse-engineering tools used for development analysis. Their binaries and outputs are not distributed here. Upstream projects without a repository license are research references only; no code or decompiler output is copied from them. Mafia game assets and trademarks are not included and remain the property of their respective owners.
