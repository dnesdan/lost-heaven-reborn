//! Particle emitor tail + part_element/part_base CPU projections (PC LS3DF.dll
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! GROUP Y2: 6 differential + 7 contract).
//!
//! Differential: LoadImage tail 0x3f9f0 (35 B/3 BB/18 insn), element ctor
//! 0x3fa20 (436 B/1 BB/90 insn), dtor remove-prefix 0x3fc00 (149 B/13 BB/54
//! insn), element Duplicate fields 0x3fca0 (409 B/6 BB/95 insn), base ctor
//! 0x406c0 (120 B/1 BB/23 insn), base Duplicate fields 0x40770 (173 B/7 BB/41
//! insn). Contract (no Ghidra body; entry/mid/RET probes only): emitor
//! Duplicate 0x3f6b0, KillElements 0x3f910, TickElements 0x3f960,
//! SetElementsOn 0x3f9b0, element Tick 0x3fe40, base m_AddFGroups 0x40820
//! (device boundary dominates despite the formed body), base m_RenderFGroup
//! 0x41320. Upstream names (mafia-re I3d_particle.cpp), never original symbols.
//! State reuses the Y1 emitor views (`crate::frame_emitor`) via the named
//! accessors below — field/word map in the module header; nothing here
//! redefines Y1 storage. Float policy: host f32 round-to-nearest; NaN/inf
//! inputs rejected before mutation. Device/render calls stay backend contracts
//! (no live device). Helper phases get no function credit; base-class tails
//! are backend effects.
#![forbid(unsafe_code)]

use crate::native_transform::is_verified_scalar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleError {
    ContractBoundary { rva: u32 },
    UnverifiedFloat,
    InvalidParams,
    Generic,
    BackendFail,
}
pub type Out<T> = Result<T, ParticleError>;

fn check_f32(v: f32) -> Out<f32> {
    if is_verified_scalar(v) {
        Ok(v)
    } else {
        Err(ParticleError::UnverifiedFloat)
    }
}
fn check_vec(v: [f32; 3]) -> Out<[f32; 3]> {
    for x in v {
        check_f32(x)?;
    }
    Ok(v)
}

/// Named view over the Y1 emitor param storage (bind to Y1 words at use).
#[derive(Clone, Debug, PartialEq)]
pub struct EmitorParams {
    pub mesh: Option<u32>,
    pub emit_time: u32,
    pub emit_time_rnd: u32,
    pub life_time: u32,
    pub life_time_rnd: u32,
    pub scale: f32,
    pub grow_1: f32,
    pub grow_2: f32,
    pub pos_rnd: f32,
    pub dir_rnd: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub dir_time: u32,
    pub dir_time_rnd: u32,
    pub dir_time_stray: f32,
    pub velocity: [f32; 3],
    pub velocity_end: [f32; 3],
    pub part_flags: u32,
    pub max_particles: u32,
    pub velocity_world: bool,
    pub velocity_end_world: bool,
    pub rot_speed_1: f32,
    pub rot_speed_2: f32,
    pub billboard: bool,
    pub rot_axis: u32,
    pub axis_mode: u32,
    pub pre_tick: u32,
    pub color: [f32; 3],
    pub color_end: [f32; 3],
    pub omni_sector: bool,
}

/// Named view over the Y1 part_element storage.
#[derive(Clone, Debug, PartialEq)]
pub struct PartElementFields {
    pub life: u32,
    pub life_max: u32,
    pub unknown_290: u32,
    pub dir_start: [f32; 3],
    pub dir_end: [f32; 3],
    pub dir: [f32; 3],
    pub dir_prev: [f32; 3],
    pub dir_time: u32,
    pub dir_time_stray: f32,
    pub speed_start: f32,
    pub speed_end: f32,
    pub grow: f32,
    pub color_start: [f32; 3],
    pub color_end: [f32; 3],
    pub age: u32,
    pub speed: f32,
    pub dir_period: i32,
    pub dir_counter: i32,
    pub lightness: f32,
    pub transparency: f32,
    pub fade: f32,
}

