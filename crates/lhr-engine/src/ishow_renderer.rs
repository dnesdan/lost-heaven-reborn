//! Texture-renderer drop for IShow AVI playback.
//!
//! Behavioral contract for LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6
//! per sibling mafia-re @ 113b5b6 (ls3df/IGraph/IShow.cpp:46-81,1225-1231,
//! ls3df/IGraph/IGraph.h:536-560). Tier contract: reviewed upstream source
//! plus pinning tests. No Ghidra body was re-observed here; the shape follows
//! the assigned REA note (null-guarded Release plus vtable/base dispatch).
//!
//! The CTextureRenderer name is the DirectX SDK sample name reused by the
//! original (IShow.cpp:46-53), not an invented label. The sound-renderer
//! member m_pSoundRenderer is a working-guess name per source (IShow.cpp:78);
//! only its optional identity crosses this boundary.
//!
//! Scope: CPU guard plus submit ordering only. DirectShow sound-renderer
//! Release, the five vtable restores and the CBaseVideoRenderer base-dtor
//! dispatch stay behind TextureRendererDtorBackend; Bink, Ogg, texture and
//! device submits are withheld with no functions.
//!
//! Ownership: typed state, no byte maps. Taking the option clears it like the
//! native null store after Release; the caller drops the renderer owner
//! exactly once after this call (native deleting-dtor frees it).
//!
//! Withheld: scalar deleting-dtor at LS3DF RVA 0x753e0 (SYNTHETIC per
//! IGraph.h:536-540) is construction-covered by this drop: it only branches
//! to this body then frees, so it receives no separate entry.

/// Texture-renderer CPU projection (IShow.cpp:65-81, working-guess member per
/// source). Base-class vtable words and COM object identities stay behind the
/// backend; only the optional sound-renderer handle is stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextureRenderer {
    pub sound_renderer: Option<u32>,
}

/// Device side of the renderer drop: DirectShow Release, vtable restores and
/// base dispatch. Slot words are backend-owned indices 0..5.
pub trait TextureRendererDtorBackend {
    fn release_sound_renderer(&mut self, renderer: u32);
    fn restore_vtable(&mut self, slot: usize);
    fn dispatch_base_dtor(&mut self);
}

/// LS3DF RVA 0x75400, VA 0x10075400. CTextureRenderer::~CTextureRenderer
/// (mafia-re ls3df/IGraph/IShow.cpp:1227): null-guarded sound-renderer
/// Release, then vtable restores, then base-dtor dispatch. The five-slot
/// restore count follows the probe's assigned REA note (not re-observed
/// here; working guess pending REA). The deleting
/// dtor at 0x753e0 is construction-covered by this body and claims no entry.
///
/// Number of vtable restore slots in the teardown order. Working guess per
/// the assigned REA note (pending re-observation); the release-then-restores-
/// then-base order is the evidenced shape, the exact arity is not.
pub const RENDERER_RESTORE_SLOTS: usize = 5;

fn restore_slots(backend: &mut impl TextureRendererDtorBackend) {
    for slot in 0..RENDERER_RESTORE_SLOTS {
        backend.restore_vtable(slot);
    }
}

pub fn ctexture_renderer_drop(
    renderer: &mut TextureRenderer,
    backend: &mut impl TextureRendererDtorBackend,
) {
    if let Some(handle) = renderer.sound_renderer.take() {
        backend.release_sound_renderer(handle);
    }
    restore_slots(backend);
    backend.dispatch_base_dtor();
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        released: Vec<u32>,
        restored: Vec<usize>,
        dispatched: usize,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                released: Vec::new(),
                restored: Vec::new(),
                dispatched: 0,
            }
        }
    }

    impl TextureRendererDtorBackend for Fake {
        fn release_sound_renderer(&mut self, renderer: u32) {
            self.released.push(renderer);
        }
        fn restore_vtable(&mut self, slot: usize) {
            self.restored.push(slot);
        }
        fn dispatch_base_dtor(&mut self) {
            self.dispatched += 1;
        }
    }

    #[test]
    fn drop_releases_guarded_renderer_and_restores_five() {
        let mut backend = Fake::new();
        let mut renderer = TextureRenderer {
            sound_renderer: Some(9),
        };
        ctexture_renderer_drop(&mut renderer, &mut backend);
        assert_eq!(renderer.sound_renderer, None);
        assert_eq!(backend.released, vec![9]);
        // Order pinned, arity not (working-guess count, see above):
        // restores are a non-empty 0-based consecutive prefix.
        assert!(!backend.restored.is_empty());
        assert_eq!(backend.restored[0], 0);
        assert!(backend.restored.windows(2).all(|w| w[1] == w[0] + 1));
        assert_eq!(backend.dispatched, 1);
    }

    #[test]
    fn drop_without_renderer_still_restores_and_dispatches() {
        let mut backend = Fake::new();
        let mut renderer = TextureRenderer {
            sound_renderer: None,
        };
        ctexture_renderer_drop(&mut renderer, &mut backend);
        assert!(backend.released.is_empty());
        assert!(!backend.restored.is_empty());
        assert_eq!(backend.restored[0], 0);
        assert!(backend.restored.windows(2).all(|w| w[1] == w[0] + 1));
        assert_eq!(backend.dispatched, 1);
    }
}

// ---------------------------------------------------------------------------
// C_game video-list marker-only contracts (Game.exe, NOT LS3DF).
//
// Hosted in this file because lib.rs is frozen for this change, so no new
// module could be added; this file already owns the TextureRenderer CPU
// projection the list entries point at. Every Game.exe entry below is tier
// contract: reviewed mafia-re source at pin 113b5b6 plus pinning tests. No
// Game.exe body was re-observed here.
//
// Provenance: mafia-re game/mission/game.h (S_i_show, S_i_show_fade,
// ISHOWF_LOOP) and game/mission/game.cpp (CreateIShow, OpenIShow, GetIShow,
// IShowStoreVolume, IShowLoop, IShowSetVolume, TickCityMusic, CityMusicOff,
// CityMusicPaused, LoadCityMusicAreas, cbCityMusic, Tick fades).
// IShow::Open/Play/Close/IsInit, IShowStream decoding, DirectShow/Bink
// graphs and device playback stay behind CGameVideolistBackend (DEFERRED, no
// host claims).
//
// Ownership: typed state, no byte maps. Show tokens are backend-owned opaque
// identities (u32), never host pointers. Names are fixed file records. List
// order is the native vector order and every walk runs back-to-front like the
// native `for (i = size; i--)` loops.
//
// Withheld (marker-only contracts in the STUB section below, no bodies): single
// DestroyIShow device sequence (GAME 0x5b62c0, cgame_destroy_ishow),
// IShowFade/IShowFadeDownAll creation (GAME 0x5b6710 / 0x5b6660,
// cgame_ishow_fade / cgame_ishow_fade_down_all), film-music stream device ops
// (GAME 0x5ae1d0 / 0x5ae390 / 0x5ae3d0, cgame_set_film_music /
// cgame_get_film_music / cgame_stream_destroy_all), the Tick
// loop-restart slice (game.cpp:2140-2151, needs device
// GetEvent/SetCurrentPos/Play), and all IShow::Open/Play/Stop/Close/IsInit
// plus IShowStream decode/submit paths.

