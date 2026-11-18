//! Live resource intake for native menu components and indicator text.
//!
//! Host intake is not GM_Menu::LoadResources, sound loading or menu-count
//! lifecycle. The CPU adapter moves the canonical TextureResources/storage pair;
//! it is not ModelArena's image loader or a scene-resource mirror. GPU/device
//! composition remains with the host through borrowed surfaces or another
//! MenuTextureBackend backed by the actual owner.
//!
//! Static basis: Game.exe SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000; GetText RVA 0x20fb40, LoadFont RVA 0x1f6f30; REA and
//! mafia-re fe7e7464 game/ui/g_textdb.cpp and g_indicators.cpp. Asset parsers
//! admit complete text/language/GMF images, not native partial reads.
//! No original-runtime or complete menu-lifecycle claim is made.

#![forbid(unsafe_code)]

use crate::itexture::{
    CpuTextureStorage, CpuTextureSurface, CreateDeviceCaps, TextureImageSource, TextureKey,
    TextureResources, TextureState,
};
use lhr_assets::{FontMetrics, Installation, LanguageFontMap};
use std::sync::Arc;

pub const MENU_FONT_COUNT: usize = 4;
pub const MENU_ALPHA_TEXTURE_FLAGS: u32 = 0x2;

/// Native setters borrow the same owner subsequently consumed by Draw.
pub trait MenuComponentResources {
    /// ID zero is a component's null string, without a database lookup.
    fn text(&mut self, text_id: u32) -> Result<Option<Arc<[u8]>>, String>;
    /// CreateITexture(name, NULL, ITC_ALPHA). None is native creation failure;
    /// host failures remain Err. A null primary name is not an empty slice.
    fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String>;
    /// Err may follow partial external effects and MUST NOT be replayed.
    /// Keep the key only as an interrupted ownership record, not a drawable or
    /// retryable reference. Clear the component's slot only after Ok. Explicit
    /// reconciliation belongs to the actual backend, not a recreated mirror.
    fn release_texture(&mut self, key: TextureKey) -> Result<(), String>;
}

/// Canonical filename factory and actual pixel storage. Failure and release
/// semantics match MenuComponentResources; surfaces borrow live storage.
pub trait MenuTextureBackend {
    fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String>;
    fn create_indicator_texture(
        &mut self,
        _name: &[u8],
        _alpha: Option<&[u8]>,
        _flags: u32,
    ) -> Result<Option<TextureKey>, String> {
        Err("indicator CreateITexture backend unavailable".into())
    }
    fn release_texture(&mut self, key: TextureKey) -> Result<(), String>;
    fn texture_surface(&self, key: TextureKey) -> Result<&CpuTextureSurface, String>;

    /// Borrow the canonical CPU pair when this backend exposes that capability.
    /// None is absence of composition, not permission to recreate its state.
    fn cpu_texture_context(&self) -> Option<(&TextureResources, &CpuTextureStorage)> {
        None
    }

    /// Scene and menu operations share this pair, its identities and lifetimes.
    /// Callers must preserve the owner pair rather than replace either half.
    fn cpu_texture_context_mut(
        &mut self,
    ) -> Option<(&mut TextureResources, &mut CpuTextureStorage)> {
        None
    }
}

pub struct MenuFontView<'a> {
    pub metrics: &'a FontMetrics,
    pub glyph_map: &'a [u16; 256],
    /// None records native creation failure or completed release, never
    /// substitutes for missing font intake.
    pub atlas: Option<TextureKey>,
}

pub trait MenuDrawResourceAccess: MenuComponentResources {
    fn font(&self, font: u32) -> Result<MenuFontView<'_>, String>;
    fn texture_surface(&self, key: TextureKey) -> Result<&CpuTextureSurface, String>;
}

/// None is failed dtaOpen, Err a host failure. Some supplies a complete
/// 0x101c-byte GMF read/close intake, never an implicitly successful short read.
pub trait MenuFontMetricsSource {
    fn read_font_metrics(&mut self, path: &[u8]) -> Result<Option<Vec<u8>>, String>;
}

pub(crate) struct InstallationFontMetrics<'a>(pub &'a Installation);

impl MenuFontMetricsSource for InstallationFontMetrics<'_> {
    fn read_font_metrics(&mut self, path: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let path =
            std::str::from_utf8(path).map_err(|_| "indicator GMF path is not supported UTF-8")?;
        let name = path
            .strip_prefix("tables\\")
            .ok_or("indicator GMF path lacks its native tables prefix")?;
        if name.contains(['/', '\\']) {
            return Err("indicator GMF filename contains a path separator".into());
        }
        self.0
            .read_optional(&format!("tables/{name}"))
            .map_err(|error| format!("indicator GMF {path}: {error}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuFontLanguage {
    Primary,
    Credits,
}

impl MenuFontLanguage {
    fn index(self) -> usize {
        match self {
            Self::Primary => 0,
            Self::Credits => 1,
        }
    }
}

/// Reached LoadFont effects, not a claim that all draw dependencies exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuFontLoadOutcome {
    pub atlas_created: bool,
    pub metrics_read: bool,
    pub metrics_available: bool,
}

struct MenuTextEntry {
    id: u32,
    bytes: Arc<[u8]>,
}

/// Sole raw-text owner. Component Arcs share these byte strings without
/// decoding or cloning another database owner.
pub struct MenuTextDatabase {
    entries: Vec<MenuTextEntry>,
    unknown: Option<u32>,
    missing: Arc<[u8]>,
}

impl Default for MenuTextDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuTextDatabase {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            unknown: None,
            missing: Arc::from(&b" "[..]),
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut database = Self::new();
        database.load_from_bytes(bytes)?;
        Ok(database)
    }

    /// Complete-file host intake, not native stream reads. The previous table
    /// is discarded first; component Arcs already issued remain valid.
    pub fn load_from_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.reset();
        let database = lhr_assets::parse_raw_text_db(bytes).map_err(|error| error.to_string())?;
        self.entries = database
            .entries
            .into_iter()
            .map(|entry| MenuTextEntry {
                id: entry.id,
                bytes: Arc::from(entry.bytes),
            })
            .collect();
        self.unknown = Some(database.unknown);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.entries.clear();
        self.unknown = None;
    }

    pub fn is_loaded(&self) -> bool {
        self.unknown.is_some()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn unknown(&self) -> Option<u32> {
        self.unknown
    }

    /// GetText, not GetTextOrNULL: preserve its strict initial probe and
    /// single-space miss. Native out-of-table probes are unsupported, not
    /// repaired by a sorted copy, sentinel or different search algorithm.
    pub fn get_text(&self, text_id: u32) -> Result<Arc<[u8]>, String> {
        let mut step = 0x1000;
        while step != 0 && step > self.entries.len() {
            step >>= 1;
        }
        if step == 0 {
            return Ok(Arc::clone(&self.missing));
        }
        let initial = self.entry(step)?;
        let mut position = if initial.id < text_id {
            self.entries.len() - step - 1
        } else {
            0
        };
        step >>= 1;
        while step != 0 {
            let index = position + step;
            step >>= 1;
            if self.entry(index)?.id <= text_id {
                position = index;
            }
        }
        let entry = self.entry(position)?;
        Ok(if entry.id == text_id {
            Arc::clone(&entry.bytes)
        } else {
            Arc::clone(&self.missing)
        })
    }

    fn entry(&self, index: usize) -> Result<&MenuTextEntry, String> {
        self.entries.get(index).ok_or_else(|| {
            format!(
                "GetText unsupported native table probe {index} for {} entries",
                self.entries.len()
            )
        })
    }
}

/// Owns the canonical immediate CPU factory pair; no ModelArena cache or
/// second texture arena is created. Device/scene composition is not supplied.
pub struct CpuMenuTextureFactory<S> {
    textures: TextureResources,
    storage: CpuTextureStorage,
    source: S,
}

impl<S: TextureImageSource> CpuMenuTextureFactory<S> {
    pub fn new(
        textures: TextureResources,
        storage: CpuTextureStorage,
        source: S,
    ) -> Result<Self, String> {
        textures
            .prove_cpu_immediate_domain(&storage)
            .map_err(|error| format!("menu CPU texture intake: {error:?}"))?;
        Ok(Self {
            textures,
            storage,
            source,
        })
    }

