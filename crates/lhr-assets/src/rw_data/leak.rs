//! Leak-flag lane: dtaDumpMemoryLeaks (PC RVA 0x4140), detector init
//! (PC RVA 0x4150) and report (PC RVA 0x4160).
//!
//! Working names: the class/counters are mafia-re guesses
//! (`rw_data_inter.h:44`: "This class doesn't exist in the PS2 build, so the
//! class and member names are guesses"); only the RVAs, widths and dialog
//! bytes are evidenced. No locks: the binary touches the flag/counters
//! without C_m_p, so atomics (not a Mutex) model the lock-free shape without
//! data-race UB. No unsafe.
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// Exact caption bytes at RWDATA 0x1000c748 ("Dta_read error message\0" w/o NUL).
pub const LEAK_CAPTION: &[u8] = b"Dta_read error message";
/// Exact formats (double spaces) at 0x1000c784 / 0x1000c760.
const REF_FORMAT: &[u8] = b"Not released:  %d  reference(s) of Dta_read.\n";
const OBJ_FORMAT: &[u8] = b"Not released:  %d  Dta_read(s).\n";
/// Native stack bounds: buffer[512], temp[128].
const BUFFER_CAP: usize = 512;
const LINE_CAP: usize = 128;

/// Global leak detector state (RWDATA 0x1000ec60: int32 +0, int32 +4, bool +8).
/// Default is zeroed, matching the 0x4150 init (u32 0, u32 0, u8 0).
#[derive(Debug, Default)]
pub struct LeakDetector {
    ref_count: AtomicI32,
    obj_count: AtomicI32,
    dump_memory_leaks: AtomicBool,
}

impl LeakDetector {
    /// PC RVA 0x4150 init: ref_count(32b)=0, obj_count(32b)=0, dump(8b)=false.
    /// REA ev_f16f84b0 (6 instr, 1 BB, caller 0x10001140).
    pub fn new() -> Self {
        Self {
            ref_count: AtomicI32::new(0),
            obj_count: AtomicI32::new(0),
            dump_memory_leaks: AtomicBool::new(false),
        }
    }

    /// PC RVA 0x4140 (via C_l_d.SetDumpMemoryLeaks): single byte store, no
    /// lock. REA ev_5613ea83 (3 instr, 1 BB, store to 0x1000ec68).
    pub fn set_dump_memory_leaks(&self, dump: bool) {
        self.dump_memory_leaks.store(dump, Ordering::Relaxed);
    }

    pub fn dump_memory_leaks(&self) -> bool {
        self.dump_memory_leaks.load(Ordering::Relaxed)
    }

    /// Host-side counter maintenance for future AddRef/Release/AddObject/
    /// ReleaseObject call sites. Wrapping matches x86 INC/DEC; native widths
    /// are 32-bit signed (wsprintf %d). Not native exports.
    pub fn add_ref(&self) {
        self.ref_count.fetch_add(1, Ordering::Relaxed);
    }
    pub fn release_ref(&self) {
        self.ref_count.fetch_sub(1, Ordering::Relaxed);
    }
    pub fn add_object(&self) {
        self.obj_count.fetch_add(1, Ordering::Relaxed);
    }
    pub fn release_object(&self) {
        self.obj_count.fetch_sub(1, Ordering::Relaxed);
    }

    /// PC RVA 0x4160 report decision + payload, without the dialog effect.
    /// Reads live counts when the original does (after the flag check).
    /// Returns None when the binary shows no dialog (flag clear, both zero,
    /// or empty body). Some((caption, body)) is byte-exact: buffer[0]='\0'
    /// then wsprintf+strcat per nonzero counter, ref line before obj line,
    /// capped at 511 payload bytes + terminator domain (native would overflow
    /// past 512 only for absurd counts; we truncate the body, never the caption).
    pub fn pending_dialog(&self) -> Option<(Vec<u8>, Vec<u8>)> {
        if !self.dump_memory_leaks.load(Ordering::Relaxed) {
            return None;
        }
        let refs = self.ref_count.load(Ordering::Relaxed);
        let objs = self.obj_count.load(Ordering::Relaxed);
        if refs == 0 && objs == 0 {
            return None;
        }
        let mut body: Vec<u8> = Vec::with_capacity(BUFFER_CAP);
        if refs != 0 {
            push_line(&mut body, REF_FORMAT, refs);
        }
        if objs != 0 {
            push_line(&mut body, OBJ_FORMAT, objs);
        }
        if body.is_empty() {
            return None;
        }
        Some((LEAK_CAPTION.to_vec(), body))
    }
}

fn push_line(body: &mut Vec<u8>, format: &[u8], value: i32) {
    // Renders "%d" with exactly the format's bytes (double spaces kept).
    let text = format_number(format, value);
    let room = BUFFER_CAP.saturating_sub(1).saturating_sub(body.len());
    body.extend_from_slice(&text[..text.len().min(room)]);
    let _ = LINE_CAP;
}

fn format_number(format: &[u8], value: i32) -> Vec<u8> {
    // Only %d in either format; no other verbs exist in the evidence.
    let digits = value.to_string().into_bytes();
    let mut out = Vec::with_capacity(format.len() + digits.len());
    let mut i = 0;
    while i < format.len() {
        if format[i] == b'%' && i + 1 < format.len() && format[i + 1] == b'd' {
            out.extend_from_slice(&digits);
            i += 2;
        } else {
            out.push(format[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod leak_tests {
    use super::*;

    #[test]
    fn init_zeroes_and_flag_stores_byte() {
        let d = LeakDetector::new();
        assert!(!d.dump_memory_leaks());
        assert_eq!(d.pending_dialog(), None);
        d.set_dump_memory_leaks(true);
        assert!(d.dump_memory_leaks());
        assert_eq!(d.pending_dialog(), None);
        d.set_dump_memory_leaks(false);
        d.add_ref();
        d.add_object();
        assert_eq!(d.pending_dialog(), None);
    }

    #[test]
    fn report_bytes_match_native_formats() {
        let d = LeakDetector::new();
        d.set_dump_memory_leaks(true);
        d.add_ref();
        d.add_ref();
        d.add_object();
        let (caption, body) = d.pending_dialog().unwrap();
        assert_eq!(caption, b"Dta_read error message");
        assert_eq!(
            body,
            b"Not released:  2  reference(s) of Dta_read.\nNot released:  1  Dta_read(s).\n"
                .to_vec()
        );
        let r = LeakDetector::new();
        r.set_dump_memory_leaks(true);
        r.add_ref();
        assert_eq!(
            r.pending_dialog().unwrap().1,
            b"Not released:  1  reference(s) of Dta_read.\n".to_vec()
        );
        let o = LeakDetector::new();
        o.set_dump_memory_leaks(true);
        o.add_object();
        o.add_object();
        o.add_object();
        assert_eq!(
            o.pending_dialog().unwrap().1,
            b"Not released:  3  Dta_read(s).\n".to_vec()
        );
    }
}