/// Loop flag (mafia-re game/mission/game.h:188, PS2's IShowLoop's, name ours):
/// Tick restarts the stream at its end while set.
pub const ISHOWF_LOOP: u32 = 0x1;

/// File-not-found word (mafia-re ls3df/ls3df.h:22): OpenIShow retries an .ogg
/// miss as .mp3 only on exactly this code.
pub const I3DERR_NOFILE: i32 = 0x8000_0001u32 as i32;

/// City-music recheck period in ms (mafia-re game/mission/game.cpp:1731).
pub const CITY_MUSIC_RECHECK_MS: i32 = 10_000;

/// City-music fade length in ms (mafia-re game/mission/game.cpp:1780,1789,3026).
pub const CITY_MUSIC_FADE_MS: i32 = 5000;

/// Area-name prefix filter (mafia-re game/mission/game.cpp:3007).
pub const CITY_MUSIC_AREA_PREFIX: &[u8] = b"city_music_";

/// One video playing in the mission (mafia-re game/mission/game.h:189-196,
/// members ours). `show` is the backend device token; only the id, flags,
/// name and own volume live on the host.
#[derive(Clone, Debug, PartialEq)]
pub struct CGameIshowEntry {
    pub show: u32,
    pub id: i32,
    pub flags: u32,
    pub name: Option<Vec<u8>>,
    pub volume: f32,
}

/// One volume fade of a video (mafia-re game/mission/game.h:198-205, members
/// ours). Times are native signed shorts in ms; Tick compares and converts
/// them so (game.h:202).
#[derive(Clone, Debug, PartialEq)]
pub struct CGameIshowFade {
    pub id: i32,
    pub from: f32,
    pub to: f32,
    pub time: i16,
    pub length: i16,
}

/// Host-owned C_game video state: the show list, the fade list, the id source,
/// the city-music fields and the film-music stream tokens (mafia-re
/// game/mission/game.h:499, `std::vector<uint>`). Mirrors the C_game::Init
/// stores that are in scope (mafia-re game/mission/game.cpp:309-311 for the
/// lists and the id source, :319 for the film clear, :618-623 for the
/// city-music fields). Film stream device ops and the scream/race/weather
/// owners are out of scope and stay deferred; only the token-list clear is
/// ported.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CGameVideolist {
    pub shows: Vec<CGameIshowEntry>,
    pub fades: Vec<CGameIshowFade>,
    pub next_id: i32,
    pub city_music_time: i32,
    pub city_music_area: Option<u32>,
    pub city_music: i32,
    pub city_music_old: i32,
    pub city_music_paused: bool,
    pub city_music_areas: Vec<u32>,
    pub film_music_streams: Vec<u32>,
}

impl CGameVideolist {
    /// Fresh video state matching C_game::Init (mafia-re
    /// game/mission/game.cpp:309-311,618-623): empty lists, id source zero,
    /// recheck due, no areas or music, unpaused.
    pub fn fresh() -> Self {
        Self {
            city_music_time: -1,
            city_music: -1,
            city_music_old: -1,
            ..Default::default()
        }
    }
}

/// Deferred device/world boundary. Every method is an original device, graph,
/// sound-driver or scene effect whose body is NOT ported here: creation,
/// file opens, volume submits, event/loop/transport calls, teardown, fades,
/// the active camera, area coverage and names, and frame enumeration. The
/// host only implements the CPU list, name, flag, volume and order logic.
pub trait CGameVideolistBackend {
    fn create_ishow(&mut self) -> Option<u32>;
    fn open_ishow(&mut self, show: u32, path: &[u8]) -> i32;
    fn set_ishow_volume(&mut self, show: u32, volume: f32);
    fn ishow_event(&mut self, show: u32) -> i32;
    fn ishow_get_volume(&mut self, show: u32) -> f32;
    fn ishow_play(&mut self, show: u32);
    fn ishow_stop(&mut self, show: u32);
    fn destroy_ishow_device(&mut self, id: i32, show: u32);
    fn ishow_fade(&mut self, id: i32, from: f32, to: f32, time_ms: i32);
    fn active_camera_pos(&mut self) -> Option<[f32; 3]>;
    fn area_contains_point(&mut self, area: u32, pos: [f32; 3]) -> bool;
    fn area_name(&mut self, area: u32) -> Option<Vec<u8>>;
    fn enum_frames(&mut self, visit: &mut dyn FnMut(u32, bool, &[u8]));
}

/// Reverse back-to-front id search shared by the list walks.
fn find_show(shows: &[CGameIshowEntry], id: i32) -> Option<usize> {
    (0..shows.len()).rev().find(|&i| shows[i].id == id)
}

/// Host list removal for a destroyed entry. The device Close/Release sequence
/// (GAME 0x5b62c0, mafia-re stub) stays behind destroy_ishow_device; the row
/// removal mirrors the evidenced per-row teardown of DestroyIShowAll
/// (mafia-re game/mission/game.cpp:3515-3528). Order-preserving `remove` is a
/// host choice, disclosed: the single-row native order is stub-covered.
fn destroy_entry(
    list: &mut CGameVideolist,
    backend: &mut impl CGameVideolistBackend,
    id: i32,
) {
    if let Some(pos) = find_show(&list.shows, id) {
        let entry = list.shows.remove(pos);
        backend.destroy_ishow_device(entry.id, entry.show);
    }
}

/// GAME 0x5b63b0. C_game::DestroyIShowAll (mafia-re game/mission/game.cpp:
/// 3515-3529): every video from the last one is closed and released through
/// the single DestroyIShow device sequence (GAME 0x5b62c0, behind
/// destroy_ishow_device), then the show, fade and film-music-stream lists are
/// emptied. Name storage is host-owned and drops with the rows; a null native
/// show pointer has no host form (created rows always hold a backend token),
/// so every row submits. Film streams clear with no device call like native.
pub fn cgame_destroy_ishow_all(
    list: &mut CGameVideolist,
    backend: &mut impl CGameVideolistBackend,
) {
    for i in (0..list.shows.len()).rev() {
        backend.destroy_ishow_device(list.shows[i].id, list.shows[i].show);
    }
    list.shows.clear();
    list.fades.clear();
    list.film_music_streams.clear();
}

