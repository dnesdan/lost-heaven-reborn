# Engine Lab

`--lab` opens an optional native egui panel over the existing wgpu renderer.
It controls the current inspection runtime and the owned driving car. It does
not add a second physics implementation or execute authored mission scripts.

```sh
./scripts/run.sh --synthetic --lab
./scripts/run.sh --root /path/to/Mafia --model models/taxi00.4ds --car-driving --lab
./scripts/run.sh --root /path/to/Mafia --model models/TommyTAXIDRIVER.4ds --animation anims/breath01a.5ds --skeletal-animation --lab
```

The synthetic scene needs no game assets. It is a static public quad: camera
controls and reload work, while simulation and vehicle controls are disabled.
The panel shows **Static scene / No simulation clock** instead of presenting an
empty driver counter as running gameplay. Pause/step become available only
when actual retained model callbacks or an owned driving/steering callback exist.
Car inspection's steering callback becomes active after an A/D preview input.

## Controls

| Control | Result |
| --- | --- |
| F1 | Show or hide the panel |
| Pause / Resume, F2 | Stop or resume the bound runtime and driving updates |
| Step 16 ms, F3 | Pause and dispatch one 16 ms bound runtime update |
| Reload scene, F4 | Recreate the selected scene/car and reset its runtime clock and inputs |
| FOV | Change the actual vertical camera FOV, from 30° to 100° |
| Distance, Height, Look ahead | Adjust the host chase camera in driving mode |
| Default view, Close view, Wide | Apply camera-only presets; Close view/Wide require driving |
| Keyboard / Manual | Select one source for actual driving input |
| Throttle, Brake, Steering, Handbrake | Stage manual input for the next real driving tick |
| Neutral inputs | Clear the staged manual input |

Keyboard driving uses W/S, A/D and Space as before. The existing R shortcut
continues to reload a car. Inspection retains its arrow-key orbit and W/S zoom.
The panel captures its own keyboard and pointer interactions. Key releases
still clear the gameplay key set, so editing a numeric value cannot latch W/A/D.
Captured pointer motion still updates the application's real cursor position,
without changing menu hover. The next admitted menu click reselects the item at
that current position; it cannot activate an older selection behind the panel.
Focus loss pauses the Lab, clears held/manual input and discards queued output
audio. Resuming requires an explicit action; synthetic focus-change key presses
cannot resume, step or reload the scene.

## Playback and camera behavior

A paused redraw does not call `advance_bound`, the vehicle update or steering
preview. Camera controls, presentation refresh and the UI remain live. A single
step uses the existing `RuntimeEngine` with a controlled 16 ms clock, then
restores the platform clock baseline. Resuming drops time spent paused instead
of catching up. Driving continues to use its existing 16 ms fixed-step scheduler
and actual `CarRuntime::driving_step` input path. UI passes stage idempotent
commands; they never advance the runtime or mutate vehicle physics directly.

Manual parameters apply only on a real driving tick. Editing them while paused
does not apply throttle or steering. Switching input source chooses one complete
input set; keyboard and manual values are not added together. Reload clears
manual input and completed-update counts, while retaining the chosen camera
settings and paused/running preference.

FOV is a field of the actual renderer camera. The source-derived live-car LOD
bridge derives horizontal FOV from that same vertical FOV and the current
viewport. Changing FOV while paused therefore changes both projection and
eligible owned car LOD on the next redraw. The raw static-world presentation
retains its documented first-LOD limitation. Chase presets change only the
host view, not body transforms or recovered vehicle parameters.

The panel reports the current retained draw/vertex/triangle counts, viewport,
runtime time and update counts. Driving additionally reads actual control
feedback, gear, position, contact count and source speed. **Frame interval** is
the time between redraw handling calls, not GPU execution time. Source speed
has no newly inferred km/h conversion.

## Rendering, dependencies and validation boundary

The three egui packages are pinned to `0.31.1`, whose wgpu 24 and winit 0.30
dependencies share the project's existing pinned versions. No eframe runtime
is added. The UI paints into the current window target after the scene, using
the same device and queue. Reload recreates egui context, input bridge and GPU
resources together, including its font atlas, and restores the window's actual
focus and last real pointer location. Texture deltas survive skipped surface
frames; pending writes are submitted before freed textures are destroyed.
Pointer actions remain captured until the first new UI paint after recreation
or showing the panel, while the window may still display the previous layout.

`--lab` requires a window and rejects `--headless`. `--capture` continues to
capture scene pixels without the Lab overlay. Parameters are session-only;
there is no preset file, engine migration, mission editor or unverified physics
tuning in this dependency.

CPU tests cover paused dispatch suppression, an exact single step through an
actual retained model, clock resumption, static-scene admission and camera
FOV/LOD consistency. Window rendering, interactive input, original-data driving
and platform support must be reported separately. A synthetic UI smoke test
does not validate original assets, vehicle behavior or Metal.
