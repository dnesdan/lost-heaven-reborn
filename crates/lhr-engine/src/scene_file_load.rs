#![forbid(unsafe_code)]
//! 4ds/5ds/6ds file-loader contracts (PC LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! RVAs 0x60390-0x61450: 8 contract).
//!
//! Names (LoadHierarchy, LoadImage, LoadMeshImage, …) are mafia-re
//! (I3D/load/Load.cpp, Load5ds.cpp, load4ds.cpp) / PS2 working names, NEVER
//! original symbols. All 8 have Ghidra bodies with callee/caller/string
//! evidence. File/clock/driver calls stay backend contracts; the semantic
//! layer owns validation order, extension rewriting, error codes and effect
//! logs. Widths: u16 version/counts, u8 type/lod counts, i32 sizes, 269-char
//! name buffer, FILETIME as [u32;2]. HRESULT codes from ls3df.h:21-42.

pub const I3D_4DS_VERSION: u16 = 29;
pub const I3D_5DS_VERSION: u16 = 20;
pub const I3D_6DS_VERSION: u16 = 2;
pub const NAME_CUT: usize = 269;
pub const DEADLOCK_MS: u32 = 4000;
pub const I3DLOAD_PROGRESS: u32 = 2;

// HRESULT ABI words above i32 range are required (ls3df.h:21-42); the
// unportable-discriminant lint does not apply to this 32-bit ABI enum.
#[allow(clippy::enum_clike_unportable_variant)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStatus {
    Ok = 0,
    NoFile = 0x80000001,
    OutOfMem = 0x80000002,
    Generic = 0x80000005,
    InvalidParams = 0x80000007,
    FileCorrupted = 0x8000000a,
    UserBreak = 0x80000011,
    Filesystem = 0x80000012,
    BadVersion = 0x80000013,
    BadTimestamp = 0x80000014,
}
impl LoadStatus {
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

pub trait LoadFileBackend {
    /// <0 = missing.
    fn open(&mut self, name: &str) -> i32;
    fn read(&mut self, h: i32, n: usize) -> Option<Vec<u8>>;
    fn close(&mut self, h: i32);
    fn delete(&mut self, name: &str);
    fn now_ms(&mut self) -> u32;
    fn pump_messages(&mut self);
    fn progress(&mut self, id: u32, bits: u32, step: u32);
    fn log(&mut self, msg: &'static str);
}

pub trait ShadowGeometrySink {
    fn reserve_verts(&mut self, n: usize);
    fn push_vert(&mut self, v: [f32; 3]);
    fn reserve_faces(&mut self, n: usize);
    fn push_face(&mut self, f: u16);
    fn reserve_parts(&mut self, n: usize);
    fn push_part(&mut self, nv: u32, nf: u32, name: Vec<u8>);
}

/// Rewrite trailing extension in a 269-cut buffer. Native scans back for
/// the last '.'; the rewrite is refused exactly when: no dot found or
/// `i<=0` (stems of length <=1 are refused — `a.4ds` is INVALIDPARAMS
/// natively), or the tail already equals `want` (case-insensitive — the
/// already-correct-extension rejection). The native `*ext=='\\'` arm tests
/// the dot position itself (dead once found), so it is not ported: `ab.\\x`
/// rewrites to `ab.4ds` exactly like the source. Returns the NUL-terminated
/// buffer.
pub fn rewrite_extension(file_name: &str, want: &str) -> Result<Vec<u8>, LoadStatus> {
    if file_name.is_empty() {
        return Err(LoadStatus::InvalidParams);
    }
    let mut name: Vec<u8> = file_name.bytes().take(NAME_CUT).collect();
    let dot = name.iter().rposition(|&b| b == b'.');
    let Some(d) = dot else {
        return Err(LoadStatus::InvalidParams);
    };
    if d <= 1 {
        return Err(LoadStatus::InvalidParams);
    }
    let tail = &name[d..];
    if tail.eq_ignore_ascii_case(want.as_bytes()) {
        return Err(LoadStatus::InvalidParams);
    }
    name.truncate(d);
    name.extend_from_slice(want.as_bytes());
    name.push(0);
    Ok(name)
}

/// F1 0x60390 LoadProgress (70B/19insn/4BB). Literal gate `p<last+0.02f`
/// (f32 — the source spells `.02f`, no double promotion). NaN takes the
/// early-return through the x87 TEST-AH-0x41 unordered path (C0=1), modeled
/// here as an explicit NaN refusal with identical outcomes; +Inf is admitted
/// exactly like the source (stores + calls back). Then IFF PROGRESS flag:
/// last=progress=p + cb(8,bits,1,ctx).
pub fn report_load_progress(
    b: &mut dyn LoadFileBackend,
    last: &mut f32,
    p: f32,
    flags: u32,
    ctx_bits: u32,
) {
    if p.is_nan() || last.is_nan() || p < *last + 0.02 {
        return;
    }
    if flags & I3DLOAD_PROGRESS != 0 {
        *last = p;
        b.progress(8, ctx_bits, 1);
    }
}

/// Deadlock-guarded begin shared by F2/F3/F4: pump loop until free or
/// 4000ms. Order log: pump-messages events then "deadlock" or "begin"
/// (`pump_messages` owns its own log event; no double log here). The
/// "begin"/"deadlock" words are harness-invented test hooks (native emits
/// dbgPrintfs); only their order is pinned.
pub fn begin_guarded_load(
    b: &mut dyn LoadFileBackend,
    busy: &mut bool,
    file: &str,
) -> Result<(), LoadStatus> {
    if *busy {
        let t = b.now_ms();
        loop {
            b.pump_messages();
            if b.now_ms().wrapping_sub(t) > DEADLOCK_MS {
                b.log("deadlock");
                return Err(LoadStatus::Generic);
            }
            if !*busy {
                break;
            }
        }
    }
    *busy = true;
    b.log("begin");
    let _ = file;
    Ok(())
}

/// F5 0x61800 LoadAnimation5ds (353B/139insn/14BB). Order: null→
/// INVALIDPARAMS (no empty-name check: the source checks null only, so ""
/// flows to dtaOpen → NOFILE); open<0→NOFILE; 4-byte id=="5DS\\0" (4th byte
/// must be 0); u16 version==20 else BADVERSION; 8-byte FILETIME +
/// CompareFileTime IFF time given else BADTIMESTAMP… (note: without a time
/// argument the filetime is consumed, never compared); i32 size +
/// `new char[size]` (None→OUTOFMEM + fatal handler note); read; close;
/// *data=buf; OK. Every exit after open closes the handle, like the source.
/// Short reads report FileCorrupted (disclosed hardening — native ignores
/// dtaRead's count); negative size reports FileCorrupted after closing
/// (disclosed hardening — native `new char[neg]` throws).
pub fn load_animation_5ds(
    b: &mut dyn LoadFileBackend,
    file: &str,
    time: Option<[u32; 2]>,
) -> Result<Vec<u8>, LoadStatus> {
    let h = b.open(file);
    if h < 0 {
        return Err(LoadStatus::NoFile);
    }
    let mut take = |n: usize| -> Result<Vec<u8>, LoadStatus> {
        b.read(h, n)
            .filter(|v| v.len() == n)
            .ok_or(LoadStatus::FileCorrupted)
    };
    let id = match take(4) {
        Ok(x) => x,
        Err(_) => {
            b.close(h);
            return Err(LoadStatus::FileCorrupted);
        }
    };
    if id[0] != b'5' || id[1] != b'D' || id[2] != b'S' || id[3] != 0 {
        b.close(h);
        return Err(LoadStatus::FileCorrupted);
    }
    let v = match take(2) {
        Ok(x) => x,
        Err(_) => {
            b.close(h);
            return Err(LoadStatus::FileCorrupted);
        }
    };
    if u16::from_le_bytes([v[0], v[1]]) != I3D_5DS_VERSION {
        b.close(h);
        return Err(LoadStatus::BadVersion);
    }
    let ft = match take(8) {
        Ok(x) => x,
        Err(_) => {
            b.close(h);
            return Err(LoadStatus::FileCorrupted);
        }
    };
    if let Some(t) = time {
        let f = [
            u32::from_le_bytes([ft[0], ft[1], ft[2], ft[3]]),
            u32::from_le_bytes([ft[4], ft[5], ft[6], ft[7]]),
        ];
        if f != t {
            b.close(h);
            return Err(LoadStatus::BadTimestamp);
        }
    }
    let s = match take(4) {
        Ok(x) => x,
        Err(_) => {
            b.close(h);
            return Err(LoadStatus::FileCorrupted);
        }
    };
    let size = i32::from_le_bytes([s[0], s[1], s[2], s[3]]);
    if size < 0 {
        b.close(h);
        return Err(LoadStatus::FileCorrupted);
    }
    let mut buf = Vec::new();
    if buf.try_reserve(size as usize).is_err() {
        b.close(h);
        b.log("oom-fatal");
        return Err(LoadStatus::OutOfMem);
    }
    buf.resize(size as usize, 0);
    let data = match take(size as usize) {
        Ok(x) => x,
        Err(_) => {
            b.close(h);
            return Err(LoadStatus::FileCorrupted);
        }
    };
    buf.copy_from_slice(&data);
    b.close(h);
    b.log("data-kept");
    Ok(buf)
}

/// F3 0x60840 LoadAnimation (516B/164insn/30BB). Order: set/null→
/// INVALIDPARAMS; loadingAnim guard (own flag); ext→.5ds; LoadAnimation5ds;
/// switch: OK/USERBREAK silent, FILESYSTEM+dtaDelete, else dbg;
/// loadingAnim=false; return r.
type LoadAnimation5dsFn = dyn Fn(&mut dyn LoadFileBackend, &str) -> Result<Vec<u8>, LoadStatus>;
pub fn load_animation(
    b: &mut dyn LoadFileBackend,
    busy: &mut bool,
    file: &str,
    set_present: bool,
    load_5ds: &LoadAnimation5dsFn,
    keep: &mut dyn FnMut(Vec<u8>),
) -> LoadStatus {
    if !set_present || file.is_empty() {
        b.log("invalidparams");
        return LoadStatus::InvalidParams;
    }
    if begin_guarded_load(b, busy, file).is_err() {
        return LoadStatus::Generic;
    }
    let name = match rewrite_extension(file, ".5ds") {
        Ok(n) => n,
        Err(e) => {
            *busy = false;
            return e;
        }
    };
    // C-string semantics: the NUL terminator is not part of the name.
    let s = String::from_utf8_lossy(&name[..name.len() - 1]).into_owned();
    let r = match load_5ds(b, &s) {
        Ok(d) => {
            keep(d);
            LoadStatus::Ok
        }
        Err(LoadStatus::UserBreak) => LoadStatus::UserBreak,
        Err(LoadStatus::Filesystem) => {
            b.delete(&s);
            LoadStatus::Filesystem
        }
        Err(e) => {
            b.log("open-failed");
            e
        }
    };
    *busy = false;
    r
}

/// F4 0x60a50 LoadShadow (419B/132insn/23BB): same guard shape with the
/// shared `loading` flag; ext→.6ds; LoadShadowImage; BADVERSION→dbg; always
/// `loading=false` before return (all paths).
pub fn load_shadow(
    b: &mut dyn LoadFileBackend,
    busy: &mut bool,
    file: &str,
    def_present: bool,
    image: &dyn Fn(&mut dyn LoadFileBackend, &str) -> LoadStatus,
) -> LoadStatus {
    if !def_present || file.is_empty() {
        return LoadStatus::InvalidParams;
    }
    if begin_guarded_load(b, busy, file).is_err() {
        return LoadStatus::Generic;
    }
    let name = match rewrite_extension(file, ".6ds") {
        Ok(n) => n,
        Err(e) => {
            *busy = false;
            return e;
        }
    };
    // C-string semantics: the NUL terminator is not part of the name.
    let s = String::from_utf8_lossy(&name[..name.len() - 1]).into_owned();
    let r = image(b, &s);
    if r == LoadStatus::BadVersion {
        b.log("bad-version");
    }
    *busy = false;
    r
}

/// F8 0x61450 LoadShadowImage (827B/282insn/39BB). Order: null→
/// INVALIDPARAMS; open<0→NOFILE; "6DS\\0"; u16==2; timestamp; ShadowInfo
/// (u32×3); verts (S_vector f32×3); faces u16; parts: (u32 nv, u32 nf) + u8
/// len + `new char[len+1]` + bytes + NUL. Callee 0x10061790 = per-part store
/// (backend `push_part` contract).
pub fn load_shadow_image(
    b: &mut dyn LoadFileBackend,
    sink: &mut dyn ShadowGeometrySink,
    file: &str,
    time: Option<[u32; 2]>,
) -> LoadStatus {
    if file.is_empty() {
        return LoadStatus::InvalidParams;
    }
    let h = b.open(file);
    if h < 0 {
        return LoadStatus::NoFile;
    }
    let mut take = |n: usize| -> Result<Vec<u8>, LoadStatus> {
        b.read(h, n)
            .filter(|v| v.len() == n)
            .ok_or(LoadStatus::FileCorrupted)
    };
    let id = take(4);
    let v = take(2);
    let ft = take(8);
    let (id, v, ft) = match (id, v, ft) {
        (Ok(a), Ok(b_), Ok(c)) => (a, b_, c),
        _ => {
            b.close(h);
            return LoadStatus::FileCorrupted;
        }
    };
    if id != [b'6', b'D', b'S', 0] {
        b.close(h);
        return LoadStatus::FileCorrupted;
    }
    if u16::from_le_bytes([v[0], v[1]]) != I3D_6DS_VERSION {
        b.close(h);
        return LoadStatus::BadVersion;
    }
    if let Some(t) = time {
        let f = [
            u32::from_le_bytes([ft[0], ft[1], ft[2], ft[3]]),
            u32::from_le_bytes([ft[4], ft[5], ft[6], ft[7]]),
        ];
        if f != t {
            b.close(h);
            return LoadStatus::BadTimestamp;
        }
    }
    let si = match take(12) {
        Ok(x) => x,
        Err(_) => {
            b.close(h);
            return LoadStatus::FileCorrupted;
        }
    };
    let (nv, ni, np) = (
        u32::from_le_bytes([si[0], si[1], si[2], si[3]]) as usize,
        u32::from_le_bytes([si[4], si[5], si[6], si[7]]) as usize,
        u32::from_le_bytes([si[8], si[9], si[10], si[11]]) as usize,
    );
    sink.reserve_verts(nv);
    for _ in 0..nv {
        let r = match take(12) {
            Ok(x) => x,
            Err(_) => {
                b.close(h);
                return LoadStatus::FileCorrupted;
            }
        };
        let g = |o: usize| f32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
        sink.push_vert([g(0), g(4), g(8)]);
    }
    sink.reserve_faces(ni);
    for _ in 0..ni {
        let r = match take(2) {
            Ok(x) => x,
            Err(_) => {
                b.close(h);
                return LoadStatus::FileCorrupted;
            }
        };
        sink.push_face(u16::from_le_bytes([r[0], r[1]]));
    }
    sink.reserve_parts(np);
    for _ in 0..np {
        let c = match take(8) {
            Ok(x) => x,
            Err(_) => {
                b.close(h);
                return LoadStatus::FileCorrupted;
            }
        };
        let (cv, cf) = (
            u32::from_le_bytes([c[0], c[1], c[2], c[3]]),
            u32::from_le_bytes([c[4], c[5], c[6], c[7]]),
        );
        let l = match take(1) {
            Ok(x) => x[0] as usize,
            Err(_) => {
                b.close(h);
                return LoadStatus::FileCorrupted;
            }
        };
        let mut nm = vec![0u8; l + 1]; // new char[len+1], NUL kept
        if l > 0 {
            match take(l) {
                Ok(x) => nm[..l].copy_from_slice(&x),
                Err(_) => {
                    b.close(h);
                    return LoadStatus::FileCorrupted;
                }
            }
        }
        sink.push_part(cv, cf, nm);
    }
    b.close(h);
    LoadStatus::Ok
}

/// F2 0x603e0 LoadHierarchy (1117B/343insn/61BB). Order: root/null/empty→
/// INVALIDPARAMS; deadlock guard; last=0; PROGRESS BEGIN (progress=0, before
/// the rewrite); ext→.4ds (269-cut, bad names report END); LoadImage; owner
/// fan-out (model vs scene); result switch (OK silent, FILESYSTEM+dtaDelete
/// of the rewritten name, BADVERSION dbg, else dbg); root UPDATE+Update();
/// PROGRESS END (progress=1) on every post-guard path; loading=false.
/// Returns r. BEGIN/END are gated on flags&I3DLOAD_PROGRESS like the source
/// (Load.cpp:109-111,128-131,172-174,202-204). The image load and owner
/// fan-out stay driver/backend contracts — this owns the guard, name,
/// result-switch, delete and progress framing in source order. RVA 0x603e0
/// is already ledger-credited (frozen port "LS3DF parsed model ownership and
/// frame materialization" via MaterializedModel::from_parsed); this
/// finer-grained same-behavior port takes NO overlay entry (no double count).
pub fn load_hierarchy(
    b: &mut dyn LoadFileBackend,
    busy: &mut bool,
    file: &str,
    flags: u32,
    root_present: bool,
    image_result: LoadStatus,
) -> LoadStatus {
    if !root_present || file.is_empty() {
        b.log("invalidparam");
        return LoadStatus::InvalidParams;
    }
    if begin_guarded_load(b, busy, file).is_err() {
        return LoadStatus::Generic;
    }
    let progress = flags & I3DLOAD_PROGRESS != 0;
    if progress {
        b.progress(8, 0.0f32.to_bits(), 0);
    }
    let name = match rewrite_extension(file, ".4ds") {
        Ok(n) => n,
        Err(_) => {
            b.log("bad-name-end");
            if progress {
                b.progress(8, 1.0f32.to_bits(), 2);
            }
            *busy = false;
            return LoadStatus::InvalidParams;
        }
    };
    // C-string semantics: the NUL terminator is not part of the name.
    let s = String::from_utf8_lossy(&name[..name.len() - 1]).into_owned();
    let r = image_result;
    match r {
        LoadStatus::Ok => {}
        LoadStatus::Filesystem => {
            b.delete(&s);
        }
        LoadStatus::BadVersion => {
            b.log("bad-version");
        }
        _ => {
            b.log("open-failed");
        }
    }
    if progress {
        b.progress(8, 1.0f32.to_bits(), 2);
    }
    *busy = false;
    r
}

/// F6 0x60cb0 LoadImage-4ds phase validators (1127B/353insn/59BB): frame-type
/// gate (visual: vtype<9 + vis_flags&2B; else type<FRAME_LAST=18) and parent
/// gate (1-based, 0=none, parent>j→corrupt — note `>`, `==j` allowed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind4ds {
    Visual(u8),
    Frame(u8),
}
pub const VISUAL_TYPES_4DS: [u32; 9] = [
    0x5f4a424f, 0x50414d4c, 0x4d474e53, 0x524d4d53, 0x44524242, 0x4850524d, 0x534e454c, 0x4a4f5250,
    0x5252494d,
];
pub const VISF_CLEAR_MASK_4DS: u32 = !(0x80000 | 0x100000);
/// F6 helper (load4ds.cpp:140-148): the file's 2-byte flags word replaces
/// vis_flags' LOW half (LE), then `&= ~(VISF_UNKNOWN_80000|VISF_INV_MAT_VALID)`.
pub fn clear_visual_flags_4ds(cur: u32, lo: u16) -> u32 {
    ((cur & 0xFFFF_0000) | u32::from(lo)) & VISF_CLEAR_MASK_4DS
}
pub fn validate_frame_type(ty: u8, vtype: Option<u8>) -> Result<FrameKind4ds, LoadStatus> {
    if ty == 1 {
        // FRAME_VISUAL
        let v = vtype.ok_or(LoadStatus::FileCorrupted)?;
        if (v as usize) >= VISUAL_TYPES_4DS.len() {
            return Err(LoadStatus::FileCorrupted);
        }
        Ok(FrameKind4ds::Visual(v))
    } else {
        if ty >= 18 {
            return Err(LoadStatus::FileCorrupted);
        }
        Ok(FrameKind4ds::Frame(ty))
    }
}
pub fn validate_parent(parent: u16, j: usize) -> Result<Option<usize>, LoadStatus> {
    if parent == 0 {
        return Ok(None);
    }
    if (parent as usize) > j {
        return Err(LoadStatus::FileCorrupted);
    }
    Ok(Some((parent - 1) as usize))
}

/// F7 0x61120 LoadMeshImage prefix classifier (806B/282insn/32BB): u16
/// index; nonzero→share (AddRef, return); u8 lods, 0→NULL; else per-level
/// lod_dist/faces/material flow with AddFGroup/SetLOD/Release tails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshPrefix {
    Shared(u16),
    Levels(u8),
    Empty,
}
pub fn classify_mesh_prefix(index: u16, num_lods: Option<u8>) -> MeshPrefix {
    if index != 0 {
        MeshPrefix::Shared(index)
    } else {
        match num_lods {
            Some(0) | None => MeshPrefix::Empty,
            Some(n) => MeshPrefix::Levels(n),
        }
    }
}