/// GAME 0x5b5e90. C_game::CreateIShow (mafia-re game/mission/game.cpp:3446-
/// 3463): without sound returns -1 and consumes no id; else the next id is
/// assigned, the backend creates the show, and only a created show is pushed
/// and scaled by the music volume. A null backend show consumes the id and
/// returns -1. `disable_sound` and `music_volume` are caller-owned live
/// settings (no globals on the host).
pub fn cgame_create_ishow(
    list: &mut CGameVideolist,
    backend: &mut impl CGameVideolistBackend,
    flags: u32,
    disable_sound: bool,
    music_volume: f32,
) -> i32 {
    if disable_sound {
        return -1;
    }
    let id = list.next_id;
    list.next_id = list.next_id.wrapping_add(1);
    if let Some(show) = backend.create_ishow() {
        list.shows.push(CGameIshowEntry {
            show,
            id,
            flags,
            name: None,
            volume: 1.0,
        });
        backend.set_ishow_volume(show, music_volume * 1.0);
        return id;
    }
    -1
}

/// Case-insensitive byte compare for the duplicate-name scan (native
/// `_stricmp`, mafia-re game/mission/game.cpp:3475).
fn names_equal_fold(left: &[u8], right: &[u8]) -> bool {
    left.eq_ignore_ascii_case(right)
}

/// GAME 0x5b6110. C_game::OpenIShow (mafia-re game/mission/game.cpp:3468-
/// 3505; STRING GAME 0x6377b8): null name is false; the first same-file other
/// id from the end is destroyed; the id row keeps the name copy and opens it;
/// an I3DERR_NOFILE miss on a name containing lowercase ".ogg" retries once
/// with the first ".ogg" replaced by ".mp3" (native in-place `ext[1..3]`
/// overwrite). Any other result, or a missing id row, keeps the stored name
/// and returns true/false respectively.
pub fn cgame_open_ishow(
    list: &mut CGameVideolist,
    backend: &mut impl CGameVideolistBackend,
    name: Option<&[u8]>,
    id: i32,
) -> bool {
    let Some(name) = name else {
        return false;
    };
    for i in (0..list.shows.len()).rev() {
        let same = list.shows[i].id != id
            && list.shows[i]
                .name
                .as_deref()
                .is_some_and(|own| names_equal_fold(own, name));
        if same {
            let doomed = list.shows[i].id;
            destroy_entry(list, backend, doomed);
            break;
        }
    }
    let Some(pos) = find_show(&list.shows, id) else {
        return false;
    };
    list.shows[pos].name = Some(name.to_vec());
    let show = list.shows[pos].show;
    if backend.open_ishow(show, name) == I3DERR_NOFILE && contains_ogg(name) {
        let mp3 = ogg_to_mp3(name);
        backend.open_ishow(show, &mp3);
    }
    true
}

/// Lowercase ".ogg" marker scan (native `strstr(name, ".ogg")`, case
/// sensitive; ".OGG" alone never falls back).
fn contains_ogg(name: &[u8]) -> bool {
    name.windows(4).any(|w| w == b".ogg")
}

/// First ".ogg" becomes ".mp3" in place (native `ext[1] = 'm'` etc. on the
/// first `strstr` hit; the tail after the hit is preserved).
fn ogg_to_mp3(name: &[u8]) -> Vec<u8> {
    let mut out = name.to_vec();
    if let Some(at) = out.windows(4).position(|w| w == b".ogg") {
        out[at + 1..at + 4].copy_from_slice(b"mp3");
    }
    out
}

/// GAME 0x5b64c0. C_game::GetIShow (mafia-re game/mission/game.cpp:3533-
/// 3541): the stream of the id from the last one, None for none.
pub fn cgame_get_ishow(list: &CGameVideolist, id: i32) -> Option<u32> {
    find_show(&list.shows, id).map(|pos| list.shows[pos].show)
}

/// GAME 0x5b6520. C_game::IShowStoreVolume (mafia-re game/mission/game.cpp:
/// 3545-3555): the stream's own volume from the last one; unknown ids do
/// nothing. This is the entry Tick's fades write through.
pub fn cgame_ishow_store_volume(list: &mut CGameVideolist, id: i32, volume: f32) {
    if let Some(pos) = find_show(&list.shows, id) {
        list.shows[pos].volume = volume;
    }
}

/// GAME 0x5b6580. C_game::IShowLoop (mafia-re game/mission/game.cpp:3557-
/// 3572): sets or clears ISHOWF_LOOP on the id row from the last one. The
/// PS2 stream notification has no PC IShow counterpart, so only the C_game
/// flag flips here; device looping stays deferred.
pub fn cgame_ishow_loop(list: &mut CGameVideolist, id: i32, looping: bool) {
    if let Some(pos) = find_show(&list.shows, id) {
        if looping {
            list.shows[pos].flags |= ISHOWF_LOOP;
        } else {
            list.shows[pos].flags &= !ISHOWF_LOOP;
        }
    }
}

/// GAME 0x5b6600. C_game::IShowSetVolume (mafia-re game/mission/game.cpp:
/// 3576-3580): from the last video to the first, each device volume is the
/// music volume scaled by the stream's own volume. The music volume is a
/// single caller snapshot per call; the device submit cannot mutate game
/// settings, so no mid-loop reread is needed.
pub fn cgame_ishow_set_volume(
    list: &CGameVideolist,
    backend: &mut impl CGameVideolistBackend,
    music_volume: f32,
) {
    for i in (0..list.shows.len()).rev() {
        backend.set_ishow_volume(list.shows[i].show, music_volume * list.shows[i].volume);
    }
}

/// GAME 0x5adf30. C_game::S_hlp::cbCityMusic (mafia-re game/mission/game.cpp:
/// 3001-3011; STRING GAME 0x6377a4): AREA frames whose name is longer than 12
/// with a case-insensitive `city_music_` prefix. `is_area` carries the native
/// `GetType() == FRAME_AREA` check; the byte length stands in for `strlen`
/// (enumeration names carry no NUL on the host).
pub fn cgame_cb_city_music(is_area: bool, name: &[u8]) -> bool {
    is_area && name.len() > 12 && name[..CITY_MUSIC_AREA_PREFIX.len()].eq_ignore_ascii_case(CITY_MUSIC_AREA_PREFIX)
}

/// GAME 0x5adee0. C_game::LoadCityMusicAreas (mafia-re game/mission/game.cpp:
/// 2992-2997): clears the areas, then the backend enumerates frames and the
/// host keeps the cbCityMusic matches. Matches C_game::Init
/// (game.cpp:618), which loads areas before clearing the current music to
/// null/-1/-1/unpaused.
pub fn cgame_load_city_music_areas(
    list: &mut CGameVideolist,
    backend: &mut impl CGameVideolistBackend,
) {
    list.city_music_areas.clear();
    backend.enum_frames(&mut |id, is_area, name| {
        if cgame_cb_city_music(is_area, name) {
            list.city_music_areas.push(id);
        }
    });
}

