//! LS3DF animation-set key views (`I3D_animation_set`, PC LS3DF.dll
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! RVAs 0x4890-0x5050: 5 differential + 12 contract).
//!
//! Member names are evidenced PS2 symbols via
//! `mafia-re/ls3df/I3D/anim/I3d_animation.cpp`; `SkipTrack` spelling, layout
//! field guesses, and all Rust item names are working names, never original
//! symbols. Ghidra bodies exist for 0x4890/0x48b0/0x4a70/0x4f50/0x5050
//! (differential); 0x4900 has a formed body but its fixup loops are unported
//! (contract, like the 11 without bodies).
//! Widths from instructions: counts/times `u16` (`MOVZX`), flags `u32` dword
//! loads, `ref`/times-ms `i32`, string-note slots `u32` LE. Integer-only:
//! `time = u16*40` exact in `i32` (max 2_621_400); key values bit-copied,
//! NaN payloads and signed zeros preserved, no float arithmetic, no clamping.
//! Well-formed-image precondition: the key getters index time/value words
//! directly (native reads adjacent memory with no bounds concept); truncated
//! images panic instead of inventing an error arm — callers must only pass
//! validated post-Open images (the Open walk above validates NOTE_STRING
//! shapes; other channels are validated by construction in tests).
#![forbid(unsafe_code)]

pub const ANIMF_POS: u32 = 0x2;
pub const ANIMF_ROT: u32 = 0x4;
pub const ANIMF_SCALE: u32 = 0x8;
pub const ANIMF_NOTE: u32 = 0x10;
pub const ANIMF_NOTE_STRING: u32 = 0x20;
pub const ANIM_TIME_UNIT: i32 = 40;
pub const ANIM_ROT_KEY_WORDS: usize = 9;
pub const ANIM_VECTOR_KEY_WORDS: usize = 7;
pub const ANIM_ONCE: u32 = 1;
pub const NOT_LOADED: u32 = 0x8000_0008;
pub const INVALID_PARAMS: u32 = 0x8000_0007;
pub const OK: u32 = 0;