/// Named view over the Y1 part_base storage.
#[derive(Clone, Debug, PartialEq)]
pub struct PartBaseFields {
    pub color: [f32; 3],
    pub billboard: bool,
    pub rot_axis: u32,
    pub axis_mode: u32,
    pub sector_aliased: bool,
    pub sort_group: u16,
    pub part_flags: u32,
}

/// Backend for refcount/device-adjacent effects (no live device here).
pub trait ParticleMeshBackend {
    fn base_load_image(&mut self) -> Out<()>;
    fn load_mesh_image(&mut self) -> Out<Option<u32>>;
    fn set_mesh(&mut self, mesh: Option<u32>) -> Out<()>;
    fn release_mesh(&mut self, mesh: u32) -> Out<()>;
    fn base_element_ctor(&mut self) -> Out<()>;
    fn base_element_dtor_tail(&mut self) -> Out<()>;
    fn base_object_ctor(&mut self) -> Out<()>;
}

/// 0x3f9f0 LoadImage tail: base → load-mesh → set-mesh → conditional release.
/// Order is the semantic (18 insn, callees base + load-mesh).
pub fn emitor_load_image_tail<B: ParticleMeshBackend>(b: &mut B) -> Out<Option<u32>> {
    b.base_load_image()?;
    let m = b.load_mesh_image()?;
    b.set_mesh(m)?; // native set-mesh dispatch runs BEFORE the release
    if let Some(k) = m {
        b.release_mesh(k)?;
    }
    Ok(m)
}

/// 0x3fa20 element ctor defaults (90 insn/1 BB, two-phase: 0x42545250 phase
/// then 0x45545250 phase; unit float words; 0xffff/0x101). The
/// lightness/transparency/fade 1.0 triple is an inferred working default
/// (runtime-plausible, matches the 1.0 color convention) — NOT in the cited
/// 90-insn evidence, which pins dirs/speeds/grow/colors/age/counters only.
/// Re-probe before any differential upgrade of these three words.
pub fn part_element_ctor_defaults() -> Out<PartElementFields> {
    Ok(PartElementFields {
        life: 0,
        life_max: 0,
        unknown_290: 0,
        dir_start: [0., 0., 0.],
        dir_end: [0., 0., 0.],
        dir: [0., 0., 0.],
        dir_prev: [0., 0., 0.],
        dir_time: 0,
        dir_time_stray: 0.,
        speed_start: 0.,
        speed_end: 0.,
        grow: 0.,
        color_start: [1., 1., 1.],
        color_end: [1., 1., 1.],
        age: 0,
        speed: 0.,
        dir_period: 0,
        dir_counter: 0,
        lightness: 1.,
        transparency: 1.,
        fade: 1.,
    })
}

/// 0x406c0 base ctor defaults (23 insn/1 BB).
pub fn part_base_ctor_defaults() -> Out<PartBaseFields> {
    Ok(PartBaseFields {
        color: [1., 1., 1.],
        billboard: true,
        rot_axis: 0,
        axis_mode: 3,
        sector_aliased: false,
        sort_group: 0xffff,
        part_flags: 0x101,
    })
}

/// Swap-remove outcome for 0x3fc00: vtable stores precede list mutation;
/// count==1 fast path requires identity, else decrement-scan-shift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementRemove {
    RemovedLast,
    RemovedShifted { moved_from_last: usize },
    Absent,
}
pub fn part_element_dtor_remove_index(
    count: u32,
    slots: &[u32],
    this: u32,
) -> (ElementRemove, u32) {
    if count == 0 {
        return (ElementRemove::Absent, 0);
    }
    if count == 1 {
        if slots.first() == Some(&this) {
            return (ElementRemove::RemovedLast, 0);
        }
        return (ElementRemove::Absent, count);
    }
    let n = (count - 1) as usize;
    match slots.iter().position(|s| *s == this) {
        // Swap-remove of the last slot itself moves nothing (native memmove
        // of zero); report RemovedLast, still decrementing the count.
        Some(i) if i < slots.len() => {
            if i == n {
                (ElementRemove::RemovedLast, count - 1)
            } else {
                (
                    ElementRemove::RemovedShifted { moved_from_last: n },
                    count - 1,
                )
            }
        }
        _ => (ElementRemove::Absent, count),
    }
}