/// GAME inline. C_game::TickCityMusic (mafia-re game/mission/game.cpp:1727-
/// 1797; STRING GAME 0x637748): while the recheck is due, once no other video
/// is still playing, the first containing area from the end wins; its area is
/// stored, then an unchanged file name returns; otherwise the old music is
/// destroyed and faded out and the new `sounds\music\city_music_??.ogg`
/// stream is created, opened, played, looped and faded in. The two-letter
/// music is the area name suffix after the prefix. A failed creation still
/// flows through Open/Get, which then find no row, like the native -1 path.
/// The paused gate (`!city_music_paused`, game.cpp:2166-2167) lives with the
/// Tick caller.
pub fn cgame_tick_city_music(
    list: &mut CGameVideolist,
    backend: &mut impl CGameVideolistBackend,
    time_ms: u32,
    music_volume: f32,
    disable_sound: bool,
) {
    if list.city_music_time >= 0 {
        list.city_music_time = list
            .city_music_time
            .wrapping_sub(time_ms as i32);
        return;
    }
    list.city_music_time = CITY_MUSIC_RECHECK_MS;
    if list.city_music_areas.is_empty() {
        return;
    }
    for i in (0..list.shows.len()).rev() {
        if list.shows[i].id != list.city_music && list.shows[i].id != list.city_music_old {
            let show = list.shows[i].show;
            let _ = backend.ishow_event(show);
            if backend.ishow_event(show) == 0 {
                return;
            }
        }
    }
    let Some(pos) = backend.active_camera_pos() else {
        return;
    };
    let mut hit: Option<u32> = None;
    for i in (0..list.city_music_areas.len()).rev() {
        let area = list.city_music_areas[i];
        if backend.area_contains_point(area, pos) {
            hit = Some(area);
            break;
        }
    }
    let Some(area) = hit else {
        return;
    };
    if Some(area) == list.city_music_area {
        return;
    }
    let name = backend.area_name(area).unwrap_or_default();
    if name.len() <= 12 {
        return;
    }
    list.city_music_area = Some(area);
    let mut file = Vec::with_capacity(24 + 2 + 4);
    file.extend_from_slice(b"sounds\\music\\city_music_");
    file.push(name[CITY_MUSIC_AREA_PREFIX.len()]);
    file.push(name[CITY_MUSIC_AREA_PREFIX.len() + 1]);
    file.extend_from_slice(b".ogg");
    let mut same = false;
    for j in (0..list.shows.len()).rev() {
        if list.shows[j].id == list.city_music {
            same = list.shows[j].name.as_deref() == Some(file.as_slice());
            break;
        }
    }
    if same {
        return;
    }
    if list.city_music_old > -1 {
        let doomed = list.city_music_old;
        destroy_entry(list, backend, doomed);
    }
    let retired = list.city_music;
    list.city_music_old = retired;
    if cgame_get_ishow(list, retired).is_some() {
        cgame_ishow_loop(list, retired, false);
        backend.ishow_fade(retired, 0.5, 0.0, CITY_MUSIC_FADE_MS);
    }
    let fresh = cgame_create_ishow(list, backend, 2, disable_sound, music_volume);
    list.city_music = fresh;
    cgame_open_ishow(list, backend, Some(&file), fresh);
    if let Some(show) = cgame_get_ishow(list, fresh) {
        backend.ishow_play(show);
        cgame_ishow_loop(list, fresh, true);
        backend.ishow_fade(fresh, 0.0, 0.5, CITY_MUSIC_FADE_MS);
    }
}

/// GAME 0x5ae130. C_game::CityMusicOff (mafia-re game/mission/game.cpp:3016-
/// 3029): forgets the area, destroys the fading-out music, fades the playing
/// music out from its device volume over 5 s without looping, and clears the
/// id. The fade creation itself stays behind ishow_fade (GAME 0x5b6710 stub).
pub fn cgame_city_music_off(
    list: &mut CGameVideolist,
    backend: &mut impl CGameVideolistBackend,
) {
    list.city_music_area = None;
    if list.city_music_old > -1 {
        let doomed = list.city_music_old;
        destroy_entry(list, backend, doomed);
    }
    let retired = list.city_music;
    list.city_music_old = retired;
    if let Some(show) = cgame_get_ishow(list, retired) {
        cgame_ishow_loop(list, retired, false);
        let from = backend.ishow_get_volume(show);
        backend.ishow_fade(retired, from, 0.0, CITY_MUSIC_FADE_MS);
    }
    list.city_music = -1;
}

/// GAME 0x5ae1b0. C_game::CityMusicPaused (mafia-re game/mission/game.cpp:
/// 3032-3035): the native body is a stub, so this single flag store is a
/// working guess pending REA. It mirrors the `city_music_paused` member
/// (game.h:496), its Init clear (game.cpp:623) and the Tick gate that skips
/// TickCityMusic while set (game.cpp:2166-2167).
pub fn cgame_city_music_paused(list: &mut CGameVideolist, paused: bool) {
    list.city_music_paused = paused;
}

/// GAME Tick slice. The videos' fades (mafia-re game/mission/game.cpp:2098-
/// 2138): each fade ages by the tick; a finished fade either stops a faded-out
/// video (destroying the fading-out city music) or stores and submits the
/// target volume; a running fade submits the interpolated volume and stores
/// it. Finished rows are swap-removed (`fades[m] = back; pop_back`) and only
/// running rows advance the index. Missing id rows still complete (removed)
/// but submit nothing. Fade creation (IShowFade, GAME 0x5b6710 stub) stays
/// deferred: callers stage rows directly. The looping-video restart slice
/// (game.cpp:2140-2151) needs device transport and stays deferred.
pub fn cgame_tick_ishow_fades(
    list: &mut CGameVideolist,
    backend: &mut impl CGameVideolistBackend,
    time_ms: u32,
    music_volume: f32,
) {
    let mut m = 0usize;
    while m < list.fades.len() {
        let (id, from, to, time, length) = {
            let fade = &list.fades[m];
            (fade.id, fade.from, fade.to, fade.time, fade.length)
        };
        let aged = time.wrapping_add(time_ms as i16);
        list.fades[m].time = aged;
        if aged >= length {
            if let Some(show) = cgame_get_ishow(list, id) {
                if to < 0.005 && to < from {
                    backend.ishow_stop(show);
                    if id == list.city_music_old {
                        destroy_entry(list, backend, id);
                        list.city_music_old = -1;
                    }
                } else {
                    cgame_ishow_store_volume(list, id, to);
                    backend.set_ishow_volume(show, music_volume * to);
                }
            }
            list.fades.swap_remove(m);
        } else {
            if let Some(show) = cgame_get_ishow(list, id) {
                // Native interpolates with the already-aged row time.
                let vol = (aged as f32) / (length as f32) * (to - from) + from;
                backend.set_ishow_volume(show, music_volume * vol);
                cgame_ishow_store_volume(list, id, vol);
            }
            m += 1;
        }
    }
}

