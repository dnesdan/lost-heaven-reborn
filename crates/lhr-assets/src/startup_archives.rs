//! Original InitSystem archive registration order and UnlockPack arguments.
//! Keys are passed before UnlockPack applies its fixed XOR.

pub const ARCHIVE_NAMES: [&[u8]; 14] = [
    b"a8.dta", b"b8.dta", b"a2.dta", b"a6.dta", b"a0.dta", b"a1.dta", b"a3.dta", b"ac.dta",
    b"a4.dta", b"aa.dta", b"a5.dta", b"a7.dta", b"a9.dta", b"ab.dta",
];
pub const ARCHIVE_UNLOCK_KEYS: [u64; 14] = [
    0xec45d8ce6a63fa71,
    0xb60f823f79e21cdb,
    0xb6399e191417d340,
    0x5055da68728e2db9,
    0xec48fe177f3d9b74,
    0x0900210ee7375f59,
    0x771f3888a94b8d3c,
    0x771f3888a94b8d3c,
    0x771f3888a94b8d3c,
    0x67da216ed4ad90c6,
    0xea3404204f4bb0c6,
    0xe266fe62f4f03a72,
    0x5b763446959d1117,
    0xec48fe177f3d9b74,
];
