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
