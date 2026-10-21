# Screenshot history

Actual captures from the Rust application using separately supplied PC game data. Images are compressed WebP previews; uncompressed validation captures remain private. Add dated images for visible milestones, up to 960 pixels wide and 200 KiB per image, rather than a capture for every commit. Game content retains its original ownership.

## 2026-10-21 — menu and static MISE01 inspection

Linux software Vulkan captures. Keyboard/mouse transitions, camera orbit and repeat-load pixel equality were checked. The retained model graph and recovered static tick path are connected to the application. These images do not demonstrate animation, driving, mission execution or original lighting/material parity.

| Czech PC menu · 640 × 480 · 55 KiB | MISE01 neighborhood and taxi · 960 × 640 · 122 KiB |
| --- | --- |
| ![Original Czech menu displayed by the Rust engine](2026-10-21-menu.webp) | ![Static MISE01 neighborhood and taxi displayed by the Rust engine](2026-10-21-mise01.webp) |

New Game and Exit work in the menu. The mission view is a bounded static inspection scene; the taxi cannot be driven.

## 2026-10-21 — original position animation

Actual public CLI capture of unchanged `sipka1.4ds` + `sipka1.5ds` after 25 controlled 16 ms steps (400 ms). The recovered update path changes the retained model pose and uploads its original geometry to the GPU. Separate 0/400/800 ms validation confirms movement and exact return; this still image shows the middle state. Only the documented single-root position domain is supported; this is not character or mission animation.

![Original arrow model at 400 ms in the Rust engine](2026-10-21-position-animation.webp)

960 × 640 · 17 KiB · engine commit [`ed770ed`](https://github.com/dnesdan/lost-heaven-reborn/commit/ed770ed). Reproduce with the README animation command and `--headless --animation-steps 25 --capture /tmp/animation.png`.