    pub fn texture_resources(&self) -> &TextureResources {
        &self.textures
    }

    pub fn texture_storage(&self) -> &CpuTextureStorage {
        &self.storage
    }
}

impl<S: TextureImageSource> MenuTextureBackend for CpuMenuTextureFactory<S> {
    fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String> {
        match self.textures.create_texture_from_file(
            Some(name),
            None,
            MENU_ALPHA_TEXTURE_FLAGS,
            &mut self.source,
            &mut self.storage,
        ) {
            Ok(key) => Ok(Some(key)),
            Err(error) if error.is_native_creation_failure() => Ok(None),
            Err(error) => Err(format!("menu CreateITexture: {error:?}")),
        }
    }

    fn create_indicator_texture(
        &mut self,
        name: &[u8],
        alpha: Option<&[u8]>,
        flags: u32,
    ) -> Result<Option<TextureKey>, String> {
        match self.textures.create_texture_full(
            Some(name),
            None,
            alpha,
            flags,
            0,
            0,
            0,
            &CreateDeviceCaps::CPU,
            &mut self.source,
            &mut self.storage,
        ) {
            Ok((key, _)) => Ok(Some(key)),
            Err(error) if error.is_native_creation_failure() => Ok(None),
            Err(error) => Err(format!("indicator CreateITexture: {error:?}")),
        }
    }

    fn release_texture(&mut self, key: TextureKey) -> Result<(), String> {
        self.textures
            .release_texture(key, &mut self.storage)
            .map(|_| ())
            .map_err(|error| format!("menu ITexture::Release interrupted: {error:?}"))
    }

    fn texture_surface(&self, key: TextureKey) -> Result<&CpuTextureSurface, String> {
        if self.textures.is_poisoned() {
            return Err("menu texture owner is poisoned".into());
        }
        let texture = self
            .textures
            .texture(key)
            .map_err(|error| format!("menu texture identity: {error:?}"))?;
        if texture.state != TextureState::Live || texture.references == 0 || !texture.initialized {
            return Err("menu texture is not a live initialized reference".into());
        }
        let graphics = texture
            .graphics
            .ok_or_else(|| "menu texture has no graphics surface".to_owned())?;
        self.storage
            .surface(graphics)
            .ok_or_else(|| "menu texture surface is not in its canonical CPU storage".into())
    }

    fn cpu_texture_context(&self) -> Option<(&TextureResources, &CpuTextureStorage)> {
        Some((&self.textures, &self.storage))
    }

    fn cpu_texture_context_mut(
        &mut self,
    ) -> Option<(&mut TextureResources, &mut CpuTextureStorage)> {
        Some((&mut self.textures, &mut self.storage))
    }
}

#[derive(Default)]
struct MenuFontSlot {
    metrics: Option<FontMetrics>,
    language: Option<MenuFontLanguage>,
    atlas: Option<TextureKey>,
    load_error: Option<String>,
    release_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndicatorTexture {
    Map,
    Logo,
    Sight,
    RaceStart,
    RaceWrong,
    Crew,
    Instrument,
    Instrument2,
    Numbers,
    Textura,
    RadarClip,
    Icons,
    Fuel,
}

impl IndicatorTexture {
    const DESTROY_ORDER: [Self; 13] = [
        Self::Instrument,
        Self::Instrument2,
        Self::Numbers,
        Self::Textura,
        Self::RadarClip,
        Self::Icons,
        Self::Fuel,
        Self::Map,
        Self::Logo,
        Self::Sight,
        Self::RaceStart,
        Self::RaceWrong,
        Self::Crew,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndicatorAssetsPhase {
    Empty,
    Creating,
    Created,
    Destroying,
}

/// Single session-owned resource intake, borrowed by setters and presentation.
/// Construction does not load fonts, create menu sounds or modify menu counts.
/// Native release is explicit; host Drop is not credited as native Destroy.
pub struct MenuDrawResources {
    text_database: MenuTextDatabase,
    textures: Box<dyn MenuTextureBackend>,
    languages: [Option<LanguageFontMap>; 2],
    fonts: [MenuFontSlot; MENU_FONT_COUNT],
    indicator_textures: [Option<TextureKey>; IndicatorTexture::DESTROY_ORDER.len()],
    indicator_assets: IndicatorAssetsPhase,
}

impl std::fmt::Debug for MenuDrawResources {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MenuDrawResources")
            .field("text_database_loaded", &self.text_database.is_loaded())
            .field("text_entries", &self.text_database.len())
            .field("indicator_assets", &self.indicator_assets)
            .field(
                "languages_installed",
                &self.languages.each_ref().map(Option::is_some),
            )
            .field(
                "font_atlases",
                &self.fonts.each_ref().map(|slot| slot.atlas),
            )
            .field(
                "font_load_incomplete",
                &self.fonts.each_ref().map(|slot| slot.load_error.is_some()),
            )
            .field(
                "font_release_interrupted",
                &self
                    .fonts
                    .each_ref()
                    .map(|slot| slot.release_error.is_some()),
            )
            .finish_non_exhaustive()
    }
}

impl MenuDrawResources {
    pub fn new(
        text_database: MenuTextDatabase,
        textures: impl MenuTextureBackend + 'static,
    ) -> Self {
        Self {
            text_database,
            textures: Box::new(textures),
            languages: [None, None],
            fonts: std::array::from_fn(|_| MenuFontSlot::default()),
            indicator_textures: [None; IndicatorTexture::DESTROY_ORDER.len()],
            indicator_assets: IndicatorAssetsPhase::Empty,
        }
    }

    pub fn from_cpu_factory<S: TextureImageSource + 'static>(
        text_database: MenuTextDatabase,
        textures: TextureResources,
        storage: CpuTextureStorage,
        source: S,
    ) -> Result<Self, String> {
        Ok(Self::new(
            text_database,
            CpuMenuTextureFactory::new(textures, storage, source)?,
        ))
    }

    pub fn text_database(&self) -> &MenuTextDatabase {
        &self.text_database
    }

    pub fn text_database_mut(&mut self) -> &mut MenuTextDatabase {
        &mut self.text_database
    }

    pub fn text(&mut self, text_id: u32) -> Result<Option<Arc<[u8]>>, String> {
        if text_id == 0 {
            Ok(None)
        } else {
            self.text_database.get_text(text_id).map(Some)
        }
    }

    pub fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String> {
        self.textures.create_alpha_texture(name)
    }

    pub fn release_texture(&mut self, key: TextureKey) -> Result<(), String> {
        self.textures.release_texture(key)
    }

    pub fn texture_surface(&self, key: TextureKey) -> Result<&CpuTextureSurface, String> {
        self.textures.texture_surface(key)
    }

    /// Shared scene/menu access to the actual backend, without an arena snapshot.
    pub fn cpu_texture_context(&self) -> Option<(&TextureResources, &CpuTextureStorage)> {
        self.textures.cpu_texture_context()
    }

    /// Mutations use the same factory pair subsequently borrowed for Draw.
    /// This is not reconciliation of an interrupted component or font Release.
    pub fn cpu_texture_context_mut(
        &mut self,
    ) -> Option<(&mut TextureResources, &mut CpuTextureStorage)> {
        self.textures.cpu_texture_context_mut()
    }

    /// Host language-image intake only. Bound fonts read this live owner's
    /// translation table, including after replacement; no table snapshot binds.
    pub fn install_language(
        &mut self,
        language: MenuFontLanguage,
        bytes: &[u8],
    ) -> Result<(), String> {
        let mapping = lhr_assets::parse_language_font_map(bytes)
            .map_err(|error| format!("menu language intake: {error}"))?;
        self.languages[language.index()] = Some(mapping);
        Ok(())
    }

    pub fn language(&self, language: MenuFontLanguage) -> Result<&LanguageFontMap, String> {
        self.languages[language.index()]
            .as_ref()
            .ok_or_else(|| format!("menu {language:?} language is uncomposed"))
    }

