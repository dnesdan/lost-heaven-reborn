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

## 2026-10-22 — original model hierarchy

Actual Linux software Vulkan capture of `4build81.4ds`: three meshes and one dummy, materialized into an owned model/frame graph. Recovered common LS3DF fields, parent transforms and guarded original point arithmetic feed the renderer. Camera, materials and rasterization remain inspection choices; this image does not establish original rendering parity or mission gameplay.

![Original building rendered through the recovered mesh and dummy hierarchy](2026-10-22-original-hierarchy.webp)

960 × 640 · 34 KiB · engine commit [`a739570`](https://github.com/dnesdan/lost-heaven-reborn/commit/a739570). Reproduce with `--model models/4build81.4ds --headless --capture /tmp/hierarchy.png`. A bounded X11 run also passed. Separate animation captures at 0/25/50 steps confirmed movement and exact return to the initial image.

## 2026-10-22 — original skinned character

Actual Linux software Vulkan capture of original `civil02.4ds`: 18 Joints and a 417-vertex first LOD. The owned hierarchy, dense joint binding, eager bounds evaluation and recovered CPU deformation supply the pose; original topology, UVs and textures are retained. A separate original-DLL comparison matches 3,124 vertex, matrix, flag and bounds words. The still image demonstrates static pose rendering, with an unlit inspection renderer; it does not show skeletal animation, original lighting or gameplay.

![Original civil02 character rendered through the recovered skin pipeline](2026-10-22-original-skin.webp)

960 × 640 · 6.3 KiB · engine commit [`31d2bf6`](https://github.com/dnesdan/lost-heaven-reborn/commit/31d2bf600104f5ae0b656d892072f524bbc5b7a6). Reproduce with `--model models/civil02.4ds --headless --capture /tmp/skin.png`. Repeated captures are pixel-identical; a bounded X11 run also verified camera input.

## 2026-10-22 — original Tommy skeletal pose

Actual Rust X11/software-Vulkan capture of `TommyTAXIDRIVER.4ds` with `anims/breath01a.5ds` independently sampled at 1,000 ms. The owned hierarchy runs original Target direction updates, morph weights and CPU skinning; original geometry, UVs and textures are retained. The camera was orbited with actual keyboard input. This still image does not establish controller playback, original lighting or gameplay.

![Original Tommy rendered at a 1000 ms skeletal pose](2026-10-22-tommy-skeletal.webp)

640 × 427 · 2.9 KiB · engine commit [`d735e53`](https://github.com/dnesdan/lost-heaven-reborn/commit/d735e53971d7cd7d3c75fe985d021253a8ec7cb3). Reproduce the pose with `--model models/TommyTAXIDRIVER.4ds --animation anims/breath01a.5ds --sample-animation-ms 1000`; use arrow keys to orbit toward the face. Add `--headless --capture /tmp/tommy.png` for the default inspection view. Static plus five separately sampled poses match 81,216 original-code words; the screenshot is a separate rendering check.