#[cfg(test)]
mod cgame_videolist_tests {
    use super::*;
    use std::collections::HashMap;

    struct Fake {
        next_token: u32,
        create_null: bool,
        next_open: Option<i32>,
        opened: Vec<Vec<u8>>,
        volumes: Vec<(u32, f32)>,
        get_volumes: HashMap<u32, f32>,
        event_calls: Vec<u32>,
        event_queue: Vec<i32>,
        plays: Vec<u32>,
        stops: Vec<u32>,
        destroyed: Vec<(i32, u32)>,
        fades: Vec<(i32, f32, f32, i32)>,
        camera: Option<[f32; 3]>,
        frames: Vec<(u32, bool, Vec<u8>)>,
        contains: HashMap<u32, bool>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                next_token: 11,
                create_null: false,
                next_open: None,
                opened: Vec::new(),
                volumes: Vec::new(),
                get_volumes: HashMap::new(),
                event_calls: Vec::new(),
                event_queue: Vec::new(),
                plays: Vec::new(),
                stops: Vec::new(),
                destroyed: Vec::new(),
                fades: Vec::new(),
                camera: None,
                frames: Vec::new(),
                contains: HashMap::new(),
            }
        }
    }

    impl CGameVideolistBackend for Fake {
        fn create_ishow(&mut self) -> Option<u32> {
            if self.create_null {
                return None;
            }
            let token = self.next_token;
            self.next_token += 1;
            Some(token)
        }
        fn open_ishow(&mut self, _show: u32, path: &[u8]) -> i32 {
            self.opened.push(path.to_vec());
            if let Some(code) = self.next_open.take() {
                return code;
            }
            if path.ends_with(b".ogg") {
                I3DERR_NOFILE
            } else {
                0
            }
        }
        fn set_ishow_volume(&mut self, show: u32, volume: f32) {
            self.volumes.push((show, volume));
        }
        fn ishow_event(&mut self, show: u32) -> i32 {
            self.event_calls.push(show);
            if self.event_queue.is_empty() {
                1
            } else {
                self.event_queue.remove(0)
            }
        }
        fn ishow_get_volume(&mut self, show: u32) -> f32 {
            self.get_volumes.get(&show).copied().unwrap_or(0.0)
        }
        fn ishow_play(&mut self, show: u32) {
            self.plays.push(show);
        }
        fn ishow_stop(&mut self, show: u32) {
            self.stops.push(show);
        }
        fn destroy_ishow_device(&mut self, id: i32, show: u32) {
            self.destroyed.push((id, show));
        }
        fn ishow_fade(&mut self, id: i32, from: f32, to: f32, time_ms: i32) {
            self.fades.push((id, from, to, time_ms));
        }
        fn active_camera_pos(&mut self) -> Option<[f32; 3]> {
            self.camera
        }
        fn area_contains_point(&mut self, area: u32, _pos: [f32; 3]) -> bool {
            self.contains.get(&area).copied().unwrap_or(false)
        }
        fn area_name(&mut self, area: u32) -> Option<Vec<u8>> {
            self.frames
                .iter()
                .find(|(id, _, _)| *id == area)
                .map(|(_, _, name)| name.clone())
        }
        fn enum_frames(&mut self, visit: &mut dyn FnMut(u32, bool, &[u8])) {
            for (id, is_area, name) in self.frames.clone() {
                visit(id, is_area, &name);
            }
        }
    }

    #[test]
    fn create_guards_sound_and_consumes_ids_only_past_the_gate() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        assert_eq!(cgame_create_ishow(&mut list, &mut backend, 2, true, 0.7), -1);
        assert_eq!(list.next_id, 0);
        assert!(list.shows.is_empty());
        assert!(backend.volumes.is_empty());
        assert_eq!(cgame_create_ishow(&mut list, &mut backend, 2, false, 0.7), 0);
        assert_eq!(list.next_id, 1);
        assert_eq!(list.shows[0].show, 11);
        assert_eq!(list.shows[0].volume.to_bits(), 1.0f32.to_bits());
        assert_eq!(backend.volumes.len(), 1);
        assert_eq!(backend.volumes[0].0, 11);
        assert_eq!(backend.volumes[0].1.to_bits(), 0.7f32.to_bits());
        backend.create_null = true;
        assert_eq!(cgame_create_ishow(&mut list, &mut backend, 0, false, 0.7), -1);
        assert_eq!(list.next_id, 2);
        assert_eq!(list.shows.len(), 1);
    }

    #[test]
    fn open_rejects_null_and_unknown_id_and_destroys_name_match_first() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        assert!(!cgame_open_ishow(&mut list, &mut backend, None, 0));
        assert!(!cgame_open_ishow(&mut list, &mut backend, Some(b"x.ogg"), 99));
        assert!(backend.opened.is_empty());
        let first = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        assert!(cgame_open_ishow(&mut list, &mut backend, Some(b"Song.OGG"), first));
        assert_eq!(backend.opened, vec![b"Song.OGG".to_vec()]);
        let second = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        assert!(cgame_open_ishow(&mut list, &mut backend, Some(b"song.ogg"), second));
        assert_eq!(backend.destroyed, vec![(first, 11)]);
        assert_eq!(list.shows.len(), 1);
        assert_eq!(list.shows[0].name, Some(b"song.ogg".to_vec()));
        assert_eq!(
            backend.opened,
            vec![
                b"Song.OGG".to_vec(),
                b"song.ogg".to_vec(),
                b"song.mp3".to_vec()
            ]
        );
    }

    #[test]
    fn open_fallback_replaces_first_ogg_only_on_nofile() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        let id = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        assert!(cgame_open_ishow(&mut list, &mut backend, Some(b"a.ogg.ogg"), id));
        assert_eq!(
            backend.opened,
            vec![b"a.ogg.ogg".to_vec(), b"a.mp3.ogg".to_vec()]
        );
        backend.opened.clear();
        backend.next_open = Some(0x8000_0005u32 as i32);
        assert!(cgame_open_ishow(&mut list, &mut backend, Some(b"q.ogg"), id));
        assert_eq!(backend.opened, vec![b"q.ogg".to_vec()]);
        backend.opened.clear();
        backend.next_open = Some(0);
        assert!(cgame_open_ishow(&mut list, &mut backend, Some(b"UP.OGG"), id));
        assert_eq!(backend.opened, vec![b"UP.OGG".to_vec()]);
    }

    #[test]
    fn get_store_loop_and_set_volume_walk_back_to_front() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        let a = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        let b = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        let c = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        assert_eq!(cgame_get_ishow(&list, 99), None);
        assert_eq!(cgame_get_ishow(&list, b), Some(12));
        cgame_ishow_store_volume(&mut list, 9, 0.9);
        cgame_ishow_store_volume(&mut list, a, 0.1);
        cgame_ishow_store_volume(&mut list, b, 0.2);
        cgame_ishow_store_volume(&mut list, c, 0.3);
        cgame_ishow_loop(&mut list, b, true);
        assert_eq!(list.shows[1].flags & ISHOWF_LOOP, ISHOWF_LOOP);
        cgame_ishow_loop(&mut list, 9, true);
        cgame_ishow_loop(&mut list, b, false);
        assert_eq!(list.shows[1].flags & ISHOWF_LOOP, 0);
        backend.volumes.clear();
        cgame_ishow_set_volume(&list, &mut backend, 0.5);
        assert_eq!(backend.volumes.len(), 3);
        assert_eq!(backend.volumes[0].0, 13);
        assert_eq!(backend.volumes[2].0, 11);
        assert_eq!(backend.volumes[0].1.to_bits(), (0.5f32 * 0.3f32).to_bits());
        assert_eq!(backend.volumes[1].1.to_bits(), (0.5f32 * 0.2f32).to_bits());
        assert_eq!(backend.volumes[2].1.to_bits(), (0.5f32 * 0.1f32).to_bits());
    }

    #[test]
    fn tick_city_music_counts_down_and_blocks_on_busy_video() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        list.city_music_time = 50;
        cgame_tick_city_music(&mut list, &mut backend, 20, 1.0, false);
        assert_eq!(list.city_music_time, 30);
        assert!(backend.event_calls.is_empty());
        list.city_music_time = -5;
        cgame_tick_city_music(&mut list, &mut backend, 20, 1.0, false);
        assert_eq!(list.city_music_time, CITY_MUSIC_RECHECK_MS);
        let a = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        let _ = a;
        list.city_music = 5;
        list.city_music_old = -1;
        list.city_music_time = -1;
        list.city_music_areas.push(7);
        backend.camera = Some([1.0, 2.0, 3.0]);
        backend.contains.insert(7, true);
        backend.event_queue = vec![9, 0];
        cgame_tick_city_music(&mut list, &mut backend, 0, 1.0, false);
        assert_eq!(backend.event_calls.len(), 2);
        assert_eq!(list.city_music, 5);
        assert!(backend.plays.is_empty());
    }

    #[test]
    fn tick_city_music_area_change_rotates_loop_and_fades() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        let busy = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        let _ = busy;
        list.city_music = 50;
        list.city_music_old = -1;
        list.city_music_time = -1;
        list.city_music_areas.extend([7, 9]);
        backend.camera = Some([0.0, 0.0, 0.0]);
        backend.frames = vec![
            (7, true, b"city_music_ab".to_vec()),
            (9, true, b"city_music_qq".to_vec()),
        ];
        backend.contains.insert(7, true);
        backend.contains.insert(9, false);
        backend.event_queue = vec![1, 1];
        backend.volumes.clear();
        cgame_tick_city_music(&mut list, &mut backend, 0, 0.8, false);
        assert_eq!(list.city_music_area, Some(7));
        assert_eq!(list.city_music, 1);
        assert_eq!(list.city_music_old, 50);
        assert_eq!(
            list.shows.last().unwrap().name,
            Some(b"sounds\\music\\city_music_ab.ogg".to_vec())
        );
        assert_eq!(list.shows.last().unwrap().flags & ISHOWF_LOOP, ISHOWF_LOOP);
        assert!(backend.plays.contains(&list.shows.last().unwrap().show));
        assert_eq!(backend.fades.len(), 1);
        assert_eq!(backend.fades[0].0, 1);
        assert_eq!(backend.fades[0].1.to_bits(), 0.0f32.to_bits());
        assert_eq!(backend.fades[0].2.to_bits(), 0.5f32.to_bits());
        assert_eq!(backend.fades[0].3, CITY_MUSIC_FADE_MS);
        let next = list.next_id;
        list.city_music_time = -1;
        cgame_tick_city_music(&mut list, &mut backend, 0, 0.8, false);
        assert_eq!(list.next_id, next);
        assert_eq!(list.city_music_area, Some(7));
    }

    #[test]
    fn tick_city_music_keeps_area_on_same_file_and_off_fades_current() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        let only = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        list.city_music = only;
        list.city_music_old = -1;
        list.city_music_area = Some(7);
        list.city_music_time = -1;
        list.city_music_areas.push(8);
        list.shows[0].name = Some(b"sounds\\music\\city_music_ab.ogg".to_vec());
        backend.camera = Some([0.0, 0.0, 0.0]);
        backend.frames = vec![(8, true, b"city_music_ab".to_vec())];
        backend.contains.insert(8, true);
        cgame_tick_city_music(&mut list, &mut backend, 0, 1.0, false);
        assert_eq!(list.city_music, only);
        // Native stores the area before the same-file early-out (game.cpp:1755).
        assert_eq!(list.city_music_area, Some(8));
        assert!(backend.plays.is_empty());
        backend.get_volumes.insert(list.shows[0].show, 0.4);
        cgame_city_music_off(&mut list, &mut backend);
        assert_eq!(list.city_music_area, None);
        assert_eq!(list.city_music, -1);
        assert_eq!(list.city_music_old, only);
        assert_eq!(list.shows[0].flags & ISHOWF_LOOP, 0);
        assert_eq!(backend.fades.len(), 1);
        assert_eq!(backend.fades[0].1.to_bits(), 0.4f32.to_bits());
        assert_eq!(backend.fades[0].2.to_bits(), 0.0f32.to_bits());
        assert_eq!(backend.fades[0].3, CITY_MUSIC_FADE_MS);
        cgame_city_music_paused(&mut list, true);
        assert!(list.city_music_paused);
        cgame_city_music_paused(&mut list, false);
        assert!(!list.city_music_paused);
    }

    #[test]
    fn load_city_music_areas_keeps_only_prefixed_areas() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        backend.frames = vec![
            (1, true, b"city_music_ab".to_vec()),
            (2, true, b"CITY_MUSIC_CD".to_vec()),
            (3, true, b"city_music_x".to_vec()),
            (4, true, b"short".to_vec()),
            (5, false, b"city_music_ef".to_vec()),
            (6, true, b"city_music_".to_vec()),
        ];
        cgame_load_city_music_areas(&mut list, &mut backend);
        assert_eq!(list.city_music_areas, vec![1, 2]);
        assert!(cgame_cb_city_music(true, b"city_music_ab"));
        assert!(!cgame_cb_city_music(false, b"city_music_ab"));
        assert!(!cgame_cb_city_music(true, b"city_music_x"));
    }

    #[test]
    fn tick_fades_complete_stop_and_remove_and_store_targets() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        let id = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        let show = cgame_get_ishow(&list, id).unwrap();
        list.city_music_old = id;
        list.fades.push(CGameIshowFade {
            id,
            from: 0.5,
            to: 0.0,
            time: 4000,
            length: 5000,
        });
        backend.volumes.clear();
        cgame_tick_ishow_fades(&mut list, &mut backend, 1000, 0.8);
        assert_eq!(backend.stops, vec![show]);
        assert_eq!(backend.destroyed, vec![(id, show)]);
        assert_eq!(list.city_music_old, -1);
        assert!(list.fades.is_empty());
        assert!(list.shows.is_empty());
        let id = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        let show = cgame_get_ishow(&list, id).unwrap();
        list.fades.push(CGameIshowFade {
            id,
            from: 0.2,
            to: 0.8,
            time: 4900,
            length: 5000,
        });
        backend.volumes.clear();
        cgame_tick_ishow_fades(&mut list, &mut backend, 100, 0.5);
        assert_eq!(list.shows.len(), 1);
        assert!(list.fades.is_empty());
        assert_eq!(list.shows[0].volume.to_bits(), 0.8f32.to_bits());
        assert_eq!(backend.volumes.len(), 1);
        assert_eq!(backend.volumes[0].0, show);
        assert_eq!(backend.volumes[0].1.to_bits(), (0.5f32 * 0.8f32).to_bits());
        list.fades.push(CGameIshowFade {
            id: 99,
            from: 0.0,
            to: 1.0,
            time: 4999,
            length: 5000,
        });
        backend.volumes.clear();
        cgame_tick_ishow_fades(&mut list, &mut backend, 1, 0.5);
        assert!(list.fades.is_empty());
        assert!(backend.volumes.is_empty());
    }

    #[test]
    fn tick_fades_interpolate_running_volumes_and_swap_remove_finished() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        let id = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        let show = cgame_get_ishow(&list, id).unwrap();
        list.fades.push(CGameIshowFade {
            id,
            from: 0.0,
            to: 0.5,
            time: 0,
            length: 5000,
        });
        list.fades.push(CGameIshowFade {
            id,
            from: 0.5,
            to: 0.0,
            time: 4000,
            length: 5000,
        });
        backend.volumes.clear();
        cgame_tick_ishow_fades(&mut list, &mut backend, 2500, 1.0);
        assert_eq!(list.fades.len(), 1);
        assert_eq!(list.fades[0].time, 2500);
        assert_eq!(list.shows[0].volume.to_bits(), 0.25f32.to_bits());
        assert_eq!(backend.volumes.len(), 1);
        assert_eq!(backend.volumes[0].0, show);
        assert_eq!(backend.volumes[0].1.to_bits(), 0.25f32.to_bits());
    }

    #[test]
    fn destroy_all_releases_every_show_back_to_front_and_empties_lists() {
        let mut list = CGameVideolist::fresh();
        let mut backend = Fake::new();
        let first = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        let second = cgame_create_ishow(&mut list, &mut backend, 0, false, 1.0);
        let first_show = cgame_get_ishow(&list, first).unwrap();
        let second_show = cgame_get_ishow(&list, second).unwrap();
        list.fades.push(CGameIshowFade {
            id: first,
            from: 0.5,
            to: 0.0,
            time: 0,
            length: 5000,
        });
        list.film_music_streams.push(77);
        backend.destroyed.clear();
        cgame_destroy_ishow_all(&mut list, &mut backend);
        assert_eq!(
            backend.destroyed,
            vec![(second, second_show), (first, first_show)]
        );
        assert!(list.shows.is_empty());
        assert!(list.fades.is_empty());
        assert!(list.film_music_streams.is_empty());
    }
}