    pub fn font(&self, font: u32) -> Result<MenuFontView<'_>, String> {
        let slot = &self.fonts[Self::font_index(font)?];
        if let Some(error) = &slot.release_error {
            return Err(format!(
                "menu font {font} Release interrupted, no draw/replay: {error}"
            ));
        }
        if let Some(error) = &slot.load_error {
            return Err(format!("menu font {font} LoadFont incomplete: {error}"));
        }
        let metrics = slot
            .metrics
            .as_ref()
            .ok_or_else(|| format!("menu font {font} metrics are uncomposed"))?;
        let language = slot
            .language
            .ok_or_else(|| format!("menu font {font} translation is unbound"))?;
        Ok(MenuFontView {
            metrics,
            glyph_map: &self.language(language)?.glyph_map,
            atlas: slot.atlas,
        })
    }

    /// LoadFont's release/null/create/read/bind order. A failed native open
    /// retains old metrics; host errors retain reached effects without drawing
    /// an incomplete font. This is not native partial-read emulation.
    pub fn load_font(
        &mut self,
        language: MenuFontLanguage,
        font: u32,
        source: &mut dyn MenuFontMetricsSource,
    ) -> Result<MenuFontLoadOutcome, String> {
        let index = Self::font_index(font)?;
        self.release_font_texture(font)?;
        self.fonts[index].load_error = Some("LoadFont completion not established".into());
        let outcome = self.load_font_after_release(language, font, index, source);
        match &outcome {
            Ok(_) => self.fonts[index].load_error = None,
            Err(error) => self.fonts[index].load_error = Some(error.clone()),
        }
        outcome
    }

    fn load_font_after_release(
        &mut self,
        language: MenuFontLanguage,
        font: u32,
        index: usize,
        source: &mut dyn MenuFontMetricsSource,
    ) -> Result<MenuFontLoadOutcome, String> {
        let name = self.font_asset_name(language, font, b"", b".tga")?;
        self.fonts[index].atlas = self.textures.create_alpha_texture(&name)?;
        let path = self.font_asset_name(language, font, b"tables\\", b".gmf")?;
        let bytes = source.read_font_metrics(&path)?;
        let metrics_read = bytes.is_some();
        if let Some(bytes) = bytes {
            self.fonts[index].metrics = Some(
                lhr_assets::parse_font_metrics(&bytes)
                    .map_err(|error| format!("menu GMF complete-read intake: {error}"))?,
            );
        }
        self.fonts[index].language = Some(language);
        Ok(MenuFontLoadOutcome {
            atlas_created: self.fonts[index].atlas.is_some(),
            metrics_read,
            metrics_available: self.fonts[index].metrics.is_some(),
        })
    }

    fn font_asset_name(
        &self,
        language: MenuFontLanguage,
        font: u32,
        prefix: &[u8],
        extension: &[u8],
    ) -> Result<Vec<u8>, String> {
        let suffix = &self.language(language)?.raw_header[40..56];
        let end = suffix
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| "menu font suffix lacks an admitted native NUL terminator".to_owned())?;
        let mut name = prefix.to_vec();
        name.extend_from_slice(format!("font{font}").as_bytes());
        name.extend_from_slice(&suffix[..end]);
        name.extend_from_slice(extension);
        Ok(name)
    }

    /// Null the slot only after definite Release success. On error the key is
    /// an interrupted record, never a reference eligible for draw or replay.
    pub fn release_font_texture(&mut self, font: u32) -> Result<(), String> {
        if self.indicator_assets == IndicatorAssetsPhase::Destroying {
            return Err("indicator assets Destroy interrupted, no font changes".into());
        }
        self.release_font_texture_inner(font)
    }

    fn release_font_texture_inner(&mut self, font: u32) -> Result<(), String> {
        let slot = &mut self.fonts[Self::font_index(font)?];
        if let Some(error) = &slot.release_error {
            return Err(format!(
                "menu font {font} Release interrupted, no replay: {error}"
            ));
        }
        let Some(key) = slot.atlas else {
            return Ok(());
        };
        slot.release_error = Some("Release completion not established".into());
        match self.textures.release_texture(key) {
            Ok(()) => {
                slot.atlas = None;
                slot.release_error = None;
                Ok(())
            }
            Err(error) => {
                slot.release_error = Some(error.clone());
                Err(error)
            }
        }
    }

    /// Indicator Destroy's font prefix only, in native slot order 0..3.
    /// This does not release component-owned keys, sounds or other indicators.
    pub fn release_font_textures(&mut self) -> Result<(), String> {
        for font in 0..MENU_FONT_COUNT as u32 {
            self.release_font_texture(font)?;
        }
        Ok(())
    }

    /// Texture and font prefix of Indicators::Create (Game.exe RVA 0x1f70e0).
    /// This does not run ResetValues or establish a live graphics device.
    /// The CPU full-create path retains raw ARGB upload bytes in storage.
    /// These HUD slots are not draw inputs; only the BGRA font atlases are.
    pub(crate) fn create_indicator_assets(
        &mut self,
        metrics: &mut dyn MenuFontMetricsSource,
    ) -> Result<(), String> {
        use crate::itexture::{ITC_ALPHA, ITC_COLORKEY, ITC_TRUECOLOR, ITC_USE_ALPHA};
        use IndicatorTexture as Slot;

        if self.indicator_assets != IndicatorAssetsPhase::Empty {
            return Err(
                "indicator assets Create would replace an active or interrupted prefix".into(),
            );
        }
        self.indicator_assets = IndicatorAssetsPhase::Creating;
        self.indicator_textures[Slot::Map as usize] = None;
        self.create_indicator_texture(Slot::Logo, b"logoplay.bmp", None, 0)?;
        self.create_indicator_texture(
            Slot::Sight,
            b"2sight.bmp",
            Some(b"2sight+.bmp"),
            ITC_TRUECOLOR | ITC_USE_ALPHA,
        )?;
        self.create_indicator_texture(Slot::RaceStart, b"racestart.bmp", None, ITC_ALPHA)?;
        self.create_indicator_texture(Slot::RaceWrong, b"racewrong.bmp", None, 0)?;
        self.create_indicator_texture(
            Slot::Crew,
            b"2blood.bmp",
            Some(b"2blood+.bmp"),
            ITC_USE_ALPHA,
        )?;
        self.create_indicator_texture(Slot::Instrument, b"1int.tga", None, ITC_COLORKEY)?;
        self.indicator_textures[Slot::Instrument2 as usize] = None;
        self.create_indicator_texture(Slot::Numbers, b"1numbers.tga", None, ITC_ALPHA)?;
        self.create_indicator_texture(Slot::Textura, b"0textura.tga", None, ITC_ALPHA)?;
        self.create_indicator_texture(Slot::RadarClip, b"radarclp.bmp", None, ITC_COLORKEY)?;
        self.indicator_textures[Slot::Icons as usize] = None;
        self.create_indicator_texture(Slot::Fuel, b"2intbenal.tga", None, ITC_COLORKEY)?;
        for font in 0..MENU_FONT_COUNT as u32 {
            self.load_font(MenuFontLanguage::Primary, font, metrics)?;
        }
        self.indicator_assets = IndicatorAssetsPhase::Created;
        Ok(())
    }

    fn create_indicator_texture(
        &mut self,
        slot: IndicatorTexture,
        name: &[u8],
        alpha: Option<&[u8]>,
        flags: u32,
    ) -> Result<(), String> {
        self.indicator_textures[slot as usize] = self
            .textures
            .create_indicator_texture(name, alpha, flags)
            .map_err(|error| format!("indicator {slot:?}: {error}"))?;
        Ok(())
    }

    /// Releases the represented Destroy slots, including an incomplete Create.
    /// The caller must first dispose of any menu users of the font atlases.
    pub(crate) fn destroy_indicator_assets(&mut self) -> Result<(), String> {
        if self.indicator_assets == IndicatorAssetsPhase::Destroying {
            return Err("indicator assets Destroy interrupted, no replay".into());
        }
        self.indicator_assets = IndicatorAssetsPhase::Destroying;
        for font in 0..MENU_FONT_COUNT as u32 {
            self.release_font_texture_inner(font)?;
        }
        for slot in IndicatorTexture::DESTROY_ORDER {
            if let Some(key) = self.indicator_textures[slot as usize] {
                self.textures
                    .release_texture(key)
                    .map_err(|error| format!("indicator {slot:?} Release: {error}"))?;
                self.indicator_textures[slot as usize] = None;
            }
        }
        self.indicator_assets = IndicatorAssetsPhase::Empty;
        Ok(())
    }

    fn font_index(font: u32) -> Result<usize, String> {
        let index = usize::try_from(font).map_err(|_| "menu font index exceeds host usize")?;
        if index >= MENU_FONT_COUNT {
            return Err(format!(
                "menu font index {font} is outside the four native slots"
            ));
        }
        Ok(index)
    }
}