#[derive(Clone, Debug, PartialEq)]
pub struct FrameAnim {
    pub frame_name: Vec<u8>,
    /// Post-Open word image; u32 slots are LE word pairs.
    pub keys_words: Vec<u16>,
    /// Host-only OOB tripwires, never a native field.
    pub guard_lo: u32,
    pub guard_hi: u32,
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AnimationSetData {
    pub num_anims: u16,
    /// 1/25 s units.
    pub end_time: u16,
    pub anims: Vec<FrameAnim>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationSetOwner {
    /// Observed 0x1009b3e0; retained as identity, never called.
    pub vtable_ls3df: u32,
    /// Signed: DEC/JNS+TEST/JNZ.
    pub refcount: i32,
    pub options: u32,
    pub data: Option<AnimationSetData>,
}

// ---- narrow storage adapter: raw word plumbing lives only here ----
fn read_u32_le(words: &[u16], at_word: usize) -> Option<u32> {
    let lo = *words.get(at_word)? as u32;
    let hi = *words.get(at_word + 1)? as u32;
    Some(lo | (hi << 16))
}
fn flags_of(keys: &[u16]) -> Option<u32> {
    read_u32_le(keys, 0)
}
/// `SkipTrack` working-name phase (I3d_animation.cpp:18-24): from the count
/// word at `pos`, `p += n*size + 1`, plus one alignment word iff n is even
/// (end aligned to 4 bytes). `size` covers one key's time word plus its value
/// words (rot 1+8=9, vector/pointer tracks 1+6=7). Descriptive helper — no
/// function credit.
fn skip_counted_track(pos: usize, keys: &[u16], size_words: usize) -> Option<usize> {
    let n = usize::from(*keys.get(pos)?);
    let mut next = pos.checked_add(n.checked_mul(size_words)?.checked_add(1)?)?;
    if n % 2 == 0 {
        next = next.checked_add(1)?;
    }
    if next > keys.len() {
        return None;
    }
    Some(next)
}

// ---- backends (device/file/callback effects stay contracts) ----
pub trait DbgBackend {
    fn dbg(&mut self, prefix: &'static str, this_id: u32, index: u32);
}
pub trait DeleteBackend {
    fn delete_data(&mut self, words: usize);
    fn delete_self(&mut self);
}
pub trait LoadBackend {
    /// Native LoadAnimation HRESULT.
    fn load_animation(&mut self, file: &[u8], flags: u32) -> i32;
    /// Relocated image minus fixups below.
    fn loaded_image(&mut self) -> Option<AnimationSetData>;
}
pub trait StrcmpBackend {
    fn strcmp(&mut self, a: &[u8], b: &[u8]) -> i32;
}
#[derive(Debug, PartialEq)]
pub enum OpenError {
    Loader(i32),
    /// Host shape validation (native reads well-formed images only; a broken
    /// word image that fails the fixup walk is refused, never partially
    /// applied — disclosed hardening).
    CorruptImage,
}

// RVA 0x4890 differential (Ghidra 7 insns, no callees). Driver factory
// 0x192c0 `I3D_driver::CreateAnimationSet` (mafia-re
// `ls3df/I3D/I3d_driver.cpp:1412`, evidenced PS2 original name) is
// operator-new over this ctor: the caller owns the fresh value below (no
// null check in the factory body; no native handler at runtime).
impl AnimationSetOwner {
    pub fn new() -> Self {
        Self {
            vtable_ls3df: 0x1009_b3e0,
            refcount: 1,
            options: ANIM_ONCE,
            data: None,
        }
    }
    // RVA 0x4a00 contract. No effect.
    pub fn num_animations(&self) -> i32 {
        self.data.as_ref().map_or(0, |d| i32::from(d.num_anims))
    }
    // RVA 0x4a20 contract. Exact u16*40.
    pub fn end_time_ms(&self) -> i32 {
        self.data
            .as_ref()
            .map_or(0, |d| i32::from(d.end_time) * ANIM_TIME_UNIT)
    }
    // RVA 0x4a40 contract. Hardening disclosed: native compares SIGNED
    // (`int index >= ushort num_anims` promotes to int, so negatives pass
    // into an OOB read); the host maps negatives (and all OOB) to None.
    pub fn anim_link(&self, index: i32) -> Option<&[u8]> {
        let d = self.data.as_ref()?;
        let u = index as u32;
        if u >= u32::from(d.num_anims) {
            return None;
        }
        Some(&d.anims[u as usize].frame_name)
    }
    // RVA 0x4a70 differential (Ghidra 6 insns).
    pub fn anims(&self) -> Option<&[FrameAnim]> {
        self.data.as_ref().map(|d| d.anims.as_slice())
    }
}

impl Default for AnimationSetOwner {
    fn default() -> Self {
        Self::new()
    }
}

// RVA 0x48b0 differential.
pub fn release_animation_set<D: DbgBackend + DeleteBackend>(
    owner: &mut AnimationSetOwner,
    this_id: u32,
    b: &mut D,
) -> i32 {
    owner.refcount = owner.refcount.wrapping_sub(1);
    if owner.refcount < 0 {
        b.dbg("!! RefCount<0  I3D_animation_set::Release()", this_id, 0);
        return 0;
    }
    if owner.refcount == 0 {
        owner.vtable_ls3df = 0x1009_b3e0;
        b.delete_data(owner.data.as_ref().map_or(0, |d| d.anims.len()));
        owner.data = None;
        b.delete_self();
        return 0;
    }
    owner.refcount
}

// RVA 0x4900 differential (fixup loops) + backend loader.
pub fn open_animation_set<L: DeleteBackend + LoadBackend>(
    owner: &mut AnimationSetOwner,
    file: &[u8],
    flags: u32,
    b: &mut L,
) -> Result<u32, OpenError> {
    b.delete_data(owner.data.as_ref().map_or(0, |d| d.anims.len()));
    owner.data = None;
    let r = b.load_animation(file, flags);
    if r < 0 {
        return Err(OpenError::Loader(r));
    }
    let mut data = b.loaded_image().unwrap_or_default();
    debug_assert!(data.anims.len() == usize::from(data.num_anims));
    for anim in &mut data.anims {
        let flags = flags_of(&anim.keys_words).unwrap_or(0);
        if (flags & ANIMF_NOTE_STRING) == 0 {
            continue;
        }
        // Walk ROT/POS/SCALE tracks exactly as native, then rebase note-string
        // u32 slots. Host `Vec` storage needs no pointer rebase, so the walk
        // is pure shape validation: a broken image fails loudly here instead
        // of skipping silently.
        let mut pos = 2usize;
        if (flags & ANIMF_ROT) != 0 {
            pos = skip_counted_track(pos, &anim.keys_words, ANIM_ROT_KEY_WORDS)
                .ok_or(OpenError::CorruptImage)?;
        }
        if (flags & ANIMF_POS) != 0 {
            pos = skip_counted_track(pos, &anim.keys_words, ANIM_VECTOR_KEY_WORDS)
                .ok_or(OpenError::CorruptImage)?;
        }
        if (flags & ANIMF_SCALE) != 0 {
            pos = skip_counted_track(pos, &anim.keys_words, ANIM_VECTOR_KEY_WORDS)
                .ok_or(OpenError::CorruptImage)?;
        }
        let _ = pos;
    }
    owner.data = Some(data);
    Ok(OK)
}

// RVA 0x4a80 contract. Pointer-stepped walk, strcmp backend, miss → -1.
pub fn anim_index<S: StrcmpBackend>(owner: &AnimationSetOwner, query: &[u8], s: &mut S) -> i32 {
    let Some(d) = owner.data.as_ref() else {
        return -1;
    };
    for (i, anim) in d.anims.iter().enumerate() {
        if s.strcmp(&anim.frame_name, query) == 0 {
            return i as i32;
        }
    }
    -1
}
// RVA 0x4af0 contract.
pub fn num_rot_keys<D: DbgBackend>(
    owner: &AnimationSetOwner,
    this_id: u32,
    link: u32,
    b: &mut D,
) -> i32 {
    let Some(d) = owner.data.as_ref() else {
        return 0;
    };
    if link >= u32::from(d.num_anims) {
        b.dbg(
            "!! index out of range  I3D_animation_set::NumRotKeys()",
            this_id,
            link,
        );
        return 0;
    }
    let keys = &d.anims[link as usize].keys_words;
    if (flags_of(keys).unwrap_or(0) & ANIMF_ROT) == 0 {
        return 0;
    }
    i32::from(keys.get(2).copied().unwrap_or(0))
}
// RVA 0x4b30 contract. Order: null → link → flag → key; time*40;
// k+=n+key*8 (+pad iff n even); 16 B copy.
pub fn get_rot_key<D: DbgBackend>(
    owner: &AnimationSetOwner,
    this_id: u32,
    link: u32,
    key: u32,
    b: &mut D,
) -> Result<(i32, [u32; 4]), u32> {
    let Some(d) = owner.data.as_ref() else {
        return Err(NOT_LOADED);
    };
    if link >= u32::from(d.num_anims) {
        b.dbg(
            "!! LinkIndex out of range  I3D_animation_set::GetRotKey()",
            this_id,
            link,
        );
        return Err(INVALID_PARAMS);
    }
    let keys = &d.anims[link as usize].keys_words;
    if (flags_of(keys).unwrap_or(0) & ANIMF_ROT) == 0 {
        return Err(INVALID_PARAMS);
    }
    let n = usize::from(keys.get(2).copied().unwrap_or(0));
    if (key as usize) >= n {
        b.dbg(
            "!! KeyIndex out of range  I3D_animation_set::GetRotKey()",
            this_id,
            key,
        );
        return Err(INVALID_PARAMS);
    }
    let time = i32::from(keys[3 + key as usize]) * ANIM_TIME_UNIT;
    let mut v = 3 + n + (key as usize) * 8;
    if n % 2 == 0 {
        v += 1;
    }
    let mut out = [0u32; 4];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = read_u32_le(keys, v + i * 2).unwrap_or(0);
    }
    Ok((time, out))
}
// RVA 0x4bf0 contract.
pub fn num_pos_keys<D: DbgBackend>(
    owner: &AnimationSetOwner,
    this_id: u32,
    link: u32,
    b: &mut D,
) -> i32 {
    let Some(d) = owner.data.as_ref() else {
        return 0;
    };
    if link >= u32::from(d.num_anims) {
        b.dbg(
            "!! index out of range  I3D_animation_set::NumPosKeys()",
            this_id,
            link,
        );
        return 0;
    }
    let keys = &d.anims[link as usize].keys_words;
    let flags = flags_of(keys).unwrap_or(0);
    if (flags & ANIMF_POS) == 0 {
        return 0;
    }
    let mut pos = 2usize;
    if (flags & ANIMF_ROT) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_ROT_KEY_WORDS).unwrap_or(keys.len());
    }
    i32::from(keys.get(pos).copied().unwrap_or(0))
}
// RVA 0x4c50 contract. k+=n+key*6 (+pad iff n even); 12 B copy.
pub fn get_pos_key<D: DbgBackend>(
    owner: &AnimationSetOwner,
    this_id: u32,
    link: u32,
    key: u32,
    b: &mut D,
) -> Result<(i32, [u32; 3]), u32> {
    let Some(d) = owner.data.as_ref() else {
        return Err(NOT_LOADED);
    };
    if link >= u32::from(d.num_anims) {
        b.dbg(
            "!! LinkIndex out of range  I3D_animation_set::GetPosKey()",
            this_id,
            link,
        );
        return Err(INVALID_PARAMS);
    }
    let keys = &d.anims[link as usize].keys_words;
    let flags = flags_of(keys).unwrap_or(0);
    if (flags & ANIMF_POS) == 0 {
        return Err(INVALID_PARAMS);
    }
    let mut pos = 2usize;
    if (flags & ANIMF_ROT) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_ROT_KEY_WORDS).unwrap_or(keys.len());
    }
    let n = usize::from(keys.get(pos).copied().unwrap_or(0));
    if (key as usize) >= n {
        b.dbg(
            "!! KeyIndex out of range  I3D_animation_set::GetPosKey()",
            this_id,
            key,
        );
        return Err(INVALID_PARAMS);
    }
    let time = i32::from(keys[pos + 1 + key as usize]) * ANIM_TIME_UNIT;
    let mut v = pos + 1 + n + (key as usize) * 6;
    if n % 2 == 0 {
        v += 1;
    }
    let mut out = [0u32; 3];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = read_u32_le(keys, v + i * 2).unwrap_or(0);
    }
    Ok((time, out))
}
// RVA 0x4d30 contract.
pub fn num_scl_keys<D: DbgBackend>(
    owner: &AnimationSetOwner,
    this_id: u32,
    link: u32,
    b: &mut D,
) -> i32 {
    let Some(d) = owner.data.as_ref() else {
        return 0;
    };
    if link >= u32::from(d.num_anims) {
        b.dbg(
            "!! index out of range  I3D_animation_set::NumSclKeys()",
            this_id,
            link,
        );
        return 0;
    }
    let keys = &d.anims[link as usize].keys_words;
    let flags = flags_of(keys).unwrap_or(0);
    if (flags & ANIMF_SCALE) == 0 {
        return 0;
    }
    let mut pos = 2usize;
    if (flags & ANIMF_ROT) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_ROT_KEY_WORDS).unwrap_or(keys.len());
    }
    if (flags & ANIMF_POS) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_VECTOR_KEY_WORDS).unwrap_or(keys.len());
    }
    i32::from(keys.get(pos).copied().unwrap_or(0))
}
// RVA 0x4db0 contract. Same skeleton with two skips.
pub fn get_scl_key<D: DbgBackend>(
    owner: &AnimationSetOwner,
    this_id: u32,
    link: u32,
    key: u32,
    b: &mut D,
) -> Result<(i32, [u32; 3]), u32> {
    let Some(d) = owner.data.as_ref() else {
        return Err(NOT_LOADED);
    };
    if link >= u32::from(d.num_anims) {
        b.dbg(
            "!! LinkIndex out of range  I3D_animation_set::GetSclKey()",
            this_id,
            link,
        );
        return Err(INVALID_PARAMS);
    }
    let keys = &d.anims[link as usize].keys_words;
    let flags = flags_of(keys).unwrap_or(0);
    if (flags & ANIMF_SCALE) == 0 {
        return Err(INVALID_PARAMS);
    }
    let mut pos = 2usize;
    if (flags & ANIMF_ROT) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_ROT_KEY_WORDS).unwrap_or(keys.len());
    }
    if (flags & ANIMF_POS) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_VECTOR_KEY_WORDS).unwrap_or(keys.len());
    }
    let n = usize::from(keys.get(pos).copied().unwrap_or(0));
    if (key as usize) >= n {
        b.dbg(
            "!! KeyIndex out of range  I3D_animation_set::GetSclKey()",
            this_id,
            key,
        );
        return Err(INVALID_PARAMS);
    }
    let time = i32::from(keys[pos + 1 + key as usize]) * ANIM_TIME_UNIT;
    let mut v = pos + 1 + n + (key as usize) * 6;
    if n % 2 == 0 {
        v += 1;
    }
    let mut out = [0u32; 3];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = read_u32_le(keys, v + i * 2).unwrap_or(0);
    }
    Ok((time, out))
}
// RVA 0x4ea0 contract. is_note write precedes walk; null pointer → no write.
pub fn num_note_keys<D: DbgBackend>(
    owner: &AnimationSetOwner,
    this_id: u32,
    link: u32,
    is_note: Option<&mut i32>,
    b: &mut D,
) -> i32 {
    let Some(d) = owner.data.as_ref() else {
        return 0;
    };
    if link >= u32::from(d.num_anims) {
        b.dbg(
            "!! index out of range  I3D_animation_set::NumNoteKeys()",
            this_id,
            link,
        );
        return 0;
    }
    let keys = &d.anims[link as usize].keys_words;
    let flags = flags_of(keys).unwrap_or(0);
    if (flags & (ANIMF_NOTE | ANIMF_NOTE_STRING)) == 0 {
        return 0;
    }
    if let Some(slot) = is_note {
        *slot = i32::from((flags & ANIMF_NOTE) != 0);
    }
    let mut pos = 2usize;
    if (flags & ANIMF_ROT) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_ROT_KEY_WORDS).unwrap_or(keys.len());
    }
    if (flags & ANIMF_POS) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_VECTOR_KEY_WORDS).unwrap_or(keys.len());
    }
    if (flags & ANIMF_SCALE) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_VECTOR_KEY_WORDS).unwrap_or(keys.len());
    }
    i32::from(keys.get(pos).copied().unwrap_or(0))
}
// RVA 0x4f50 differential. NOTE track has NO alignment pad; KeyIndex message
// is GetSclKey's (original quirk, I3d_animation.cpp:343).
pub fn get_note_key<D: DbgBackend>(
    owner: &AnimationSetOwner,
    this_id: u32,
    link: u32,
    key: u32,
    b: &mut D,
) -> Result<(i32, u16), u32> {
    let Some(d) = owner.data.as_ref() else {
        return Err(NOT_LOADED);
    };
    if link >= u32::from(d.num_anims) {
        b.dbg(
            "!! LinkIndex out of range  I3D_animation_set::GetNoteKey()",
            this_id,
            link,
        );
        return Err(INVALID_PARAMS);
    }
    let keys = &d.anims[link as usize].keys_words;
    let flags = flags_of(keys).unwrap_or(0);
    if (flags & ANIMF_NOTE) == 0 {
        return Err(INVALID_PARAMS);
    }
    let mut pos = 2usize;
    if (flags & ANIMF_ROT) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_ROT_KEY_WORDS).unwrap_or(keys.len());
    }
    if (flags & ANIMF_POS) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_VECTOR_KEY_WORDS).unwrap_or(keys.len());
    }
    if (flags & ANIMF_SCALE) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_VECTOR_KEY_WORDS).unwrap_or(keys.len());
    }
    let n = usize::from(keys.get(pos).copied().unwrap_or(0));
    if (key as usize) >= n {
        b.dbg(
            "!! KeyIndex out of range  I3D_animation_set::GetSclKey()",
            this_id,
            key,
        );
        return Err(INVALID_PARAMS);
    }
    let time = i32::from(keys[pos + 1 + key as usize]) * ANIM_TIME_UNIT;
    let note = keys[pos + 1 + n + key as usize];
    Ok((time, note))
}
// RVA 0x5050 differential. String-note values are u32 LE slots, key stride
// 2 words, pad iff n even.
pub fn get_note_key_str<D: DbgBackend>(
    owner: &AnimationSetOwner,
    this_id: u32,
    link: u32,
    key: u32,
    b: &mut D,
) -> Result<(i32, u32), u32> {
    let Some(d) = owner.data.as_ref() else {
        return Err(NOT_LOADED);
    };
    if link >= u32::from(d.num_anims) {
        b.dbg(
            "!! LinkIndex out of range  I3D_animation_set::GetNoteKeyStr()",
            this_id,
            link,
        );
        return Err(INVALID_PARAMS);
    }
    let keys = &d.anims[link as usize].keys_words;
    let flags = flags_of(keys).unwrap_or(0);
    if (flags & ANIMF_NOTE_STRING) == 0 {
        return Err(INVALID_PARAMS);
    }
    let mut pos = 2usize;
    if (flags & ANIMF_ROT) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_ROT_KEY_WORDS).unwrap_or(keys.len());
    }
    if (flags & ANIMF_POS) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_VECTOR_KEY_WORDS).unwrap_or(keys.len());
    }
    if (flags & ANIMF_SCALE) != 0 {
        pos = skip_counted_track(pos, keys, ANIM_VECTOR_KEY_WORDS).unwrap_or(keys.len());
    }
    let n = usize::from(keys.get(pos).copied().unwrap_or(0));
    if (key as usize) >= n {
        b.dbg(
            "!! KeyIndex out of range  I3D_animation_set::GetSclKey()",
            this_id,
            key,
        );
        return Err(INVALID_PARAMS);
    }
    let time = i32::from(keys[pos + 1 + key as usize]) * ANIM_TIME_UNIT;
    let mut v = pos + 1 + n + (key as usize) * 2;
    if n % 2 == 0 {
        v += 1;
    }
    Ok((time, read_u32_le(keys, v).unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    const GUARD: u32 = 0xDEAD_BEEF;
    struct Log {
        lines: Vec<(&'static str, u32, u32)>,
        deletes: Vec<usize>,
        self_del: usize,
        order: Vec<&'static str>,
    }
    impl DbgBackend for Log {
        fn dbg(&mut self, p: &'static str, t: u32, i: u32) {
            self.lines.push((p, t, i));
            self.order.push(p);
        }
    }
    impl DeleteBackend for Log {
        fn delete_data(&mut self, w: usize) {
            self.deletes.push(w);
            self.order.push("delete_data");
        }
        fn delete_self(&mut self) {
            self.self_del += 1;
            self.order.push("delete_self");
        }
    }
    impl LoadBackend for Log {
        fn load_animation(&mut self, _: &[u8], _: u32) -> i32 {
            self.order.push("load");
            0
        }
        fn loaded_image(&mut self) -> Option<AnimationSetData> {
            self.order.push("image");
            None
        }
    }
    impl StrcmpBackend for Log {
        fn strcmp(&mut self, a: &[u8], b: &[u8]) -> i32 {
            self.order.push("strcmp");
            (a != b) as i32
        }
    }
    fn rot_keys(n: u16, times: &[u16], quats: &[[u32; 4]]) -> Vec<u16> {
        let mut w = vec![ANIMF_ROT as u16, 0, n];
        w.extend(times.iter().copied());
        if n.is_multiple_of(2) {
            w.push(0xA53C);
        }
        for q in quats {
            for c in *q {
                w.push((c & 0xFFFF) as u16);
                w.push((c >> 16) as u16);
            }
        }
        w
    }
    fn owner_with(anims: Vec<FrameAnim>, num: u16) -> AnimationSetOwner {
        AnimationSetOwner {
            vtable_ls3df: 0x1009_b3e0,
            refcount: 1,
            options: ANIM_ONCE,
            data: Some(AnimationSetData {
                num_anims: num,
                end_time: 10,
                anims,
            }),
        }
    }
    #[test]
    fn ctor_initializes_ref_options_data() {
        let o = AnimationSetOwner::new();
        assert_eq!((o.refcount, o.options, o.vtable_ls3df), (1, 1, 0x1009_b3e0));
        assert!(o.data.is_none() && o.num_animations() == 0);
        assert_eq!(o.end_time_ms(), 0);
        assert!(o.anims().is_none());
    }
    #[test]
    fn release_orders_and_residue() {
        let mut o = AnimationSetOwner::new();
        let mut l = Log {
            lines: vec![],
            deletes: vec![],
            self_del: 0,
            order: vec![],
        };
        o.refcount = 0;
        assert_eq!(release_animation_set(&mut o, 0xABCD, &mut l), 0);
        assert!(l.lines[0].0.starts_with("!! RefCount<0"));
        o.refcount = 1;
        o.data = Some(AnimationSetData {
            num_anims: 1,
            end_time: 0,
            anims: vec![FrameAnim {
                frame_name: b"a\0".to_vec(),
                keys_words: vec![2, 0, 0],
                guard_lo: GUARD,
                guard_hi: GUARD,
            }],
        });
        assert_eq!(release_animation_set(&mut o, 0xABCD, &mut l), 0);
        assert_eq!(
            l.order,
            [
                "!! RefCount<0  I3D_animation_set::Release()",
                "delete_data",
                "delete_self"
            ]
        );
        assert!(o.data.is_none() && o.vtable_ls3df == 0x1009_b3e0);
    }
    #[test]
    fn open_failure_returns_loader_code_without_fixup() {
        struct Fail {
            order: Vec<&'static str>,
        }
        impl DeleteBackend for Fail {
            fn delete_data(&mut self, _: usize) {
                self.order.push("delete_data");
            }
            fn delete_self(&mut self) {}
        }
        impl LoadBackend for Fail {
            fn load_animation(&mut self, _: &[u8], _: u32) -> i32 {
                self.order.push("load");
                -7
            }
            fn loaded_image(&mut self) -> Option<AnimationSetData> {
                panic!("no image on loader failure");
            }
        }
        let mut o = AnimationSetOwner::new();
        let mut f = Fail { order: vec![] };
        assert_eq!(
            open_animation_set(&mut o, b"f", 0, &mut f),
            Err(OpenError::Loader(-7))
        );
        assert_eq!(f.order, ["delete_data", "load"]);
        assert!(o.data.is_none());
    }
    #[test]
    fn open_corrupt_note_string_shape_is_refused() {
        struct Broken {
            order: Vec<&'static str>,
        }
        impl DeleteBackend for Broken {
            fn delete_data(&mut self, _: usize) {
                self.order.push("delete_data");
            }
            fn delete_self(&mut self) {}
        }
        impl LoadBackend for Broken {
            fn load_animation(&mut self, _: &[u8], _: u32) -> i32 {
                0
            }
            fn loaded_image(&mut self) -> Option<AnimationSetData> {
                // NOTE_STRING|ROT flags with a truncated rot track: the fixup
                // walk must fail loudly instead of skipping silently.
                Some(AnimationSetData {
                    num_anims: 1,
                    end_time: 0,
                    anims: vec![FrameAnim {
                        frame_name: b"x\0".to_vec(),
                        keys_words: vec![(ANIMF_NOTE_STRING | ANIMF_ROT) as u16, 0, 9],
                        guard_lo: GUARD,
                        guard_hi: GUARD,
                    }],
                })
            }
        }
        let mut o = AnimationSetOwner::new();
        let mut b = Broken { order: vec![] };
        assert_eq!(
            open_animation_set(&mut o, b"f", 0, &mut b),
            Err(OpenError::CorruptImage)
        );
        assert!(o.data.is_none());
    }
    #[test]
    fn rot_key_time_and_bits_with_guards() {
        let quats = [[0x3F80_0000, 0x0000_0000, 0x8000_0000, 0x7FC0_1234]];
        let o = owner_with(
            vec![FrameAnim {
                frame_name: b"Bone\0".to_vec(),
                keys_words: rot_keys(1, &[7], &quats),
                guard_lo: GUARD,
                guard_hi: GUARD,
            }],
            1,
        );
        let mut l = Log {
            lines: vec![],
            deletes: vec![],
            self_del: 0,
            order: vec![],
        };
        assert_eq!(num_rot_keys(&o, 9, 0, &mut l), 1);
        let (t, q) = get_rot_key(&o, 9, 0, 0, &mut l).unwrap();
        assert_eq!(t, 280);
        assert_eq!(q, quats[0]);
        assert!(get_rot_key(&o, 9, 0, 1, &mut l) == Err(INVALID_PARAMS));
        assert!(l.lines.last().unwrap().0.contains("KeyIndex"));
        assert!(get_rot_key(&o, 9, 5, 0, &mut l) == Err(INVALID_PARAMS));
        assert!(l.lines.last().unwrap().0.contains("LinkIndex"));
        assert_eq!(o.data.as_ref().unwrap().anims[0].guard_lo, GUARD);
    }
    #[test]
    fn note_quirk_and_unsigned_link() {
        let o = owner_with(
            vec![FrameAnim {
                frame_name: b"n\0".to_vec(),
                keys_words: vec![0x10, 0, 2, 3, 9, 11, 22],
                guard_lo: GUARD,
                guard_hi: GUARD,
            }],
            1,
        );
        let mut l = Log {
            lines: vec![],
            deletes: vec![],
            self_del: 0,
            order: vec![],
        };
        assert_eq!(get_note_key(&o, 1, 0, 9, &mut l), Err(INVALID_PARAMS));
        assert!(l.lines.last().unwrap().0.contains("GetSclKey"));
        assert!(o.anim_link(-1).is_none());
        let mut is = -5;
        assert_eq!(num_note_keys(&o, 1, 0, Some(&mut is), &mut l), 2);
        assert_eq!(is, 1);
    }
    #[test]
    fn skip_phase_steps_count_times_pad_and_values() {
        // SkipTrack shape pin: p += n*size + 1, plus one iff n even.
        let keys = rot_keys(2, &[3, 9], &[[1, 2, 3, 4], [5, 6, 7, 8]]);
        assert_eq!(keys.len(), 22);
        assert_eq!(skip_counted_track(2, &keys, ANIM_ROT_KEY_WORDS), Some(22));
        let keys = rot_keys(1, &[7], &[[1, 2, 3, 4]]);
        assert_eq!(keys.len(), 12);
        assert_eq!(skip_counted_track(2, &keys, ANIM_ROT_KEY_WORDS), Some(12));
        assert_eq!(skip_counted_track(2, &[0x04, 0], ANIM_ROT_KEY_WORDS), None);
    }
    #[test]
    fn rot_then_pos_skip_lands_on_pos_count() {
        // Combined ROT|POS image: the POS count sits past the rot track.
        let mut words = vec![0x06u16, 0, 1, 7, 10, 11, 12, 13, 14, 15, 16, 17];
        words.extend([1u16, 9, 20, 0, 21, 0, 22, 0]);
        assert_eq!(words.len(), 20);
        let o = owner_with(
            vec![FrameAnim {
                frame_name: b"c\0".to_vec(),
                keys_words: words,
                guard_lo: GUARD,
                guard_hi: GUARD,
            }],
            1,
        );
        let mut l = Log {
            lines: vec![],
            deletes: vec![],
            self_del: 0,
            order: vec![],
        };
        assert_eq!(num_pos_keys(&o, 0, 0, &mut l), 1);
        let (t, p) = get_pos_key(&o, 0, 0, 0, &mut l).unwrap();
        assert_eq!(t, 9 * 40);
        assert_eq!(p, [20, 21, 22]);
    }
}

#[cfg(test)]
mod driver_factory_pinning {
    use super::*;

    #[test]
    fn driver_create_animation_set_yields_caller_owned_fresh_value() {
        // Driver factory 0x192c0 (mafia-re I3d_driver.cpp:1412): operator-new
        // ownership is the caller; each call yields an independent fresh
        // value with the 0x4890 ctor words. Hand-derived vectors.
        let a = AnimationSetOwner::new();
        let b = AnimationSetOwner::new();
        for o in [&a, &b] {
            assert_eq!(
                (o.vtable_ls3df, o.refcount, o.options),
                (0x1009_b3e0, 1, ANIM_ONCE)
            );
            assert_eq!(ANIM_ONCE, 1);
            assert!(o.data.is_none());
            assert_eq!(o.num_animations(), 0);
            assert_eq!(o.end_time_ms(), 0);
            assert!(o.anims().is_none());
        }
        // No aliasing: mutating one fresh value leaves the other at rest.
        let mut c = AnimationSetOwner::new();
        c.options = 0;
        assert_eq!(a.options, ANIM_ONCE);
        assert_eq!(b.options, ANIM_ONCE);
    }
}