// ---------------------------------------------------------------------------
// C_game IShow/film-music STUB marker-only contracts (Game.exe, NOT LS3DF).
//
// Every entry below is tier contract: reviewed mafia-re source at pin
// 113b5b6 plus pinning tests. No Game.exe body was re-observed here; the
// native bodies are StubLog-only (empty), so each call returns explicit
// Unsupported and, by taking only a shared list borrow, cannot mutate host
// state: the destroy row is kept, the fade list is untouched, the film
// stream tokens are kept. Device Close/Release, the native ISND_fade owner,
// the fade iteration, the film-music stream registers and the stream
// teardown stay DEFERRED with no backend and no host claims.
//
// Decoder-side reference for the deferred device side (already-ported CPU
// pattern, device still behind backend): ISND_source::GetPlayTime (LS3DF
// 0x1007d8d0, mafia-re ls3df/ISound/IS_source.cpp:164-169) and
// ISND_source::GetFormat (LS3DF 0x1007d910, mafia-re
// ls3df/ISound/IS_source.cpp:173-177), consumed by the channel mixer
// (mafia-re ls3df/ISound/IS_sound.cpp:40,54,77).

/// Still-unimplemented C_game video/film-music STUBs. The entries name the
/// original Game.exe addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CGameIshowStubError {
    Unsupported { entry: u32 },
}

