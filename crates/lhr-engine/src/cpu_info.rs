#![forbid(unsafe_code)]
//! CPU model-name helper, LS3DF.dll
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6 RVA
//! 0x6adc0. `map_mname` keeps its Microsoft CPUID-sample name (the file's
//! first three functions are the sample's). Contract key is SHA+RVA, never
//! the name. Pinned mafia-re ab67b13 `ls3df/IGraph/cpuid.c:44-160`. The
//! x86-only `cpuid`/`RDTSC` probes (`IsCPUID`, `_cpuid`, `GetCPUSpeed`) are
//! withheld and claim nothing here.

fn vendor_eq(vendor: &[u8], lit: &[u8; 12]) -> bool {
    if vendor.len() < 12 {
        return false;
    }
    &vendor[..12] == lit
}

fn write_cstr(out: &mut [u8], text: &[u8]) {
    if out.is_empty() {
        return;
    }
    let n = text.len().min(out.len() - 1);
    out[..n].copy_from_slice(&text[..n]);
    out[n] = 0;
    for b in out[n + 1..].iter_mut() {
        *b = 0;
    }
}

/// LS3DF RVA 0x6adc0 `map_mname` (mafia-re ls3df/IGraph/cpuid.c:44-160).
/// `strncmp` vendor match on 12 bytes (shorter vendor buffers miss instead
/// of reading out of bounds like the native call — disclosed safe
/// hardening), then family/model switches. Unknown
/// models of known families (and Cyrix/Centaur/unknown vendors) write
/// `"Unknown"`; unknown families and AMD family-5 models 4/5 leave `m_name`
/// untouched (caller zeroed, so empty). `m_name` must hold the 30-byte
/// sample field plus NUL; writes truncate to its length (defense: native
/// `strcpy` would overflow a short buffer).
pub fn map_model_name(family: i32, model: i32, vendor: &[u8], m_name: &mut [u8]) {
    if vendor_eq(vendor, b"AuthenticAMD") {
        match family {
            4 => write_cstr(m_name, b"AMD Am486"),
            5 => match model {
                0..=3 => write_cstr(m_name, b"AMD K5"),
                4 | 5 => {}
                6 | 7 => write_cstr(m_name, b"AMD K6"),
                8 => write_cstr(m_name, b"AMD K6-2"),
                9..=15 => write_cstr(m_name, b"AMD K6-3"),
                _ => write_cstr(m_name, b"Unknown"),
            },
            6 => write_cstr(m_name, b"AMD ATHLON"),
            _ => {}
        }
    } else if vendor_eq(vendor, b"GenuineIntel") {
        match family {
            4 => match model {
                0 | 1 => write_cstr(m_name, b"INTEL 486DX"),
                2 => write_cstr(m_name, b"INTEL 486SX"),
                3 => write_cstr(m_name, b"INTEL 486DX2"),
                4 => write_cstr(m_name, b"INTEL 486SL"),
                5 => write_cstr(m_name, b"INTEL 486SX2"),
                7 => write_cstr(m_name, b"INTEL 486DX2E"),
                8 => write_cstr(m_name, b"INTEL 486DX4"),
                _ => write_cstr(m_name, b"Unknown"),
            },
            5 => match model {
                1..=3 => write_cstr(m_name, b"INTEL Pentium"),
                4 => write_cstr(m_name, b"INTEL Pentium-MMX"),
                _ => write_cstr(m_name, b"Unknown"),
            },
            6 => match model {
                1 => write_cstr(m_name, b"INTEL Pentium-Pro"),
                3 => write_cstr(m_name, b"INTEL Pentium-II"),
                5 => write_cstr(m_name, b"INTEL Pentium-II"),
                6 => write_cstr(m_name, b"INTEL Celeron"),
                7 => write_cstr(m_name, b"INTEL Pentium-III"),
                _ => write_cstr(m_name, b"Unknown"),
            },
            _ => {}
        }
    } else {
        // Sample maps CyrixInstead, CentaurHauls, and any other vendor to "Unknown".
        write_cstr(m_name, b"Unknown");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cstr_str(buf: &[u8]) -> &[u8] {
        let n = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        &buf[..n]
    }
    fn run(family: i32, model: i32, vendor: &[u8]) -> Vec<u8> {
        let mut out = [0u8; 30];
        map_model_name(family, model, vendor, &mut out);
        cstr_str(&out).to_vec()
    }
    #[test]
    fn amd_families_match_sample_table() {
        assert_eq!(run(4, 0, b"AuthenticAMD"), b"AMD Am486".to_vec());
        assert_eq!(run(5, 2, b"AuthenticAMD"), b"AMD K5".to_vec());
        assert_eq!(run(5, 4, b"AuthenticAMD"), b"".to_vec());
        assert_eq!(run(5, 6, b"AuthenticAMD"), b"AMD K6".to_vec());
        assert_eq!(run(5, 8, b"AuthenticAMD"), b"AMD K6-2".to_vec());
        assert_eq!(run(5, 12, b"AuthenticAMD"), b"AMD K6-3".to_vec());
        assert_eq!(run(5, 99, b"AuthenticAMD"), b"Unknown".to_vec());
        assert_eq!(run(6, 0, b"AuthenticAMD"), b"AMD ATHLON".to_vec());
        assert_eq!(run(7, 0, b"AuthenticAMD"), b"".to_vec());
    }
    #[test]
    fn intel_families_match_sample_table() {
        assert_eq!(run(4, 1, b"GenuineIntel"), b"INTEL 486DX".to_vec());
        assert_eq!(run(4, 2, b"GenuineIntel"), b"INTEL 486SX".to_vec());
        assert_eq!(run(4, 8, b"GenuineIntel"), b"INTEL 486DX4".to_vec());
        assert_eq!(run(4, 9, b"GenuineIntel"), b"Unknown".to_vec());
        assert_eq!(run(5, 2, b"GenuineIntel"), b"INTEL Pentium".to_vec());
        assert_eq!(run(5, 4, b"GenuineIntel"), b"INTEL Pentium-MMX".to_vec());
        assert_eq!(run(6, 1, b"GenuineIntel"), b"INTEL Pentium-Pro".to_vec());
        assert_eq!(run(6, 6, b"GenuineIntel"), b"INTEL Celeron".to_vec());
        assert_eq!(run(6, 9, b"GenuineIntel"), b"Unknown".to_vec());
        assert_eq!(run(9, 0, b"GenuineIntel"), b"".to_vec());
    }
    #[test]
    fn other_vendors_are_unknown_and_short_vendor_misses() {
        assert_eq!(run(6, 0, b"CyrixInstead"), b"Unknown".to_vec());
        assert_eq!(run(6, 0, b"CentaurHauls"), b"Unknown".to_vec());
        assert_eq!(run(6, 0, b"SomethingElse!"), b"Unknown".to_vec());
        assert_eq!(run(6, 0, b"AMD"), b"Unknown".to_vec());
    }
}