#[cfg(test)]
mod scene_file_load_tests {
    use super::*;
    struct MemBackend {
        files: std::collections::HashMap<String, Vec<u8>>,
        pos: std::collections::HashMap<i32, usize>,
        next: i32,
        now: u32,
        events: Vec<String>,
        progresses: Vec<(u32, u32, u32)>,
    }
    impl MemBackend {
        fn with(files: &[(&str, Vec<u8>)]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
                pos: Default::default(),
                next: 1,
                now: 0,
                events: vec![],
                progresses: vec![],
            }
        }
    }
    impl LoadFileBackend for MemBackend {
        fn open(&mut self, n: &str) -> i32 {
            match self.files.get(n) {
                Some(_) => {
                    let h = self.next;
                    self.next += 1;
                    self.pos.insert(h, 0);
                    h
                }
                None => -1,
            }
        }
        fn read(&mut self, h: i32, n: usize) -> Option<Vec<u8>> {
            // Single-file fixtures: resolve bytes by open handle order.
            let name = self
                .files
                .keys()
                .nth((h - 1) as usize % self.files.len().max(1))
                .cloned()?;
            let all = self.files.get(&name)?.clone();
            let p = *self.pos.get(&h)?;
            let take = all.get(p..(p + n).min(all.len()))?.to_vec();
            *self.pos.get_mut(&h).unwrap() += take.len();
            if take.len() != n {
                None
            } else {
                Some(take)
            }
        }
        fn close(&mut self, h: i32) {
            self.pos.remove(&h);
            self.events.push("close".into());
        }
        fn delete(&mut self, n: &str) {
            self.files.remove(n);
            self.events.push(format!("delete:{n}"));
        }
        fn now_ms(&mut self) -> u32 {
            self.now
        }
        fn pump_messages(&mut self) {
            self.now += 5000;
            self.events.push("pump".into());
        }
        fn progress(&mut self, id: u32, bits: u32, step: u32) {
            self.progresses.push((id, bits, step));
        }
        fn log(&mut self, m: &'static str) {
            self.events.push(m.into());
        }
    }
    #[test]
    fn progress_gate_step_and_nan() {
        let mut b = MemBackend::with(&[]);
        let mut last = 0.0f32;
        report_load_progress(&mut b, &mut last, 0.019, 2, 0);
        assert!(b.progresses.is_empty());
        report_load_progress(&mut b, &mut last, 0.02, 2, 0);
        assert_eq!(b.progresses, vec![(8, 0, 1)]);
        report_load_progress(&mut b, &mut last, f32::NAN, 2, 0);
        assert_eq!(b.progresses.len(), 1);
        // +Inf is admitted like the source (stores + calls back).
        report_load_progress(&mut b, &mut last, f32::INFINITY, 2, 7);
        assert_eq!(b.progresses.len(), 2);
        assert_eq!(b.progresses[1], (8, 7, 1));
        assert_eq!(last, f32::INFINITY);
    }
    #[test]
    fn ext_rewrite_vectors() {
        assert!(rewrite_extension("car.5ds", ".4ds")
            .unwrap()
            .starts_with(b"car.4ds"));
        assert_eq!(
            rewrite_extension("car.4ds", ".4ds").unwrap_err(),
            LoadStatus::InvalidParams
        );
        assert_eq!(
            rewrite_extension("nodot", ".4ds").unwrap_err(),
            LoadStatus::InvalidParams
        );
        assert_eq!(
            rewrite_extension("", ".4ds").unwrap_err(),
            LoadStatus::InvalidParams
        );
        assert_eq!(
            rewrite_extension(".4ds", ".4ds").unwrap_err(),
            LoadStatus::InvalidParams
        );
        // Single-char stems are refused (`i<=0` in the native scan-back).
        assert_eq!(
            rewrite_extension("a.4ds", ".5ds").unwrap_err(),
            LoadStatus::InvalidParams
        );
        assert!(rewrite_extension("ab.4ds", ".5ds")
            .unwrap()
            .starts_with(b"ab.5ds"));
        // Backslash after the dot rewrites like the source (no refusal).
        assert!(rewrite_extension("ab.\\x", ".4ds")
            .unwrap()
            .starts_with(b"ab.4ds"));
    }
    #[test]
    fn deadlock_order_log() {
        struct Stuck(MemBackend);
        impl LoadFileBackend for Stuck {
            fn open(&mut self, n: &str) -> i32 {
                self.0.open(n)
            }
            fn read(&mut self, h: i32, n: usize) -> Option<Vec<u8>> {
                self.0.read(h, n)
            }
            fn close(&mut self, h: i32) {
                self.0.close(h)
            }
            fn delete(&mut self, n: &str) {
                self.0.delete(n)
            }
            fn now_ms(&mut self) -> u32 {
                self.0.now
            }
            fn pump_messages(&mut self) {
                self.0.now += 5000;
                self.0.events.push("pump".into());
            }
            fn progress(&mut self, i: u32, x: u32, s: u32) {
                self.0.progress(i, x, s)
            }
            fn log(&mut self, m: &'static str) {
                self.0.log(m)
            }
        }
        let mut busy = true;
        let mut s = Stuck(MemBackend::with(&[]));
        assert_eq!(
            begin_guarded_load(&mut s, &mut busy, "x"),
            Err(LoadStatus::Generic)
        );
        assert_eq!(s.0.events, vec!["pump", "deadlock"]);
    }
    #[test]
    fn anim5ds_guards_and_residue() {
        let good_id = vec![
            b'5', b'D', b'S', 0, 20, 0, 1, 2, 3, 4, 5, 6, 7, 8, 3, 0, 0, 0, 9, 9, 9,
        ];
        let mut b = MemBackend::with(&[("a.5ds", good_id)]);
        let d = load_animation_5ds(&mut b, "a.5ds", None).unwrap();
        assert_eq!(d, vec![9, 9, 9]);
        assert!(
            b.events.contains(&"close".to_string()) && b.events.contains(&"data-kept".to_string())
        );
        let mut b2 = MemBackend::with(&[("b.5ds", vec![b'5', b'D', b'S', b'X', 20, 0])]);
        assert_eq!(
            load_animation_5ds(&mut b2, "b.5ds", None),
            Err(LoadStatus::FileCorrupted)
        );
        let mut b3 = MemBackend::with(&[("c.5ds", vec![b'5', b'D', b'S', 0, 21, 0])]);
        assert_eq!(
            load_animation_5ds(&mut b3, "c.5ds", None),
            Err(LoadStatus::BadVersion)
        );
        assert_eq!(
            load_animation_5ds(&mut MemBackend::with(&[]), "no.5ds", None),
            Err(LoadStatus::NoFile)
        );
        // No empty-name check in the source: "" flows to open → NoFile.
        assert_eq!(
            load_animation_5ds(&mut MemBackend::with(&[]), "", None),
            Err(LoadStatus::NoFile)
        );
    }
    #[test]
    fn hierarchy_delete_and_progress_framing() {
        // FILESYSTEM deletes the rewritten name; BEGIN(0)/END(2) frame every
        // path with progress 0.0/1.0.
        let mut b = MemBackend::with(&[("mm.4ds", vec![0])]);
        let mut busy = false;
        // "mm.5ds" rewrites to "mm.4ds": the delete targets the rewritten name.
        let r = load_hierarchy(
            &mut b,
            &mut busy,
            "mm.5ds",
            I3DLOAD_PROGRESS,
            true,
            LoadStatus::Filesystem,
        );
        assert_eq!(r, LoadStatus::Filesystem);
        assert!(!busy);
        assert!(b.events.iter().any(|e| e == "delete:mm.4ds"));
        assert_eq!(
            b.progresses,
            vec![(8, 0.0f32.to_bits(), 0), (8, 1.0f32.to_bits(), 2)]
        );
        let mut b2 = MemBackend::with(&[]);
        let mut busy2 = false;
        let r2 = load_hierarchy(
            &mut b2,
            &mut busy2,
            "m2.5ds",
            I3DLOAD_PROGRESS,
            true,
            LoadStatus::Ok,
        );
        assert_eq!(r2, LoadStatus::Ok);
        assert_eq!(
            b2.progresses,
            vec![(8, 0.0f32.to_bits(), 0), (8, 1.0f32.to_bits(), 2)]
        );
        // Bad names report END after BEGIN, like the source bad-name path.
        let mut b3 = MemBackend::with(&[]);
        let mut busy3 = false;
        let r3 = load_hierarchy(
            &mut b3,
            &mut busy3,
            "nodot",
            I3DLOAD_PROGRESS,
            true,
            LoadStatus::Ok,
        );
        assert_eq!(r3, LoadStatus::InvalidParams);
        assert!(!busy3);
        assert_eq!(
            b3.progresses,
            vec![(8, 0.0f32.to_bits(), 0), (8, 1.0f32.to_bits(), 2)]
        );
        // Without the PROGRESS flag the framing stays silent.
        let mut b4 = MemBackend::with(&[]);
        let mut busy4 = false;
        let r4 = load_hierarchy(&mut b4, &mut busy4, "ok.5ds", 0, true, LoadStatus::Ok);
        assert_eq!(r4, LoadStatus::Ok);
        assert!(b4.progresses.is_empty());
    }
    #[test]
    fn visual_flags_low_half_then_mask() {
        // 0xFFFFFFFF low half replaced by 0xFFFF, then bits 19,20 cleared.
        assert_eq!(clear_visual_flags_4ds(0xFFFF_FFFF, 0xFFFF), 0xFFE7_FFFF);
        // 0x12345678 low half -> 0x00FF, then bit 20 (in 0x34) cleared.
        assert_eq!(clear_visual_flags_4ds(0x1234_5678, 0x00FF), 0x1224_00FF);
    }
    #[test]
    fn frame_parent_and_type_bounds() {
        assert_eq!(validate_parent(0, 5), Ok(None));
        assert_eq!(validate_parent(5, 5), Ok(Some(4)));
        assert_eq!(validate_parent(6, 5), Err(LoadStatus::FileCorrupted));
        assert!(validate_frame_type(1, Some(9)).is_err());
        assert!(validate_frame_type(18, None).is_err());
        assert_eq!(validate_frame_type(9, None), Ok(FrameKind4ds::Frame(9)));
    }
    #[test]
    fn mesh_prefix_vectors() {
        assert_eq!(classify_mesh_prefix(3, None), MeshPrefix::Shared(3));
        assert_eq!(classify_mesh_prefix(0, Some(0)), MeshPrefix::Empty);
        assert_eq!(classify_mesh_prefix(0, Some(2)), MeshPrefix::Levels(2));
    }
    #[test]
    fn shadow_image_parts_with_guard() {
        struct Sink {
            verts: Vec<[f32; 3]>,
            faces: Vec<u16>,
            parts: Vec<(u32, u32, Vec<u8>)>,
        }
        impl ShadowGeometrySink for Sink {
            fn reserve_verts(&mut self, n: usize) {
                self.verts.reserve(n);
            }
            fn push_vert(&mut self, v: [f32; 3]) {
                self.verts.push(v);
            }
            fn reserve_faces(&mut self, n: usize) {
                self.faces.reserve(n);
            }
            fn push_face(&mut self, f: u16) {
                self.faces.push(f);
            }
            fn reserve_parts(&mut self, n: usize) {
                self.parts.reserve(n);
            }
            fn push_part(&mut self, a: u32, b: u32, n: Vec<u8>) {
                self.parts.push((a, b, n));
            }
        }
        let mut file = vec![
            b'6', b'D', b'S', 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0,
            0, 0, 128, 63, 0, 0, 0, 64, 0, 0, 64, 64, 7, 0, 4, 0, 0, 0, 5, 0, 0, 0, 3, b'a', b'b',
            b'c',
        ];
        file.push(0xCC);
        let mut b = MemBackend::with(&[("s.6ds", file)]);
        let mut s = Sink {
            verts: vec![],
            faces: vec![],
            parts: vec![],
        };
        assert_eq!(
            load_shadow_image(&mut b, &mut s, "s.6ds", None),
            LoadStatus::Ok
        );
        assert_eq!((s.verts, s.faces), (vec![[1.0, 2.0, 3.0]], vec![7]));
        assert_eq!(s.parts, vec![(4, 5, vec![b'a', b'b', b'c', 0])]);
    }
    #[test]
    fn load_animation_happy_path_and_guards() {
        let mut b = MemBackend::with(&[]);
        let mut busy = false;
        let mut kept = Vec::new();
        assert_eq!(
            load_animation(&mut b, &mut busy, "", false, &|_, _| Ok(vec![]), &mut |d| {
                kept.push(d)
            }),
            LoadStatus::InvalidParams
        );
        assert!(!busy);
        assert_eq!(
            load_animation(
                &mut b,
                &mut busy,
                "ab.4ds",
                true,
                &|_, _| Ok(vec![7]),
                &mut |d| kept.push(d)
            ),
            LoadStatus::Ok
        );
        assert_eq!(kept, vec![vec![7]]);
        assert!(!busy);
        assert_eq!(
            load_animation(
                &mut b,
                &mut busy,
                "ab.4ds",
                true,
                &|_, _| Err(LoadStatus::Filesystem),
                &mut |d| kept.push(d)
            ),
            LoadStatus::Filesystem
        );
        assert!(b.events.iter().any(|e| e.starts_with("delete:")));
    }
    #[test]
    fn load_shadow_happy_path_and_guards() {
        let mut b = MemBackend::with(&[]);
        let mut busy = false;
        assert_eq!(
            load_shadow(&mut b, &mut busy, "", false, &|_, _| LoadStatus::Ok),
            LoadStatus::InvalidParams
        );
        assert_eq!(
            load_shadow(&mut b, &mut busy, "sh.4ds", true, &|_, _| {
                LoadStatus::BadVersion
            }),
            LoadStatus::BadVersion
        );
        assert!(b.events.contains(&"bad-version".to_string()));
        assert!(!busy);
    }
    #[test]
    fn load_hierarchy_guard_and_result_switch() {
        let mut b = MemBackend::with(&[]);
        let mut busy = false;
        assert_eq!(
            load_hierarchy(
                &mut b,
                &mut busy,
                "",
                I3DLOAD_PROGRESS,
                false,
                LoadStatus::Ok
            ),
            LoadStatus::InvalidParams
        );
        assert!(!busy);
        assert_eq!(
            load_hierarchy(
                &mut b,
                &mut busy,
                "mm.5ds",
                I3DLOAD_PROGRESS,
                true,
                LoadStatus::Ok
            ),
            LoadStatus::Ok
        );
        assert_eq!(
            b.progresses,
            vec![(8, 0.0f32.to_bits(), 0), (8, 1.0f32.to_bits(), 2)]
        );
        assert!(!busy);
        assert_eq!(
            load_hierarchy(
                &mut b,
                &mut busy,
                "mm.5ds",
                I3DLOAD_PROGRESS,
                true,
                LoadStatus::BadVersion
            ),
            LoadStatus::BadVersion
        );
        assert!(b.events.contains(&"bad-version".to_string()));
    }
}