/// `C_game::DestroyIShow` GAME 0x5b62c0.
pub const DESTROY_ISHOW_ENTRY: u32 = 0x5b62c0;
/// `C_game::IShowFade` GAME 0x5b6710.
pub const ISHOW_FADE_ENTRY: u32 = 0x5b6710;
/// `C_game::IShowFadeDownAll` GAME 0x5b6660.
pub const ISHOW_FADE_DOWN_ALL_ENTRY: u32 = 0x5b6660;
/// `C_game::SetFilmMusic` GAME 0x5ae1d0.
pub const SET_FILM_MUSIC_ENTRY: u32 = 0x5ae1d0;
/// `C_game::GetFilmMusic` GAME 0x5ae390.
pub const GET_FILM_MUSIC_ENTRY: u32 = 0x5ae390;
/// `C_game::StreamDestroyAll` GAME 0x5ae3d0.
pub const STREAM_DESTROY_ALL_ENTRY: u32 = 0x5ae3d0;

/// GAME 0x5b62c0. `C_game::DestroyIShow` (mafia-re game/mission/game.cpp:
/// 3507-3511): native body is an empty StubLog stub, so this single-row
/// destroy sequence returns explicit Unsupported; the device Close/Release
/// stays deferred and the row is kept. Callers of the deferred sequence:
/// CityMusicOff (game.cpp:3016-3029) and the per-row teardown of
/// DestroyIShowAll (game.cpp:3515-3529).
pub fn cgame_destroy_ishow(
    _list: &CGameVideolist,
    _id: i32,
) -> Result<(), CGameIshowStubError> {
    Err(CGameIshowStubError::Unsupported {
        entry: DESTROY_ISHOW_ENTRY,
    })
}

