//! Engine Lab presentation of read-only owner observations.
use crate::lab_owner_inspection::{Channel, Inspection, MAX_ANIMATED_ROWS};
use egui::{Color32, RichText};

pub fn materials(ui: &mut egui::Ui, inspection: &Inspection) {
    ui.heading("Animated materials");
    ui.label(format!(
        "{} animated · {} materials · {} owners",
        inspection.animated_materials, inspection.materials, inspection.material_owners
    ));
    ui.small("Last resolved frame state. Inspecting this panel does not tick either channel or consume random values.");
    ui.small("Pause and Step use the live driver timeline. A texture advances only when a draw reaches its lazy getter.");
    if inspection.material_owners == 0 {
        ui.separator();
        ui.label("This view has no admitted original material owner.");
        ui.small("Open an installed model or mission to inspect its owned material state. Public procedural scenes use host preview materials.");
        return;
    }
    if inspection.animated_materials == 0 {
        ui.separator();
        ui.label("No animated materials in the current owners.");
        return;
    }
    ui.separator();
    for material in &inspection.animated {
        egui::CollapsingHeader::new(format!(
            "Material {}:{} · color {} / {}",
            material.arena, material.slot, material.color.frame, material.color.frames
        ))
        .id_salt(("material", material.arena, material.slot))
        .show(ui, |ui| {
            ui.monospace(format!(
                "References {} · flags {:#010x}",
                material.references, material.flags
            ));
            ui.monospace(format!("Driver time {} ms", material.driver_time_ms));
            channel(ui, "Color", &material.color);
            channel(ui, "Environment", &material.environment);
            ui.small(
                "Environment state is retained; environment texture rendering is not implemented.",
            );
        });
    }
    if inspection.animated_materials > MAX_ANIMATED_ROWS {
        ui.small(format!(
            "Showing the first {MAX_ANIMATED_ROWS} animated materials; counts include every owner."
        ));
    }
}

fn channel(ui: &mut egui::Ui, name: &str, channel: &Channel) {
    ui.strong(name);
    egui::Grid::new(("material channel", ui.id(), name))
        .num_columns(2)
        .spacing([14., 4.])
        .show(ui, |ui| {
            for (label, value) in [
                (
                    "Frame index / count",
                    format!("{} / {}", channel.frame, channel.frames),
                ),
                (
                    "Delay / remainder",
                    format!("{} / {} ms", channel.delay_ms, channel.elapsed_ms),
                ),
                ("Last getter time", format!("{} ms", channel.last_time_ms)),
                (
                    "Loop start",
                    if channel.loop_frame == -1 {
                        "Random".into()
                    } else {
                        channel.loop_frame.to_string()
                    },
                ),
                (
                    "Current texture",
                    channel
                        .texture
                        .map_or_else(|| "None".into(), |key| key.to_string()),
                ),
            ] {
                ui.label(label);
                ui.monospace(value);
                ui.end_row();
            }
        });
    ui.small("Frame indices are zero-based; clock values retain signed 32-bit source state.");
}

pub fn mission(ui: &mut egui::Ui, inspection: &Inspection) {
    ui.separator();
    ui.heading("Mission source owners");
    ui.small("These are live intake and ownership counts. They do not indicate playable mission progression.");
    ui.monospace(format!(
        "Startup patches: {} · {} rows",
        inspection.patch_phase, inspection.patch_replacements
    ));
    if let Some(mission) = &inspection.mission {
        egui::Grid::new("mission owner counts")
            .num_columns(2)
            .spacing([14., 4.])
            .show(ui, |ui| {
                for (label, count) in [
                    ("Live frames", mission.frames),
                    ("Completed Scene2 objects", mission.objects),
                    ("Published frame names", mission.map_entries),
                    ("Live actors", mission.actors),
                    ("Standalone source programs", mission.standalone_programs),
                    ("Content allocations", mission.content_allocations),
                    ("Scene2 byte cursor", mission.cursor),
                ] {
                    ui.label(label);
                    ui.monospace(count.to_string());
                    ui.end_row();
                }
            });
        if let Some(frontier) = &mission.frontier {
            ui.label(
                RichText::new("Loading stopped at an explicit boundary")
                    .color(Color32::from_rgb(245, 175, 92)),
            );
            ui.monospace(format!(
                "Entry {:#010x} · byte {}",
                frontier.entry, frontier.cursor
            ));
            ui.label(&frontier.detail);
        } else {
            ui.label("No recorded Scene2 stop. Game initialization and authored mission execution remain unbound.");
        }
    } else {
        ui.small("No active canonical mission source owner in this view.");
    }
    ui.separator();
    ui.heading("Retained cleanup");
    if inspection.retained_cleanup.is_empty() && inspection.retained_car_cleanup.is_empty() {
        ui.small("No interrupted Scene or Car cleanup is retained.");
    }
    for retained in &inspection.retained_cleanup {
        ui.label(
            RichText::new(format!("Interrupted Scene slot {}", retained.slot))
                .color(Color32::from_rgb(245, 175, 92)),
        );
        ui.monospace(format!(
            "{} frames · {} actors · {} programs",
            retained.frames, retained.actors, retained.standalone_programs
        ));
        if let Some(reason) = &retained.reason {
            ui.label(reason);
        }
    }
    for retained in &inspection.retained_car_cleanup {
        ui.label(
            RichText::new(format!("Interrupted Car slot {}", retained.slot))
                .color(Color32::from_rgb(245, 175, 92)),
        );
        ui.monospace(format!("{} retained frames", retained.frames));
        ui.label(&retained.reason);
    }
}