/// 0x3fca0 Duplicate gate + 30-word copy (base result first,
/// gate visual m_Type word == 1 (FRAME_VISUAL) && fourcc==0x45545250
/// (PART_ELEMENT), then copy, return 0).
pub const PART_ELEMENT_FOURCC: u32 = 0x45545250;
pub fn part_element_duplicate_fields(
    base_result: i32,
    src_type: u32,
    src_fourcc: u32,
    src: &PartElementFields,
) -> Result<Option<PartElementFields>, i32> {
    if base_result < 0 {
        return Err(base_result);
    }
    if src_type == 1 && src_fourcc == PART_ELEMENT_FOURCC {
        // Host float-domain refusal maps to the caller's code (disclosed
        // conflation: Err(base_result) is Err(0) on success — identical to
        // I3D_OK. No callers yet; use a distinct error when these gain any).
        for v in [
            src.dir_start,
            src.dir_end,
            src.dir,
            src.dir_prev,
            src.color_start,
            src.color_end,
        ]
        .iter()
        .flatten()
        .chain(
            [
                src.dir_time_stray,
                src.speed_start,
                src.speed_end,
                src.grow,
                src.speed,
                src.lightness,
                src.transparency,
                src.fade,
            ]
            .iter(),
        ) {
            check_f32(*v).map_err(|_| base_result)?;
        }
        Ok(Some(src.clone()))
    } else {
        Ok(None)
    }
}

/// 0x40770 Duplicate gate + 9-word copy (object result first,
/// sector=NULL store, gate m_Type == 1 (FRAME_VISUAL) && (0x42545250 PART_BASE
/// |0x45545250 PART_ELEMENT), then copy; sector copies the source pointer —
/// alias, not deep).
pub fn part_base_duplicate_fields(
    object_result: i32,
    src_type: u32,
    src_fourcc: u32,
    src: &PartBaseFields,
) -> Result<Option<PartBaseFields>, i32> {
    if object_result < 0 {
        return Err(object_result);
    }
    if src_type == 1 && (src_fourcc == 0x42545250 || src_fourcc == PART_ELEMENT_FOURCC) {
        // Same disclosed Err(base_result) conflation as the element twin.
        check_vec(src.color).map_err(|_| object_result)?;
        Ok(Some(src.clone()))
    } else {
        Ok(None)
    }
}

/// Tick-step cap admitted from the 0x3fe40 walk (dt = min(time,10); boundary).
pub fn part_tick_step(time: i32) -> Out<i32> {
    if time <= 0 {
        return Err(ParticleError::InvalidParams);
    }
    Ok(time.min(10))
}

// ---- Contract boundaries (no parity credit) ----
pub fn emitor_duplicate_contract() -> Out<()> {
    Err(ParticleError::ContractBoundary { rva: 0x3f6b0 })
}
pub fn emitor_kill_elements_contract() -> Out<()> {
    Err(ParticleError::ContractBoundary { rva: 0x3f910 })
}
pub fn emitor_tick_elements_contract() -> Out<()> {
    Err(ParticleError::ContractBoundary { rva: 0x3f960 })
}
pub fn emitor_set_elements_on_contract() -> Out<()> {
    Err(ParticleError::ContractBoundary { rva: 0x3f9b0 })
}
pub fn part_element_tick_contract() -> Out<()> {
    Err(ParticleError::ContractBoundary { rva: 0x3fe40 })
}
pub fn part_base_add_fgroups_contract() -> Out<()> {
    Err(ParticleError::ContractBoundary { rva: 0x40820 })
}