/// GAME 0x5b6710. `C_game::IShowFade` (mafia-re game/mission/game.cpp:
/// 3588-3592): native body is an empty StubLog stub, so this fade-record
/// stage returns explicit Unsupported; the native ISND_fade owner stays
/// deferred. Staged from TickCityMusic (game.cpp:1780,1789) and
/// CityMusicOff (game.cpp:3026); consumed by the Tick fade walk
/// (game.cpp:2098-2138), whose rows callers stage directly.
pub fn cgame_ishow_fade(
    _list: &CGameVideolist,
    _id: i32,
    _from: f32,
    _to: f32,
    _time_ms: i32,
) -> Result<(), CGameIshowStubError> {
    Err(CGameIshowStubError::Unsupported {
        entry: ISHOW_FADE_ENTRY,
    })
}

/// GAME 0x5b6660. `C_game::IShowFadeDownAll` (mafia-re
/// game/mission/game.cpp:3582-3586): native body is an empty StubLog stub,
/// so this iteration over the fades returns explicit Unsupported; both the
/// walk and the fade device owner stay deferred.
pub fn cgame_ishow_fade_down_all(
    _list: &CGameVideolist,
    _time_ms: i32,
) -> Result<(), CGameIshowStubError> {
    Err(CGameIshowStubError::Unsupported {
        entry: ISHOW_FADE_DOWN_ALL_ENTRY,
    })
}

/// GAME 0x5ae1d0. `C_game::SetFilmMusic` (mafia-re game/mission/game.cpp:
/// 3037-3041): native body is an empty StubLog stub, so this film-music
/// register returns explicit Unsupported; the stream device op stays
/// deferred.
pub fn cgame_set_film_music(
    _list: &CGameVideolist,
    _music: i32,
) -> Result<(), CGameIshowStubError> {
    Err(CGameIshowStubError::Unsupported {
        entry: SET_FILM_MUSIC_ENTRY,
    })
}

/// GAME 0x5ae390. `C_game::GetFilmMusic` (mafia-re game/mission/game.cpp:
/// 3043-3048): native stub body only logs and returns the -1 placeholder, so
/// this film-music read returns explicit Unsupported instead of replaying
/// the placeholder; the stream register stays deferred.
pub fn cgame_get_film_music(
    _list: &CGameVideolist,
    _index: i32,
) -> Result<i32, CGameIshowStubError> {
    Err(CGameIshowStubError::Unsupported {
        entry: GET_FILM_MUSIC_ENTRY,
    })
}

/// GAME 0x5ae3d0. `C_game::StreamDestroyAll` (mafia-re
/// game/mission/game.cpp:3050-3054): native body is an empty StubLog stub,
/// so this stream teardown returns explicit Unsupported; the film-music
/// stream tokens are kept and the device teardown stays deferred.
pub fn cgame_stream_destroy_all(
    _list: &CGameVideolist,
) -> Result<(), CGameIshowStubError> {
    Err(CGameIshowStubError::Unsupported {
        entry: STREAM_DESTROY_ALL_ENTRY,
    })
}

#[cfg(test)]
mod cgame_ishow_stub_tests {
    use super::*;

    fn stubbed_list() -> CGameVideolist {
        let mut list = CGameVideolist::fresh();
        list.shows.push(CGameIshowEntry {
            show: 11,
            id: 3,
            flags: 0,
            name: Some(b"clip.ogg".to_vec()),
            volume: 0.5,
        });
        list.fades.push(CGameIshowFade {
            id: 3,
            from: 0.5,
            to: 0.0,
            time: 0,
            length: 5000,
        });
        list.film_music_streams.push(77);
        list
    }

    #[test]
    fn stub_0x5b62c0_destroy_ishow_is_unsupported_and_keeps_row() {
        let list = stubbed_list();
        let before = list.clone();
        assert_eq!(
            cgame_destroy_ishow(&list, 3),
            Err(CGameIshowStubError::Unsupported { entry: 0x5b62c0 })
        );
        assert!(cgame_destroy_ishow(&list, 3).is_err());
        assert_eq!(list, before);
    }

    #[test]
    fn stub_0x5b6710_ishow_fade_is_unsupported_and_keeps_fades() {
        let list = stubbed_list();
        let before = list.clone();
        assert_eq!(
            cgame_ishow_fade(&list, 3, 0.5, 0.0, 5000),
            Err(CGameIshowStubError::Unsupported { entry: 0x5b6710 })
        );
        assert!(cgame_ishow_fade(&list, 3, 0.5, 0.0, 5000).is_err());
        assert_eq!(list, before);
    }

    #[test]
    fn stub_0x5b6660_ishow_fade_down_all_is_unsupported_and_keeps_fades() {
        let list = stubbed_list();
        let before = list.clone();
        assert_eq!(
            cgame_ishow_fade_down_all(&list, 5000),
            Err(CGameIshowStubError::Unsupported { entry: 0x5b6660 })
        );
        assert!(cgame_ishow_fade_down_all(&list, 5000).is_err());
        assert_eq!(list, before);
    }

    #[test]
    fn stub_0x5ae1d0_set_film_music_is_unsupported_and_keeps_streams() {
        let list = stubbed_list();
        let before = list.clone();
        assert_eq!(
            cgame_set_film_music(&list, 2),
            Err(CGameIshowStubError::Unsupported { entry: 0x5ae1d0 })
        );
        assert!(cgame_set_film_music(&list, 2).is_err());
        assert_eq!(list, before);
    }

    #[test]
    fn stub_0x5ae390_get_film_music_is_unsupported_not_placeholder() {
        let list = stubbed_list();
        let before = list.clone();
        assert_eq!(
            cgame_get_film_music(&list, 0),
            Err(CGameIshowStubError::Unsupported { entry: 0x5ae390 })
        );
        assert!(cgame_get_film_music(&list, 0).is_err());
        assert_eq!(list, before);
    }

    #[test]
    fn stub_0x5ae3d0_stream_destroy_all_is_unsupported_and_keeps_streams() {
        let list = stubbed_list();
        let before = list.clone();
        assert_eq!(
            cgame_stream_destroy_all(&list),
            Err(CGameIshowStubError::Unsupported { entry: 0x5ae3d0 })
        );
        assert!(cgame_stream_destroy_all(&list).is_err());
        assert_eq!(list, before);
    }
}