impl MenuComponentResources for MenuDrawResources {
    fn text(&mut self, text_id: u32) -> Result<Option<Arc<[u8]>>, String> {
        MenuDrawResources::text(self, text_id)
    }

    fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String> {
        MenuDrawResources::create_alpha_texture(self, name)
    }

    fn release_texture(&mut self, key: TextureKey) -> Result<(), String> {
        MenuDrawResources::release_texture(self, key)
    }
}

impl MenuDrawResourceAccess for MenuDrawResources {
    fn font(&self, font: u32) -> Result<MenuFontView<'_>, String> {
        MenuDrawResources::font(self, font)
    }

    fn texture_surface(&self, key: TextureKey) -> Result<&CpuTextureSurface, String> {
        MenuDrawResources::texture_surface(self, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itexture::TextureImageError;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Create(Vec<u8>),
        CreateIndicator(Vec<u8>, Option<Vec<u8>>, u32),
        Release(TextureKey),
        Read(Vec<u8>),
    }

    type Events = Rc<RefCell<Vec<Event>>>;
    type ReleaseFailure = Rc<RefCell<Option<TextureKey>>>;

    struct Images {
        result: Result<Vec<u8>, TextureImageError>,
        reads: Vec<Vec<u8>>,
    }

    impl TextureImageSource for Images {
        fn read_image(&mut self, path: &[u8]) -> Result<Vec<u8>, TextureImageError> {
            self.reads.push(path.to_vec());
            if self.result.is_ok() && path.ends_with(b".bmp") {
                let pixels = if path.ends_with(b"+.bmp") {
                    [64; 3]
                } else {
                    [3, 2, 1]
                };
                return Ok(indicator_bmp(pixels));
            }
            self.result.clone()
        }
    }

    struct LoggedTextures {
        factory: CpuMenuTextureFactory<Images>,
        events: Events,
        release_failure: ReleaseFailure,
        expose_cpu_context: bool,
    }

    impl MenuTextureBackend for LoggedTextures {
        fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String> {
            self.events.borrow_mut().push(Event::Create(name.to_vec()));
            self.factory.create_alpha_texture(name)
        }

        fn create_indicator_texture(
            &mut self,
            name: &[u8],
            alpha: Option<&[u8]>,
            flags: u32,
        ) -> Result<Option<TextureKey>, String> {
            self.events.borrow_mut().push(Event::CreateIndicator(
                name.to_vec(),
                alpha.map(<[u8]>::to_vec),
                flags,
            ));
            self.factory.create_indicator_texture(name, alpha, flags)
        }

        fn release_texture(&mut self, key: TextureKey) -> Result<(), String> {
            self.events.borrow_mut().push(Event::Release(key));
            self.factory.release_texture(key)?;
            if *self.release_failure.borrow() == Some(key) {
                return Err("injected host failure after actual release".into());
            }
            Ok(())
        }

        fn texture_surface(&self, key: TextureKey) -> Result<&CpuTextureSurface, String> {
            self.factory.texture_surface(key)
        }

        fn cpu_texture_context(&self) -> Option<(&TextureResources, &CpuTextureStorage)> {
            if self.expose_cpu_context {
                self.factory.cpu_texture_context()
            } else {
                None
            }
        }
    }

    struct Metrics {
        result: Result<Option<Vec<u8>>, String>,
        events: Events,
    }

    impl MenuFontMetricsSource for Metrics {
        fn read_font_metrics(&mut self, path: &[u8]) -> Result<Option<Vec<u8>>, String> {
            self.events.borrow_mut().push(Event::Read(path.to_vec()));
            self.result.clone()
        }
    }

    fn text_image(records: &[(u32, &[u8])]) -> Vec<u8> {
        let mut bytes = vec![0; 8 + records.len() * 8];
        bytes[..4].copy_from_slice(&(records.len() as u32).to_le_bytes());
        bytes[4..8].copy_from_slice(&17u32.to_le_bytes());
        for (index, (text_id, text)) in records.iter().enumerate() {
            let table = 8 + index * 8;
            let offset = bytes.len() as u32;
            bytes[table..table + 4].copy_from_slice(&text_id.to_le_bytes());
            bytes[table + 4..table + 8].copy_from_slice(&offset.to_le_bytes());
            bytes.extend_from_slice(text);
            bytes.push(0);
        }
        bytes
    }

    fn font_image(height: f32) -> Vec<u8> {
        let mut bytes = vec![0; 0x101c];
        bytes[..4].copy_from_slice(b"GFnt");
        bytes[8..12].copy_from_slice(&height.to_le_bytes());
        bytes[12..16].copy_from_slice(&1.0f32.to_le_bytes());
        bytes[16..20].copy_from_slice(&0.1f32.to_le_bytes());
        bytes[20..24].copy_from_slice(&0.02f32.to_le_bytes());
        bytes[24..28].copy_from_slice(&0.3f32.to_le_bytes());
        bytes
    }

    fn language_image(suffix: &[u8], glyph: u16) -> Vec<u8> {
        let mut bytes = vec![0; 56 + 256 * 2];
        bytes[..6].copy_from_slice(b"Langxx");
        bytes[8..12].copy_from_slice(b"Test");
        bytes[40..40 + suffix.len()].copy_from_slice(suffix);
        let offset = 56 + usize::from(b'A') * 2;
        bytes[offset..offset + 2].copy_from_slice(&glyph.to_le_bytes());
        bytes
    }

    fn texture_image() -> Vec<u8> {
        let mut bytes = vec![0; 18];
        bytes[2] = 2;
        bytes[12] = 2;
        bytes[14] = 2;
        bytes[16] = 32;
        bytes[17] = 0x20;
        bytes.extend_from_slice(&[3, 2, 1, 17, 6, 5, 4, 34, 9, 8, 7, 51, 12, 11, 10, 68]);
        bytes
    }

    fn indicator_bmp(pixel: [u8; 3]) -> Vec<u8> {
        let mut bytes = vec![0; 54];
        bytes[..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&70u32.to_le_bytes());
        bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&2u32.to_le_bytes());
        bytes[22..26].copy_from_slice(&2u32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
        for _ in 0..2 {
            bytes.extend(pixel);
            bytes.extend(pixel);
            bytes.extend([0, 0]);
        }
        bytes
    }

    fn factory(result: Result<Vec<u8>, TextureImageError>) -> CpuMenuTextureFactory<Images> {
        CpuMenuTextureFactory::new(
            TextureResources::new().unwrap(),
            CpuTextureStorage::default(),
            Images {
                result,
                reads: Vec::new(),
            },
        )
        .unwrap()
    }

    fn logged_owner() -> (MenuDrawResources, Events, ReleaseFailure) {
        logged_owner_with_cpu_context(false)
    }

    fn logged_owner_with_cpu_context(
        expose_cpu_context: bool,
    ) -> (MenuDrawResources, Events, ReleaseFailure) {
        let events = Rc::new(RefCell::new(Vec::new()));
        let release_failure = Rc::new(RefCell::new(None));
        let owner = MenuDrawResources::new(
            MenuTextDatabase::new(),
            LoggedTextures {
                factory: factory(Ok(texture_image())),
                events: Rc::clone(&events),
                release_failure: Rc::clone(&release_failure),
                expose_cpu_context,
            },
        );
        (owner, events, release_failure)
    }

    fn metrics_source(height: f32, events: &Events) -> Metrics {
        Metrics {
            result: Ok(Some(font_image(height))),
            events: Rc::clone(events),
        }
    }

    fn indicator_owner() -> (MenuDrawResources, Events, ReleaseFailure) {
        let (mut owner, events, release_failure) = logged_owner_with_cpu_context(true);
        owner
            .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
            .unwrap();
        owner
            .install_language(MenuFontLanguage::Credits, &language_image(b"_cz", 22))
            .unwrap();
        (owner, events, release_failure)
    }

    #[test]
    fn indicator_assets_use_native_create_and_destroy_order_with_live_storage() {
        use IndicatorTexture as Slot;

        let (mut owner, events, _) = indicator_owner();
        let mut metrics = metrics_source(2.0, &events);
        owner.create_indicator_assets(&mut metrics).unwrap();
        let mut expected: Vec<_> = [
            ("logoplay.bmp", None, 0),
            ("2sight.bmp", Some("2sight+.bmp"), 0x404),
            ("racestart.bmp", None, 2),
            ("racewrong.bmp", None, 0),
            ("2blood.bmp", Some("2blood+.bmp"), 4),
            ("1int.tga", None, 8),
            ("1numbers.tga", None, 2),
            ("0textura.tga", None, 2),
            ("radarclp.bmp", None, 8),
            ("2intbenal.tga", None, 8),
        ]
        .into_iter()
        .map(|(name, alpha, flags)| {
            Event::CreateIndicator(
                name.as_bytes().to_vec(),
                alpha.map(|name| name.as_bytes().to_vec()),
                flags,
            )
        })
        .collect();
        for font in 0..4 {
            expected.push(Event::Create(format!("font{font}_en.tga").into_bytes()));
            expected.push(Event::Read(
                format!("tables\\font{font}_en.gmf").into_bytes(),
            ));
        }
        assert!(owner.create_indicator_assets(&mut metrics).is_err());
        assert_eq!(*events.borrow(), expected);
        for slot in [Slot::Map, Slot::Instrument2, Slot::Icons] {
            assert_eq!(owner.indicator_textures[slot as usize], None);
        }
        let sight = owner.indicator_textures[Slot::Sight as usize].unwrap();
        let (textures, storage) = owner.cpu_texture_context().unwrap();
        let texture = textures.texture(sight).unwrap();
        assert_eq!(texture.creation_flags, 0x404);
        let surface = owner.texture_surface(sight).unwrap();
        assert!(std::ptr::eq(
            surface,
            storage.surface(texture.graphics.unwrap()).unwrap()
        ));
        let raw_argb_upload = &surface.bgra;
        assert_eq!(*raw_argb_upload, [64, 1, 2, 3].repeat(4));
        let mut released: Vec<_> = (0..4)
            .map(|font| {
                let view = owner.font(font).unwrap();
                assert_eq!(view.metrics.height_scale, 2.0);
                assert_eq!(view.glyph_map[usize::from(b'A')], 11);
                let atlas = view.atlas.unwrap();
                assert_eq!(
                    &owner.texture_surface(atlas).unwrap().bgra[..4],
                    &[3, 2, 1, 17]
                );
                atlas
            })
            .collect();
        assert_eq!(
            owner.create_alpha_texture(b"font0_en.tga").unwrap(),
            Some(released[0])
        );
        owner.release_texture(released[0]).unwrap();
        for slot in [
            Slot::Instrument,
            Slot::Numbers,
            Slot::Textura,
            Slot::RadarClip,
            Slot::Fuel,
            Slot::Logo,
            Slot::Sight,
            Slot::RaceStart,
            Slot::RaceWrong,
            Slot::Crew,
        ] {
            released.push(owner.indicator_textures[slot as usize].unwrap());
        }
        events.borrow_mut().clear();
        owner.destroy_indicator_assets().unwrap();
        assert_eq!(
            *events.borrow(),
            released
                .iter()
                .copied()
                .map(Event::Release)
                .collect::<Vec<_>>()
        );
        assert!(released
            .iter()
            .all(|key| owner.texture_surface(*key).is_err()));
        assert_eq!(owner.cpu_texture_context().unwrap().1.resident_bytes(), 0);
    }

    #[test]
    fn indicator_assets_destroy_releases_atlas_reached_before_gmf_error() {
        struct FailThirdFont(Metrics);

        impl MenuFontMetricsSource for FailThirdFont {
            fn read_font_metrics(&mut self, path: &[u8]) -> Result<Option<Vec<u8>>, String> {
                let result = self.0.read_font_metrics(path);
                if path == b"tables\\font2_en.gmf" {
                    Err("third GMF host read failed".into())
                } else {
                    result
                }
            }
        }

        let (mut owner, events, _) = indicator_owner();
        let mut metrics = FailThirdFont(metrics_source(2.0, &events));
        let error = owner.create_indicator_assets(&mut metrics).unwrap_err();
        assert_eq!(error, "third GMF host read failed");
        assert_eq!(events.borrow().len(), 16);
        assert_eq!(
            events.borrow().last(),
            Some(&Event::Read(b"tables\\font2_en.gmf".to_vec()))
        );
        let partial_atlas = owner.fonts[2].atlas.unwrap();
        assert!(owner.texture_surface(partial_atlas).is_ok());
        assert!(owner.font(2).is_err());
        assert!(owner.fonts[3].atlas.is_none());
        events.borrow_mut().clear();
        owner.destroy_indicator_assets().unwrap();
        assert_eq!(events.borrow().len(), 13);
        assert_eq!(events.borrow()[2], Event::Release(partial_atlas));
        assert!(owner.texture_surface(partial_atlas).is_err());
    }

    #[test]
    fn indicator_assets_release_error_preserves_prefix_without_replay() {
        use IndicatorTexture as Slot;

        let (mut owner, events, release_failure) = indicator_owner();
        let mut metrics = metrics_source(2.0, &events);
        owner.create_indicator_assets(&mut metrics).unwrap();
        let mut prefix: Vec<_> = (0..4)
            .map(|font| owner.font(font).unwrap().atlas.unwrap())
            .collect();
        let instrument = owner.indicator_textures[Slot::Instrument as usize].unwrap();
        let numbers = owner.indicator_textures[Slot::Numbers as usize].unwrap();
        let textura = owner.indicator_textures[Slot::Textura as usize].unwrap();
        prefix.extend([instrument, numbers]);
        *release_failure.borrow_mut() = Some(numbers);
        events.borrow_mut().clear();
        assert!(owner
            .destroy_indicator_assets()
            .unwrap_err()
            .contains("Numbers Release: injected host failure after actual release"));
        assert_eq!(owner.indicator_textures[Slot::Instrument as usize], None);
        assert_eq!(
            owner.indicator_textures[Slot::Numbers as usize],
            Some(numbers)
        );
        assert!(prefix
            .iter()
            .all(|key| owner.texture_surface(*key).is_err()));
        assert!(owner.texture_surface(textura).is_ok());
        assert!(owner
            .destroy_indicator_assets()
            .unwrap_err()
            .contains("no replay"));
        assert!(owner.create_indicator_assets(&mut metrics).is_err());
        assert!(owner
            .load_font(MenuFontLanguage::Primary, 0, &mut metrics)
            .is_err());
        assert!(owner.release_font_texture(0).is_err());
        assert!(owner.release_font_textures().is_err());
        assert_eq!(
            *events.borrow(),
            prefix.into_iter().map(Event::Release).collect::<Vec<_>>()
        );
    }

    #[test]
    fn indicators_load_save_stops_at_reset_values_after_loading_assets() {
        use crate::game_loadsave_lifecycle::LoadSaveEffects;
        use crate::indicators::{
            IndicatorsEffectError, IndicatorsLoadSave, IndicatorsState, TimerClock,
        };

        let (mut owner, events, _) = indicator_owner();
        let mut state = IndicatorsState::constructed(TimerClock {
            hour: 0,
            minute: 0,
            second: 0,
        });
        let mut metrics = metrics_source(2.0, &events);
        let mut effects = IndicatorsLoadSave::new(&mut state).with_assets(&mut owner, &mut metrics);
        assert_eq!(
            effects.create_indicators(),
            Err(IndicatorsEffectError::Unavailable(
                "complete Indicators::ResetValues"
            ))
        );
        assert_eq!(events.borrow().len(), 18);
        effects.destroy_indicators().unwrap();
        assert_eq!(events.borrow().len(), 32);
    }

    #[test]
    fn raw_text_preserves_non_utf8_empty_strings_and_shared_arcs() {
        let database = MenuTextDatabase::from_bytes(&text_image(&[
            (10, b"\x8a\xff"),
            (20, b""),
            (30, b"tail"),
        ]))
        .unwrap();
        let first = database.get_text(10).unwrap();
        let repeated = database.get_text(10).unwrap();
        assert_eq!(&*first, b"\x8a\xff");
        assert!(Arc::ptr_eq(&first, &repeated));
        assert!(database.get_text(20).unwrap().is_empty());
        assert_eq!(database.unknown(), Some(17));
        assert_eq!(database.len(), 3);
        assert!(database.is_loaded());
    }

    #[test]
    fn raw_text_uses_native_strict_probe_and_rejects_out_of_table_domain() {
        let database = MenuTextDatabase::from_bytes(&text_image(&[
            (10, b"first"),
            (20, b"second"),
            (30, b"strict probe"),
        ]))
        .unwrap();
        assert_eq!(&*database.get_text(20).unwrap(), b"second");
        assert_eq!(&*database.get_text(30).unwrap(), b" ");
        assert_eq!(&*database.get_text(999).unwrap(), b" ");
        for count in [1, 2, 4, 8] {
            let records = vec![(10, &b"text"[..]); count];
            let database = MenuTextDatabase::from_bytes(&text_image(&records)).unwrap();
            assert!(database
                .get_text(10)
                .unwrap_err()
                .contains("native table probe"));
        }
        assert_eq!(&*MenuTextDatabase::new().get_text(999).unwrap(), b" ");
    }

    #[test]
    fn reload_failure_unloads_database_without_invalidating_issued_arc() {
        let mut database = MenuTextDatabase::from_bytes(&text_image(&[
            (10, b"kept"),
            (20, b"second"),
            (30, b"third"),
        ]))
        .unwrap();
        let issued = database.get_text(10).unwrap();
        assert!(database.load_from_bytes(b"bad").is_err());
        assert!(!database.is_loaded());
        assert!(database.is_empty());
        assert_eq!(database.unknown(), None);
        assert_eq!(&*issued, b"kept");
        assert_eq!(&*database.get_text(10).unwrap(), b" ");
    }

    #[test]
    fn zero_component_id_bypasses_database_instead_of_creating_empty_text() {
        let database = MenuTextDatabase::from_bytes(&text_image(&[(0, b"entry")])).unwrap();
        let mut owner = MenuDrawResources::new(database, factory(Ok(texture_image())));
        let components: &mut dyn MenuComponentResources = &mut owner;
        assert!(components.text(0).unwrap().is_none());
        assert!(components.text(10).is_err());
        owner.text_database_mut().reset();
        assert_eq!(&*owner.text(10).unwrap().unwrap(), b" ");
    }

    #[test]
    fn cpu_factory_uses_alpha_null_secondary_cache_and_final_release() {
        let mut factory = factory(Ok(texture_image()));
        let key = factory.create_alpha_texture(b"color.tga").unwrap().unwrap();
        let cached = factory.create_alpha_texture(b"COLOR.TGA").unwrap().unwrap();
        assert_eq!(key, cached);
        let texture = factory.texture_resources().texture(key).unwrap();
        assert_eq!(texture.references, 2);
        assert_eq!(texture.flags, MENU_ALPHA_TEXTURE_FLAGS);
        assert_eq!(texture.creation_flags, MENU_ALPHA_TEXTURE_FLAGS);
        let strings = factory.texture_resources().strings();
        let alpha_rep_key = strings
            .string(texture.alpha_file_name)
            .unwrap()
            .rep
            .unwrap();
        let alpha_rep = strings.rep(alpha_rep_key).unwrap();
        assert_eq!(alpha_rep.bytes.as_slice(), b"");
        assert_eq!(alpha_rep.size, 0);
        assert_eq!(
            (alpha_rep.references, alpha_rep.external_references),
            (1, 0)
        );
        assert_eq!(texture.alpha_name, crate::itexture::Observed::Value(None));
        assert_eq!(factory.texture_storage().live_strings(), 2);
        assert_eq!(factory.source.reads, vec![b"Maps\\color.tga".to_vec()]);
        let surface = factory.texture_surface(key).unwrap();
        assert_eq!((surface.width, surface.height, surface.format), (2, 2, 21));
        assert_eq!(surface.bgra, texture_image()[18..]);
        factory.release_texture(key).unwrap();
        assert!(factory.texture_surface(cached).is_ok());
        assert_eq!(
            factory
                .texture_resources()
                .texture(cached)
                .unwrap()
                .references,
            1
        );
        assert_eq!(factory.texture_storage().live_strings(), 2);
        factory.release_texture(cached).unwrap();
        assert!(factory.texture_surface(key).is_err());
        assert!(factory
            .texture_resources()
            .strings()
            .rep(alpha_rep_key)
            .is_err());
        assert!(factory.texture_resources().cached().is_empty());
        assert_eq!(factory.texture_storage().live_graphics(), 0);
        assert_eq!(factory.texture_storage().live_strings(), 0);
        assert_eq!(factory.texture_storage().live_pixels(), 0);
    }

    #[test]
    fn factory_keeps_native_creation_failure_distinct_from_host_error() {
        for failure in [
            TextureImageError::Missing,
            TextureImageError::UnknownFormat,
            TextureImageError::Corrupt,
        ] {
            let mut factory = factory(Err(failure));
            assert!(factory
                .create_alpha_texture(b"missing.tga")
                .unwrap()
                .is_none());
            assert!(factory.texture_resources().cached().is_empty());
            assert_eq!(factory.texture_storage().live_graphics(), 0);
            assert_eq!(factory.texture_storage().live_strings(), 0);
        }
        for failure in [
            TextureImageError::Unsupported,
            TextureImageError::Backend("test I/O".into()),
        ] {
            assert!(factory(Err(failure))
                .create_alpha_texture(b"color.tga")
                .is_err());
        }
        assert!(factory(Ok(texture_image()))
            .create_alpha_texture(b"\xff.tga")
            .is_err());
        for byte_length in [17, 18, 33] {
            let mut corrupt = texture_image();
            corrupt.truncate(byte_length);
            let mut factory = factory(Ok(corrupt));
            assert!(factory
                .create_alpha_texture(b"corrupt.tga")
                .unwrap()
                .is_none());
            assert_eq!(factory.source.reads, vec![b"Maps\\corrupt.tga".to_vec()]);
            assert!(!factory.texture_resources().is_poisoned());
            assert!(factory.texture_resources().cached().is_empty());
            assert_eq!(factory.texture_storage().live_graphics(), 0);
            assert_eq!(factory.texture_storage().live_strings(), 0);
            assert_eq!(factory.texture_storage().live_pixels(), 0);
        }
    }

    #[test]
    fn cpu_factory_keeps_unsupported_resize_as_host_error() {
        let mut non_power_of_two = texture_image();
        non_power_of_two[12] = 3;
        non_power_of_two.extend_from_slice(&[15, 14, 13, 85, 18, 17, 16, 102]);
        for bytes in [vec![0; 18], non_power_of_two] {
            let mut factory = factory(Ok(bytes));
            assert_eq!(
                factory.create_alpha_texture(b"resize.tga").unwrap_err(),
                "menu CreateITexture: Unsupported(\"image resize\")"
            );
            assert_eq!(factory.source.reads, vec![b"Maps\\resize.tga".to_vec()]);
            assert!(!factory.texture_resources().is_poisoned());
            assert!(factory.texture_resources().cached().is_empty());
            assert_eq!(factory.texture_storage().live_graphics(), 0);
            assert_eq!(factory.texture_storage().live_strings(), 0);
            assert_eq!(factory.texture_storage().live_pixels(), 0);
        }
    }

    #[test]
    fn foreign_keys_are_rejected_without_touching_the_real_reference() {
        let mut first = factory(Ok(texture_image()));
        let mut other = factory(Ok(texture_image()));
        let own = first.create_alpha_texture(b"color.tga").unwrap().unwrap();
        let foreign = other.create_alpha_texture(b"color.tga").unwrap().unwrap();
        assert_ne!(own.arena(), foreign.arena());
        assert!(first.texture_surface(foreign).is_err());
        assert!(first.release_texture(foreign).is_err());
        assert_eq!(
            first.texture_resources().texture(own).unwrap().references,
            1
        );
        first.release_texture(own).unwrap();
        other.release_texture(foreign).unwrap();
    }

    #[test]
    fn session_intake_moves_the_existing_factory_and_keeps_same_keys() {
        let mut original = factory(Ok(texture_image()));
        let key = original
            .create_alpha_texture(b"color.tga")
            .unwrap()
            .unwrap();
        let CpuMenuTextureFactory {
            textures,
            storage,
            source,
        } = original;
        let mut owner =
            MenuDrawResources::from_cpu_factory(MenuTextDatabase::new(), textures, storage, source)
                .unwrap();
        let cached = owner.create_alpha_texture(b"COLOR.TGA").unwrap().unwrap();
        assert_eq!(cached, key);
        assert_eq!(
            owner.texture_surface(key).unwrap().bgra,
            texture_image()[18..]
        );
        owner.release_texture(key).unwrap();
        owner.release_texture(cached).unwrap();
        assert!(owner.texture_surface(key).is_err());
    }

    #[test]
    fn constructor_does_not_invent_language_fonts_or_native_load_work() {
        let (owner, events, _) = logged_owner();
        assert!(events.borrow().is_empty());
        assert!(!owner.text_database().is_loaded());
        assert!(owner.language(MenuFontLanguage::Primary).is_err());
        assert!(owner.language(MenuFontLanguage::Credits).is_err());
        for font in 0..4 {
            assert!(owner.font(font).is_err());
        }
        assert!(owner.font(4).is_err());
    }

    #[test]
    fn load_font_releases_creates_reads_then_binds_selected_language() {
        let (mut owner, events, _) = logged_owner();
        owner
            .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
            .unwrap();
        owner
            .install_language(MenuFontLanguage::Credits, &language_image(b"_cz", 22))
            .unwrap();
        let mut metrics = metrics_source(2.0, &events);
        let outcome = owner
            .load_font(MenuFontLanguage::Primary, 0, &mut metrics)
            .unwrap();
        assert_eq!(
            outcome,
            MenuFontLoadOutcome {
                atlas_created: true,
                metrics_read: true,
                metrics_available: true,
            }
        );
        let old = owner.font(0).unwrap().atlas.unwrap();
        assert_eq!(
            events.borrow().as_slice(),
            &[
                Event::Create(b"font0_en.tga".to_vec()),
                Event::Read(b"tables\\font0_en.gmf".to_vec()),
            ]
        );
        events.borrow_mut().clear();
        owner
            .load_font(MenuFontLanguage::Credits, 0, &mut metrics)
            .unwrap();
        assert_eq!(
            events.borrow().as_slice(),
            &[
                Event::Release(old),
                Event::Create(b"font0_cz.tga".to_vec()),
                Event::Read(b"tables\\font0_cz.gmf".to_vec()),
            ]
        );
        assert!(owner.texture_surface(old).is_err());
        let font = owner.font(0).unwrap();
        assert_eq!(font.metrics.height_scale, 2.0);
        assert_eq!(font.glyph_map[usize::from(b'A')], 22);
        assert!(std::ptr::eq(
            font.glyph_map,
            &owner.language(MenuFontLanguage::Credits).unwrap().glyph_map
        ));
        owner.release_font_textures().unwrap();
    }

    #[test]
    fn replacing_language_changes_live_translation_without_reloading_atlas() {
        let (mut owner, events, _) = logged_owner();
        owner
            .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
            .unwrap();
        owner
            .load_font(
                MenuFontLanguage::Primary,
                2,
                &mut metrics_source(2.0, &events),
            )
            .unwrap();
        let atlas = owner.font(2).unwrap().atlas;
        events.borrow_mut().clear();
        owner
            .install_language(MenuFontLanguage::Primary, &language_image(b"_new", 77))
            .unwrap();
        assert_eq!(owner.font(2).unwrap().glyph_map[usize::from(b'A')], 77);
        assert_eq!(owner.font(2).unwrap().atlas, atlas);
        assert!(events.borrow().is_empty());
        owner.release_font_textures().unwrap();
    }

    #[test]
    fn failed_native_metrics_open_retains_previous_metrics_and_binds_language() {
        let (mut owner, events, _) = logged_owner();
        owner
            .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
            .unwrap();
        owner
            .install_language(MenuFontLanguage::Credits, &language_image(b"_cz", 22))
            .unwrap();
        owner
            .load_font(
                MenuFontLanguage::Primary,
                1,
                &mut metrics_source(2.0, &events),
            )
            .unwrap();
        let mut missing = Metrics {
            result: Ok(None),
            events: Rc::clone(&events),
        };
        let outcome = owner
            .load_font(MenuFontLanguage::Credits, 1, &mut missing)
            .unwrap();
        assert!(!outcome.metrics_read);
        assert!(outcome.metrics_available);
        assert_eq!(owner.font(1).unwrap().metrics.height_scale, 2.0);
        assert_eq!(owner.font(1).unwrap().glyph_map[usize::from(b'A')], 22);
        let outcome = owner
            .load_font(MenuFontLanguage::Primary, 3, &mut missing)
            .unwrap();
        assert!(!outcome.metrics_available);
        assert!(owner.font(3).is_err());
        owner.release_font_textures().unwrap();
    }

    #[test]
    fn actual_atlas_creation_failure_keeps_metrics_and_null_atlas_distinct() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut owner = MenuDrawResources::new(
            MenuTextDatabase::new(),
            factory(Err(TextureImageError::Missing)),
        );
        owner
            .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
            .unwrap();
        let outcome = owner
            .load_font(
                MenuFontLanguage::Primary,
                0,
                &mut metrics_source(2.0, &events),
            )
            .unwrap();
        assert!(!outcome.atlas_created);
        assert!(outcome.metrics_read);
        assert!(outcome.metrics_available);
        assert!(owner.font(0).unwrap().atlas.is_none());
        assert_eq!(owner.font(0).unwrap().metrics.height_scale, 2.0);
        owner.release_font_textures().unwrap();
    }

    #[test]
    fn metrics_host_failure_retains_new_atlas_old_metrics_and_unreached_binding() {
        for failure in [Err("host read failure".into()), Ok(Some(vec![0; 12]))] {
            let (mut owner, events, _) = logged_owner();
            owner
                .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
                .unwrap();
            owner
                .install_language(MenuFontLanguage::Credits, &language_image(b"_cz", 22))
                .unwrap();
            owner
                .load_font(
                    MenuFontLanguage::Primary,
                    0,
                    &mut metrics_source(2.0, &events),
                )
                .unwrap();
            let old = owner.font(0).unwrap().atlas.unwrap();
            events.borrow_mut().clear();
            let mut source = Metrics {
                result: failure,
                events: Rc::clone(&events),
            };
            assert!(owner
                .load_font(MenuFontLanguage::Credits, 0, &mut source)
                .is_err());
            let slot = &owner.fonts[0];
            let new = slot.atlas.unwrap();
            assert_ne!(old, new);
            assert!(owner.texture_surface(old).is_err());
            assert!(owner.texture_surface(new).is_ok());
            assert_eq!(slot.metrics.as_ref().unwrap().height_scale, 2.0);
            assert_eq!(slot.language, Some(MenuFontLanguage::Primary));
            assert!(slot.load_error.is_some());
            assert!(owner.font(0).is_err());
            assert_eq!(
                events.borrow().as_slice(),
                &[
                    Event::Release(old),
                    Event::Create(b"font0_cz.tga".to_vec()),
                    Event::Read(b"tables\\font0_cz.gmf".to_vec()),
                ]
            );
            owner.release_font_textures().unwrap();
        }
    }

    #[test]
    fn missing_language_or_unterminated_suffix_fails_after_old_atlas_release() {
        for install_unterminated in [false, true] {
            let (mut owner, events, _) = logged_owner();
            owner
                .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
                .unwrap();
            owner
                .load_font(
                    MenuFontLanguage::Primary,
                    0,
                    &mut metrics_source(2.0, &events),
                )
                .unwrap();
            let old = owner.font(0).unwrap().atlas.unwrap();
            if install_unterminated {
                owner
                    .install_language(
                        MenuFontLanguage::Credits,
                        &language_image(b"abcdefghijklmnop", 22),
                    )
                    .unwrap();
            }
            events.borrow_mut().clear();
            assert!(owner
                .load_font(
                    MenuFontLanguage::Credits,
                    0,
                    &mut metrics_source(2.0, &events)
                )
                .is_err());
            assert_eq!(events.borrow().as_slice(), &[Event::Release(old)]);
            assert!(owner.fonts[0].atlas.is_none());
            assert!(owner.fonts[0].load_error.is_some());
            assert!(owner.texture_surface(old).is_err());
            assert!(owner.font(0).is_err());
        }
    }

    #[test]
    fn uncertain_release_keeps_key_and_blocks_draw_reload_and_replay() {
        let (mut owner, events, release_failure) = logged_owner();
        owner
            .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
            .unwrap();
        owner
            .load_font(
                MenuFontLanguage::Primary,
                0,
                &mut metrics_source(2.0, &events),
            )
            .unwrap();
        let key = owner.font(0).unwrap().atlas.unwrap();
        *release_failure.borrow_mut() = Some(key);
        events.borrow_mut().clear();
        assert!(owner.release_font_texture(0).is_err());
        assert_eq!(owner.fonts[0].atlas, Some(key));
        assert!(owner.fonts[0].release_error.is_some());
        assert!(owner.texture_surface(key).is_err());
        assert!(owner.font(0).is_err());
        assert!(owner
            .release_font_texture(0)
            .unwrap_err()
            .contains("no replay"));
        assert!(owner
            .load_font(
                MenuFontLanguage::Primary,
                0,
                &mut metrics_source(2.0, &events)
            )
            .is_err());
        assert!(owner.release_font_textures().is_err());
        assert_eq!(events.borrow().as_slice(), &[Event::Release(key)]);
    }

    #[test]
    fn successful_font_cleanup_releases_in_native_slot_order() {
        let (mut owner, events, _) = logged_owner();
        owner
            .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
            .unwrap();
        let mut keys = Vec::new();
        for font in 0..4 {
            owner
                .load_font(
                    MenuFontLanguage::Primary,
                    font,
                    &mut metrics_source(2.0, &events),
                )
                .unwrap();
            keys.push(owner.font(font).unwrap().atlas.unwrap());
        }
        events.borrow_mut().clear();
        owner.release_font_textures().unwrap();
        assert_eq!(
            events.borrow().as_slice(),
            keys.iter()
                .copied()
                .map(Event::Release)
                .collect::<Vec<_>>()
                .as_slice()
        );
        for key in keys {
            assert!(owner.texture_surface(key).is_err());
        }
        assert!(owner.fonts.iter().all(|slot| slot.atlas.is_none()));
        events.borrow_mut().clear();
        owner.release_font_textures().unwrap();
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn cleanup_stops_at_uncertain_release_without_touching_later_slots() {
        let (mut owner, events, release_failure) = logged_owner();
        owner
            .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
            .unwrap();
        let mut keys = Vec::new();
        for font in 0..4 {
            owner
                .load_font(
                    MenuFontLanguage::Primary,
                    font,
                    &mut metrics_source(2.0, &events),
                )
                .unwrap();
            keys.push(owner.font(font).unwrap().atlas.unwrap());
        }
        *release_failure.borrow_mut() = Some(keys[1]);
        events.borrow_mut().clear();
        assert!(owner.release_font_textures().is_err());
        assert!(owner.fonts[0].atlas.is_none());
        assert_eq!(owner.fonts[1].atlas, Some(keys[1]));
        assert_eq!(owner.fonts[2].atlas, Some(keys[2]));
        assert_eq!(owner.fonts[3].atlas, Some(keys[3]));
        assert!(owner.texture_surface(keys[2]).is_ok());
        assert!(owner.texture_surface(keys[3]).is_ok());
        assert!(owner.release_font_textures().is_err());
        assert_eq!(
            events.borrow().as_slice(),
            &[Event::Release(keys[0]), Event::Release(keys[1])]
        );
    }

    #[test]
    fn cpu_factory_context_borrows_its_existing_owner_pair() {
        let factory = factory(Ok(texture_image()));
        let backend: &dyn MenuTextureBackend = &factory;
        let (textures, storage) = backend.cpu_texture_context().unwrap();
        assert!(std::ptr::eq(textures, factory.texture_resources()));
        assert!(std::ptr::eq(storage, factory.texture_storage()));
    }

    #[test]
    fn shared_cpu_context_mutations_feed_the_same_factory_cache_and_draw_surface() {
        let mut owner =
            MenuDrawResources::new(MenuTextDatabase::new(), factory(Ok(texture_image())));
        let mut scene_images = Images {
            result: Ok(texture_image()),
            reads: Vec::new(),
        };
        let scene_key = {
            let (textures, storage) = owner.cpu_texture_context_mut().unwrap();
            textures
                .create_texture_from_file(
                    Some(b"shared.tga"),
                    None,
                    MENU_ALPHA_TEXTURE_FLAGS,
                    &mut scene_images,
                    storage,
                )
                .unwrap()
        };
        let menu_key = owner.create_alpha_texture(b"SHARED.TGA").unwrap().unwrap();
        assert_eq!(scene_key, menu_key);
        let (textures, storage) = owner.cpu_texture_context().unwrap();
        let texture = textures.texture(scene_key).unwrap();
        assert_eq!(texture.references, 2);
        let graphics = texture.graphics.unwrap();
        assert!(std::ptr::eq(
            storage.surface(graphics).unwrap(),
            owner.texture_surface(menu_key).unwrap(),
        ));
        assert_eq!(scene_images.reads, vec![b"Maps\\shared.tga".to_vec()]);
        {
            let (textures, storage) = owner.cpu_texture_context_mut().unwrap();
            textures.release_texture(scene_key, storage).unwrap();
        }
        assert!(owner.texture_surface(menu_key).is_ok());
        assert_eq!(
            owner
                .cpu_texture_context()
                .unwrap()
                .0
                .texture(menu_key)
                .unwrap()
                .references,
            1,
        );
        owner.release_texture(menu_key).unwrap();
        let (textures, storage) = owner.cpu_texture_context().unwrap();
        assert!(textures.cached().is_empty());
        assert_eq!(storage.live_graphics(), 0);
        assert_eq!(storage.live_strings(), 0);
        assert_eq!(storage.live_pixels(), 0);
        assert!(owner.texture_surface(scene_key).is_err());
    }

    #[test]
    fn unexposed_backend_cpu_context_remains_absent_without_side_effects() {
        let (mut owner, events, _) = logged_owner();
        assert!(owner.cpu_texture_context().is_none());
        assert!(owner.cpu_texture_context_mut().is_none());
        assert!(events.borrow().is_empty());
        assert!(!owner.text_database().is_loaded());
        assert!(owner.font(0).is_err());
    }

    #[test]
    fn debug_reads_owner_status_without_replaying_interrupted_release() {
        let (mut owner, events, release_failure) = logged_owner();
        owner
            .install_language(MenuFontLanguage::Primary, &language_image(b"_en", 11))
            .unwrap();
        owner
            .load_font(
                MenuFontLanguage::Primary,
                1,
                &mut metrics_source(2.0, &events),
            )
            .unwrap();
        let key = owner.font(1).unwrap().atlas.unwrap();
        *release_failure.borrow_mut() = Some(key);
        assert!(owner.release_font_texture(1).is_err());
        events.borrow_mut().clear();
        let diagnostic = format!("{owner:?}");
        assert!(diagnostic.starts_with("MenuDrawResources {"));
        assert!(diagnostic.contains("text_database_loaded: false"));
        assert!(diagnostic.contains("languages_installed: [true, false]"));
        assert!(diagnostic.contains("font_release_interrupted: [false, true, false, false]"));
        assert!(events.borrow().is_empty());
        assert_eq!(owner.fonts[1].atlas, Some(key));
        assert!(owner.font(1).is_err());
    }

    #[test]
    fn cpu_intake_rejects_surface_storage_from_another_owner() {
        let mut original = factory(Ok(texture_image()));
        original
            .create_alpha_texture(b"color.tga")
            .unwrap()
            .unwrap();
        let CpuMenuTextureFactory {
            textures,
            storage: _actual_storage,
            source,
        } = original;
        assert!(
            CpuMenuTextureFactory::new(textures, CpuTextureStorage::default(), source).is_err()
        );
    }
}