#[cfg(test)]
mod particle_y2_tests {
    use super::*;
    struct SeqBackend {
        calls: Vec<&'static str>,
        mesh: Option<u32>,
    }
    impl ParticleMeshBackend for SeqBackend {
        fn base_load_image(&mut self) -> Out<()> {
            self.calls.push("base");
            Ok(())
        }
        fn load_mesh_image(&mut self) -> Out<Option<u32>> {
            self.calls.push("load");
            Ok(self.mesh)
        }
        fn set_mesh(&mut self, m: Option<u32>) -> Out<()> {
            self.calls.push("set");
            assert_eq!(m, self.mesh);
            Ok(())
        }
        fn release_mesh(&mut self, m: u32) -> Out<()> {
            self.calls.push("release");
            assert_eq!(Some(m), self.mesh);
            Ok(())
        }
        fn base_element_ctor(&mut self) -> Out<()> {
            Ok(())
        }
        fn base_element_dtor_tail(&mut self) -> Out<()> {
            Ok(())
        }
        fn base_object_ctor(&mut self) -> Out<()> {
            Ok(())
        }
    }
    #[test]
    fn load_image_tail_order_set_before_release() {
        let mut b = SeqBackend {
            calls: vec![],
            mesh: Some(7),
        };
        assert_eq!(emitor_load_image_tail(&mut b).unwrap(), Some(7));
        assert_eq!(b.calls, ["base", "load", "set", "release"]);
        let mut n = SeqBackend {
            calls: vec![],
            mesh: None,
        };
        assert_eq!(emitor_load_image_tail(&mut n).unwrap(), None);
        assert_eq!(n.calls, ["base", "load", "set"]);
    }
    #[test]
    fn tick_order_dead_swap_moves_last_into_place() {
        let mut slots = vec![10u32, 20, 30];
        let (r, n) = part_element_dtor_remove_index(3, &slots, 10);
        assert_eq!(
            (r, n),
            (ElementRemove::RemovedShifted { moved_from_last: 2 }, 2)
        );
        slots[0] = *slots.last().unwrap();
        slots.pop();
        assert_eq!(slots, [30, 20]);
        // Removing the last slot itself moves nothing.
        let (r, n) = part_element_dtor_remove_index(3, &[10, 20, 30], 30);
        assert_eq!((r, n), (ElementRemove::RemovedLast, 2));
    }
    #[test]
    fn kill_residue_zeroes_count_keeps_slots() {
        let count = 3u32;
        let (r, n) = part_element_dtor_remove_index(count, &[1, 2, 3], 9);
        assert_eq!((r, n), (ElementRemove::Absent, 3));
        let (_, z) = part_element_dtor_remove_index(1, &[5], 5);
        assert_eq!(z, 0);
    }
    #[test]
    fn duplicate_equality_gated_copies() {
        let e = part_element_ctor_defaults().unwrap();
        assert_eq!(
            part_element_duplicate_fields(0, 1, PART_ELEMENT_FOURCC, &e).unwrap(),
            Some(e.clone())
        );
        assert_eq!(
            part_element_duplicate_fields(0, 1, 0x42545250, &e).unwrap(),
            None
        );
        assert_eq!(
            part_element_duplicate_fields(-1, 1, PART_ELEMENT_FOURCC, &e),
            Err(-1)
        );
        let b = part_base_ctor_defaults().unwrap();
        assert_eq!(
            part_base_duplicate_fields(0, 1, 0x42545250, &b).unwrap(),
            Some(b.clone())
        );
        assert_eq!(
            part_base_duplicate_fields(0, 2, 0x42545250, &b).unwrap(),
            None
        );
        assert!(part_element_duplicate_fields(
            0,
            1,
            PART_ELEMENT_FOURCC,
            &PartElementFields {
                dir: [f32::NAN, 0., 0.],
                ..e
            }
        )
        .is_err());
    }
    #[test]
    fn queue_and_render_are_boundaries() {
        // 0x41320 (part_base render arms) is contract via
        // frame_emitor::part_base_render_plan (reference-reported arms, no
        // formed body); the add-fgroups queue and the emitor/element-tick
        // interiors stay boundaries.
        for (f, rva) in [
            (part_base_add_fgroups_contract as fn() -> Out<()>, 0x40820),
            (emitor_duplicate_contract as fn() -> Out<()>, 0x3f6b0),
            (part_element_tick_contract as fn() -> Out<()>, 0x3fe40),
        ] {
            assert_eq!(f(), Err(ParticleError::ContractBoundary { rva }));
        }
        assert_eq!(part_tick_step(25).unwrap(), 10);
        assert_eq!(part_tick_step(7).unwrap(), 7);
    }
}
