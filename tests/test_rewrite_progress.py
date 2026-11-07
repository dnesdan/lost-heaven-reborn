"""Unit tests for scripts/rewrite_progress.py (loads the script by path)."""
import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "scripts" / "rewrite_progress.py"


def load_script():
    spec = importlib.util.spec_from_file_location("rewrite_progress", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class RewriteProgressTests(unittest.TestCase):
    def test_live_overlay_validates_and_counts_combine(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        by_sha = script.ledger_ports_by_binary(ports)
        # Raises on any inconsistency a commit must not introduce.
        script.validate_overlay(overlay, libraries, by_sha)
        cards = script.aggregate(libraries, ports, overlay)
        game = next(card for card in cards if card["name"] == "Game.exe")
        self.assertEqual(game["total"], 4914)
        # Ledger base plus overlay entries, no overlap by construction.
        # +3 VM-adjacent actor/inventory remap (0x7b6d0 CanUseTruck working
        # name differential with REA body; 0x7aed0 S_GameItem ctor +
        # 0x7b020 G_Inventory Clear working-name contract; 0x147220
        # SetFromMission withheld: from_mission offset unevidenced).
        # +4 human/menu/racecam (0x16f400 DuplicateFrom + 0x16f440 SaveData +
        # 0x1bebd0 RacingInMainMenu + 0x1962e0 LoadData, all contract;
        # 0x1ed040 Reset held: #[path]-only module fails the overlay
        # module-membership gate, wrapper rejected by review).
        # +3 detector pair + WinMain chop (0x7b750 GetSignal + 0x7b760
        # SetSignal + 0x166160 WinMain dir-chop prefix, all contract).
        # +1 G_Camera::Reset 0x1ed040 differential confirm-only (2-line
        # lib.rs/mission_game.rs wiring makes the faithful port creditable;
        # fresh 217B REA body + listing corroboration on top of port+tests).
        # +1 C_driver::Tick 0x3c4f0 leading-prefix contract (upstream STUB,
        # 130B slice to the CALL halt, tail/callers deferred).
        # +1 G_Camera::Tick prologue 0x1ed4c0 contract (guards + dt/naklon;
        # Rizin bounds only, Ghidra split deferred). Detector SETSIGNAL/
        # ISSIGNAL arms (27/28) ride ledgered Process 0x6d3b0, no entries.
        # +1 C_human::GameInit prefix slice 0x174bd0 contract (NULL latches,
        # STAND, max->current copy; non-contiguous :3174-3176+:3180).
        # +3 UseCar seat core (0x17e020 Intern_UseCar + 0x17e500 Intern_FromCar
        # + 0x1808c0 Can_EnterToCar guard, all contract) + 3 camera lifecycle
        # (0x1ed400 SetMode + 0x1f2ea0 Begin + 0x1f3450 End, all contract).
        # +2 movement leaves (0x17d2e0 Do_StopAnim + 0x17cb60 TestMoveColl
        # guarded prefix, contract) + 3 police registry (0x09c7e0 AddPerson +
        # 0x09ca50 DelPerson + 0x09b860 SetActive prefix, contract; Ghidra
        # single-session recollection, differential explicitly unwarranted).
        # +1 menu name-select 0x1e0670 contract (20B leaf, LAB_ disclosed).
        # Detector Init/Compile admission + transport: helpers without RVAs,
        # no entries (vtable gate lifted for Init, subroutine still Base-only).
        # +1 swept-capsule static core 0x1d59c0 contract (triple-sphere
        # nearest-hit; slide/dynamic/InVehicle deferred, pos_out hedged).
        # +1 C_car::LoadMaterials 0x1c7f0 contract (pub materials symbol,
        # mafia-re C_car.cpp:60-122 mapping; marker-only, no PC Ghidra body
        # at VA 0x401c7f0 disclosed).
        # +7 Tier-2 marker-only batch, all contract (LoadTyres 0x1d480 scope
        # intake, GetPos 0x7fe0 null/frame branches, SetActState 0x6da0 +0x1c
        # store, clock Init 0x16ae20 / GameInit 0x16ae90 / AI 0x16afe0,
        # racecam dtor 0x196240 shared destroy path; RotateDir(y) deferred:
        # two-level #[path] chain fails the validator gate; clock GameDone
        # deferred: no own state word).
        # +1 C_detector::Process 0x33d10 tick dispatch + INRANGE counting
        # scope (contract; dispatch core in #[path] program_runtime module,
        # filed via actor-side counting owner; coll/use arms loud-refuse).
        # +11 menu/HUD wave, all contract (OnClick 0x5e06e0 / OnChange
        # 0x5e4a50 / OnCreate 0x1e0690 via main_menu module; CreateCompsByDef
        # language probe 0x5ea370 via lhr_assets::menu; indicators ctor
        # 0x5f6b70 / Create 0x5f70e0 / Destroy 0x5f7250 / ResetValues 0x5f6cf0
        # / TimerSetTime 0x5f7500 / GameInit plan 0x5f73c0 / GameDone 0x5f74a0;
        # MenuStack stays behind main.rs #[path], unfileable like RotateDir).
        # +13 dabing/IShow wave, all contract (Dabing Init 0x5f4bc0 / Done
        # 0x5f4c10 / GetFree 0x5f4ca0 / SetHuman 0x5f4cd0 / Reset 0x5f5060 /
        # Speak 0x5f5570 / IsSpeaking 0x5f5c40 / Tick 0x5f5dd0; CreateIShow
        # 0x5b5e90 / OpenIShow 0x5b6110 / LoadCityMusicAreas 0x5adee0 /
        # cbCityMusic 0x5adf30 / CityMusicOff 0x5ae130; exact-RVA Get/Store/
        # Loop/SetVolume skipped: only the 0x5b64c0-0x5b6600 range evidenced).
        # +69 inventory wave (implemented-but-unfiled sweep, all contract):
        # car_ex STUB 0x2f110, Compile 0x6c770, 8 flag lanes 0x7a070-0x7a340,
        # SetSpeed 0x7b4f0, ghost empty/push 0x107b50/0x12f920, DecodeParams
        # 0x11b470, DecodeInstruction 0x11cdd0, mission ctor 0x13f010,
        # load-map 0x14120a, GenerateDebris 0x165060, DynSet 0x17cfe0, rail
        # STUB 0x196710, recorder-close 0x19c6a0, TickSound 0x1a5a64,
        # traffic-visible 0x1a8470, spatial 0x1aca70/0x1acf00, CityMusicPaused
        # 0x1ae1b0, Get/Store/Loop/SetVolume 0x1b64c0-0x1b6600, DelSemaphore
        # 0x1b6d50, DelTelephones 0x1b7440, InVehicle 0x1d6430, Tick click
        # 0x1ea8c0, ExecuteMenu guard 0x1eba40, camera latch 0x1ebe80,
        # 29 indicators lanes, FreerideStoreA 0x20e920, Break_Glass 0x262f0,
        # SetEngineOn 0xcb5b0, glass worker 0xd6670; MenuStack/#[path] scopes
        # deferred as unfileable (RotateDir precedent).
        # +13 MenuStack wave after moving menu.rs under lib.rs as a real
        # module (Create 0x1ea2e0 / Close 0x1eae10 / CloseAll 0x1eae40 / Open
        # 0x1ead50 / Exchange 0x1ead80 / SetDisabled 0x1eaed0 / ActivateByID
        # 0x1eb1c0 / OrNext 0x1eb210 / Find 0x1eb2f0 / Prev/Next id+list
        # 0x1eb320-0x1eb4e0; Tick 0x1ea8c0 already filed via main_menu).
        # +4 visibility wave: RotateDir 0x84d0 via native_transform re-export
        # (itexture pub-use precedent), ghost push 0x52f920 / empty 0x507b50 /
        # DynSet merge scope 0x57cfe0 (HumanState fns promoted to pub).
        # +1 vehicle light discovery 0xc1420 via vehicle_init re-export
        # (model/sound cache methods stay unfileable: associated fns have no
        # re-export path, same class as pre-fix RotateDir).
        # +25 police/traffic/vehicle wave, all contract (TickDoRiciActors
        # 0x49ea80 / Add 0x49e820 / Del 0x49e9e0 / FreeMyTemp 0x49e7e0 / Tick
        # prefix 0x49a300 / Firing 0x49cb20 / Boxing 0x49d030 / ForceArrest
        # 0x49d130; SetVisible 0x44e960 / TickElements 0x44ef70 / IsCarHasOcas
        # 0x44f090 / PlayerInTaxi 0x44f740 / taxi lifecycle 0x44fcf0-0x450860;
        # door SetState 0x439610 / angle 0x43b810 / ChangeState 0x43bcc0 /
        # EnableUsingObject 0x43add0; OpenDoor 0x4cda20 / windows 0x4d7a80+ /
        # colls flags 0x4cdb10+).
        # +5 movement/HUD/detector wave, all contract (Movement dispatch
        # 0x57a710 / MovementIntern core 0x57b660 / InOwner probe 0x57d0e0 /
        # player-HUD step 0x5a0810 / Detector GameInit latch scope 0x432a00).
        # +21 textdb/mission-options/police wave, all contract (TextDB ctor
        # 0x60fa40 / dtor 0x60fa50 / Load 0x60fa60 / Unload 0x60fb10 / GetText
        # 0x60fb40 / GetOrNull 0x60fba0; LOD 0x545b20 / DrawDist 0x545b60 /
        # Effects 0x545ba0 / Sounds 0x545bd0 / Shadows 0x545c00 / GetShadows
        # 0x545c60 / Particles 0x545cb0; run_over 0x49d220 / status 0x49d390 /
        # crash 0x49d3e0 / fire 0x49d420 / angry 0x49d460+0x49d4b0 / offenses
        # 0x49d520 / nevi 0x49d560).
        # +7 actor/projection wave, all contract (width lane 0x204890 /
        # GetDead 0x47afc0 / GetCar 0x47b7b0 / GetSpeed 0x47b0f0 / IsCarUsable
        # 0x429850 / SetBreakMotor 0x47b3e0 / LockControls 0x595f00;
        # console_clear deferred: no observable latch without AddText owner).
        # +22 housekeeping/mission/vehicle wave, all contract (cbEnumSectors
        # 0x5a3b90 / PreloadAnim 0x5a3bd0 / GlobalProgramRun 0x5afd50 /
        # cbFreeFramePlacement 0x5afca0 / InvalidateFrame 0x5b0210 /
        # cbDelFrame 0x5b0230 / 3 stack clears 0x5b1da0-0x5b2590; cbFrameMap
        # 0x5455e0 / AddTimeTransparent 0x5457a0 / Finish 0x5459b0 /
        # UpdateSettings 0x545a30 / RestoreSettings 0x545ad0 / StoreModel
        # 0x546010; SetFuel 0x4cb6e0 / Unstable 0x4cd5b0 / Size 0x4cd5c0 /
        # Pivot 0x4cd5e0 / LockVehicle 0x4cd600 / RecordKey 0x404f30 /
        # cbCollTester 0x587d30).
        # +14 human/registry wave, all contract (Do_Breath 0x57d310 / Go_*
        # 0x57d360-0x57d5a0 / Do_Aimed 0x57f830 / InventoryTruncate 0x57f9a0;
        # Add/DelFrame 0x4081d0+0x4083e0 / anim comparator 0x408690 /
        # GetWheelCamPos 0x4c6010 / AddSector pair 0x4ce560+0x4ce760 /
        # CompareShadows 0x5acfb0; wrappers TestLine* deferred).
        # +5 angle/map/sector wave (AngleTo2 0x408490 signed atan2 diff via
        # animation_angle re-export / RotateDir 0x4084d0 + NoNorm 0x408630 via
        # rotate_y re-export / MapEnable 0x5f9d10 / cbSectorBitFill 0x540ff0
        # via mission_load_session re-export; GlobalProgram 0x540520/50
        # deferred: methods have no re-export path).
        # +4 vehicle materials latch (CloseMaterials 0x4c5c80 / GetHeadPos
        # 0x4cc620 / SetMaterials 0x4c5c10 / SetParticles 0x4cc0).
        # +10 housekeeping/mission slice, all contract (cbFindTaxiPoints
        # 0x5b6e0 / LightnessClear 0x5b5180 / ScoreSet 0x5b9fe0 / ScoreSetOn
        # 0x5f9a10 / InternTempActDel 0x5a79f0 / cbTelephones 0x5b71b0 /
        # cbScreamArea 0x5b9110 / ShowProgress 0x540950 / SceneErrFnc 0x540fd0
        # / GetTShift 0x547120).
        # +3 indexed sound tables (LoadTables 0x55f390 / ConstructSound
        # 0x55f820 / FreeTables 0x55ee00; parse helper has no RVA).
        # +4 rnd/don_ocas/IShowDestroy wave (rnd_mod 0x408450 / rnd_unit
        # 0x408470 / C_don_ocas ctor 0x44c640, dtor 0x44c680 honestly
        # unfiled as no-observable-state / DestroyIShowAll 0x5b63b0).
        # +8 controls/logo wave, all contract (C_controls ctor/dtor/Close/
        # Init 0x4efb40-0x4efde0, SetMouseScale 0x4efdb0, ClearControls
        # 0x4f00f0, SetControl 0x4f02b0, PlayLogo 0x15d460 caller projection).
        # +2 menu/detector scope (SetHidden 0x5eaf20, C_detector::Init mask
        # scope 0x432990; MapEnable 0x5f9d10 already filed).
        # +4 police/traffic STUB dispatch (SendPoliceBackup 0x49eee0 /
        # SendPoliceToHuman 0x49f980 / RemoveAllCars 0x4bad80 / RemoveCar
        # 0x4bbe90; explicit-Unsupported stubs with tests; Do_Crouched
        # 0x17f8a0 + GetAnimIndex 0x8760 already ledger-credited).
        # +6 IShow STUB slice (DestroyIShow 0x5b62c0 / IShowFade 0x5b6710 /
        # FadeDownAll 0x5b6660 / SetFilmMusic 0x5ae1d0 / GetFilmMusic
        # 0x5ae390 / StreamDestroyAll 0x5ae3d0; all explicit-Unsupported).
        # +0 VA-form remediation (no new ports): 161 Game.exe entries used
        # absolute VAs as rvas (my systematic error; base tree had 0).
        # Converted to true base-relative RVAs; dropped 5 ledger duplicates
        # (GetText 0x20fb40, LockControls 0x195f00, GlobalProgramRun 0x1afd50,
        # Do_Breath 0x17d310, MapEnable 0x1f9d10) and 5 within-overlay dupes
        # (InitAttempt working-name, AiGhostStore pair, free-fn merge scope,
        # RotateDir dup). Net honest count now.
        # +8 verified-upgrade tranche B (indicators flag lanes 0x7a070-0x7a340:
        # Ghidra bodies 3-177 insns + per-function pinning tests).
        # +1 racecam dtor 0x196240 (2-insn vtable-store + base tail) + VA-form
        # remediation of 33 Game + 13 LS3DF entries (no count change).
        # +3 driving tables (LoadMaterials 0x1c7f0 / LoadTyres 0x1d480 upgrades
        # + ConvertTyres 0x1d770 new filing; Ghidra bodies 164/240/49 insns).
        # +3 clock walks (Init 0x16ae20 42 insns / GameInit 0x16ae90 80 insns /
        # AI 0x16afe0 94 insns; complete instruction walks + pinning tests).
        # +26 recon batch (detector/human/vehicle/indicators/police/menu/
        # ishow getters + STUBs; Ghidra bodies 2-929 insns + pinning tests).
        # +5 roads helpers (clear 0x15cb90 / zone 0x15be10 / emit 0x15bc10 /
        # fixup 0x15bcd0 / edge 0x15b5d0; Ghidra bodies 20-132 insns).
        # +13 police/traffic/car batch (GameInit/release/remove 0x099cd0-
        # 0x09f780, DecrementCount/Save 0x0b9560-0x0aabf0, GetAIPos/Nav
        # 0x029510/0x0299c0, railway Save 0x086920, roads edge/ctor;
        # 10 dossiers + 3 walks).
        # +19 animation blending contracts (C_animations 0x00e020-0x0103d0;
        # marker-only, tests green; differential after Ghidra bodies).
        # +16 animation differential (all non-STUB; Ghidra bodies 19-132
        # insns + pinning tests; 3 STUBs stay contract).
        # +13 mission actor-list contracts (C_mission/C_game 0x13efb0-0x1a3990;
        # marker-only; add_actor 0x13fea0 + unload 0x140080 + Tick 0x1407e0
        # honestly unfiled as ledger-credited; 17 tests green).
        # +10 ctor/order differential (cache open / mission open-close-uninit /
        # clock-shot-rail-carex-racecam ctors / inventory item; 8-680 insns).
        # +20 traffic/rail contracts (traffic_car 6 / car_ex 4 / rail_generator
        # 4 / railway 3 / turnout 1 / wagon 2; marker-only, tests green).
        self.assertEqual(game["ported"], 149 + 363)
        self.assertEqual(game["verified"], 149 + 92)
        self.assertLessEqual(game["verified"], game["ported"])
        self.assertLessEqual(game["ported"], game["total"])
        ls3df = next(card for card in cards if card["name"] == "LS3DF.dll")
        self.assertEqual(ls3df["total"], 1714)
        # 145 ledger differential + 149 overlay (17 texture: 9 differential +
        # 8 contract; 21 material: 19 differential + 2 contract; 22 mesh:
        # 6 differential + 16 contract; 22 frame: 8 differential +
        # 14 contract; 18 model: 9 differential + 9 contract; 49 scene:
        # 19 differential + 30 contract; 43 sector/camera/light: 27
        # differential + 16 contract; 44 visual/object/frames/target: 19
        # differential + 25 contract; 34 lit+mirror: 14 differential + 20
        # contract (21 lit: 6 differential + 15 contract; 13 mirror: 8
        # differential + 5 contract); 54 shadow/singlemesh/singlemorph: 28
        # differential + 26 contract (9 shadow lifecycle: 7 differential +
        # 2 contract; 8 shadow setters: 2 differential + 6 contract; 12 shadow
        # passes: 10 differential + 2 contract; 14 singlemesh: 6 differential
        # + 8 contract (0x54240 confirm-only const + 0x56860 untranscribed tail
        # stay contract); 11 singlemorph: 3 differential + 8 contract;
        # 61 wave-1 particle/projector/billboard: 28 differential + 33 contract
        # (14 emitor: 8 differential + 6 contract; 13 elements: 6 differential
        # + 7 contract; 12 projector lifecycle: 3 differential + 9 contract,
        # 0x464d0 withheld with no tree symbol, 0x46000 partial port stays
        # contract; 8 projector passes: 5 differential + 3 contract;
        # 14 billboard: 6 differential + 8 contract, 0x5e10 ABI-only teardown
        # and 0x9630 partial dtor stay contract).
        # 78 wave-2 morph/anim/interpolator/pose/area: 37 differential +
        # 41 contract (21 morph: 11 differential + 10 contract; 17 anim:
        # 5 differential + 12 contract; 13 interpolator: 8 differential +
        # 5 contract; 16 pose: 7 differential + 9 contract; 11 area:
        # 6 differential + 5 contract).
        # 98 wave-3 portal/occluder/sound/landscape/collision/load/math/
        # lightmap: 20 differential + 78 contract (9 portal: 7 differential +
        # 2 contract; 10 occluder: 7 differential + 3 contract; 19 sound:
        # 5 differential + 14 contract; 16 landscape, 8 collision, 7 load,
        # 19 math, 10 lightmap: all contract; 13 deferred-scope withholds
        # with reasons in the commit message: 0x603e0 ledger-credited,
        # 9 portal/occluder single-owner-pending, 3 sound getter/callee-folded).
        # 73 wave-4 render/clipping/driver: 3 differential + 70 contract
        # (8 factories + tick-list downgraded per review: native-only FATAL
        # arms / unverified bodies; thunks own forwarder symbols; 0x598d0
        # ledger-credited like F2 0x603e0; 20 render-core, 20 clipping,
        # 12 driver-fileable, 7 driver-draws, 14 driver-newmods; device
        # draws + no-symbol items withheld with reason in the commit
        # message).
        # 94 wave-5 ISound/IGraph: all contract (69 ISound isound_* + 25
        # IGraph incl. ResizeImage/format/RLA/image-info/maps-dirs/keyboard/
        # cpuid/result-message; intra-draft 0x7be20 dup withheld/deduped).
        # 49 wave-6 visual/C-str/remap/getters: 7 differential + 42 contract
        # (29 visual STL+helpers, 5 C-str, 7 remap confirm-only, 2 getters,
        # 6 scalar-dtor shared symbols; 11 math/material + 3 C-str already
        # ledger-credited, no entries).
        # 24 wave-7 early/render/queues/GetName: all contract (20 early
        # math/state, 1 Render scene pass, 2 helper-struct ctors, 1 GetName
        # pointer projection; 21 anim + 24 frames reuse/new RVAs already
        # ledger-credited, no entries).
        # 2 wave-8 occluder pair (0x3c2e0 CullOcclsEachOther + 0x3c480
        # POIS_EnumProc, PDB static names, REA bodies 25BB/411B + 25BB/386B):
        # both differential.
        # 3 wave-9 inline trio (0x2ad00 GetRenderMatrix working name +
        # 0x2e360 Cross original + 0x68840 GetInvMatrix original; REA bodies
        # 5BB/40B + 1BB/57B + 5BB/60B): all differential.
        # 20 wave-10 IShow CPU prefixes (8 logo refcount/flag/rect/region +
        # 4 vorbis dta delegates + 7 stream flag/submit + 1 renderer drop;
        # 0x753e0 construction-covered): all contract.
        # 4 wave-11 IShow stragglers (2 inline getters + dB sub-map +
        # IsStreamPlaying withhold-lift; full SetVolume stays device):
        # all contract.
        # +26 verified-upgrade tranche A (13 vector math + 2 driver +
        # 5 C-str + RLA/maps/keyboard/cpuid/result; 6 render/vb held).", Tier rules applied: differential needs
        # a Ghidra body (or complete instruction walk) plus pinning tests —
        # this includes confirm-only bodies that corroborate a pre-existing
        # port (new evidence + tests on top); contract covers no-body walks,
        # ABI-only teardown bodies (vtable re-store + base tailjump, no
        # portable state effect), and partial-coverage ports. Same standard
        # as 0x2b830 throughout.
        # +15 verified-upgrade tranche C (native_transform math: rotation
        # X/Y/Z 0x2cdc0-0x2ce60, AngleTo 0x2ceb0, rotate product 0x2cf40,
        # Mult4X4 0x2d520, Make4X4 0x2d780, SetDir 0x2e030, GetScale2/Uscale
        # 0x2e510/0x2e580, CosAngleTo 0x2e710, RotateByNorm 0x2eab0, quat
        # Inverse 0x2fd70/RotateByMatrix 0x30040, scale lanes 0x56e60;
        # Ghidra bodies + pinning tests, 1 new test).
        # +8 ISound tranche D (listener ctor/AddRef/Release/SetPos
        # 0x7b060-0x7b100, sound ctor/dtor 0x7c8e0/0x7c960, source
        # GetPlayTime/GetFormat 0x7d8d0/0x7d910; 3 Ghidra bodies + 5
        # complete instruction walks + pinning tests).
        # +10 ISound tranche E (I3D_sound Open/IsPlaying/SetRange/SetCone/
        # SetOutVol/SetVolume/SetLoop/SetCurrTime/SetVelocity/GetCurrTime;
        # complete walks + pinning tests; SetSoundType/SetFrequency held as
        # hardening ODCHYLKA).
        # +7 listener tranche F (SetDir/SetVelocity/Distance/Doppler/Rolloff
        # 0x7b120-0x7b1a0 walks + 3dsound ctor/dtor; pin tests).
        self.assertEqual(ls3df["ported"], 145 + 830)
        self.assertEqual(ls3df["verified"], 145 + 324)

    def test_overlay_rejects_duplicates_and_unknown_symbols(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        by_sha = script.ledger_ports_by_binary(ports)
        bad = json.loads(json.dumps(overlay))
        bad["functions"].append(dict(bad["functions"][0]))
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)
        bad = json.loads(json.dumps(overlay))
        bad["functions"][0]["symbol"] = "lhr_engine::nope::missing_fn"
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)
        bad = json.loads(json.dumps(overlay))
        bad["functions"][0]["rva"] = "not-an-rva"
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)

    def test_rw_data_card_counts_own_functions_without_crt(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        cards = script.aggregate(libraries, ports, overlay)
        rw = next(card for card in cards if card["name"] == "rw_data.dll")
        self.assertEqual(rw["total"], 62)
        # 12 frozen ledger ports + 12 overlay behavior ports (2 write, 8
        # read/leak lane, DeCrypt, FirstForce) + 38 construction-covered
        # (4 XCU thunks, 12 further CRT startup, empty/synthetic dtors,
        # placement-delete no-op, 16 STL templates; contract tier only).
        self.assertEqual(rw["ported"], 24)
        self.assertEqual(rw.get("covered", 0), 38)
        self.assertEqual(rw["ported"] + rw.get("covered", 0), 62)
        self.assertEqual(rw["verified"], 17)
        self.assertLessEqual(rw["verified"], rw["ported"])
        self.assertLessEqual(rw["ported"], rw["total"])

    def test_rw_data_own_set_is_retained_below_first_crt_plus_xcu(self):
        # Evidence list pinned to the frozen ledger and the mafia-re total:
        # own = 58 retained entries below the first CRT address plus the 4
        # XCU startup thunks (0x1000/0x1060/0x10f0/0x1130) proven by the live
        # .CRT$XCU table at 0x1000c000 and raw CALL/JMP decode; they carry no
        # retained pseudocode (Ghidra folds the boundaries).
        script = load_script()
        libraries, _ = script.load_ledger()
        lib = libraries["rw_data.dll"]
        retained = {entry.lower() for entry in lib["retained_entries"]}
        self.assertEqual(len(retained), 176)
        below = {
            rva for rva in retained
            if int(rva, 16) < script.RW_DATA_FIRST_CRT_RVA
        }
        self.assertEqual(len(below), 58)
        xcu = {"0x1000", "0x1060", "0x10f0", "0x1130"}
        self.assertTrue(xcu.isdisjoint(below))
        self.assertEqual(set(script.RW_DATA_OWN_RVAS), below | xcu)
        self.assertEqual(len(script.RW_DATA_OWN_RVAS), 62)
        self.assertEqual(script.RW_DATA_BINJA_ONLY_GAP, 0)
        self.assertEqual(script.RW_DATA_OWN_TOTAL, 62)

    def test_rw_data_ledger_and_overlay_ports_are_all_own(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        sha = libraries["rw_data.dll"]["binary_sha256"]
        ledger = script.ledger_ports_by_binary(ports).get(sha, {})
        self.assertEqual(len(ledger), 12)
        for rva in ledger:
            self.assertIn(rva, script.RW_DATA_OWN_RVAS)
        for entry in overlay["functions"]:
            if entry["binary"] == "rw_data.dll":
                self.assertIn(
                    entry["rva"].lower(), script.RW_DATA_OWN_RVAS
                )
        # Construction-covered entries are all contract tier and exactly 38.
        covered = [
            entry for entry in overlay["functions"]
            if entry.get("construction_covered")
        ]
        self.assertEqual(len(covered), 38)
        for entry in covered:
            self.assertEqual(entry["binary"], "rw_data.dll")
            self.assertEqual(entry["tier"], "contract")

    def test_overlay_rejects_bare_type_behavior_for_rw_data(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        by_sha = script.ledger_ports_by_binary(ports)
        # A bare-type behavior symbol for rw_data must fail validation.
        # (Drop the live 0x1010 entry first so the duplicate tripwire, which
        # runs before the symbol check, does not mask the bare-type verdict.)
        bad = json.loads(json.dumps(overlay))
        bad["functions"] = [
            f for f in bad["functions"]
            if not (f["binary"] == "rw_data.dll" and f["rva"] == "0x1010")
        ]
        bad["functions"].append({
            "binary": "rw_data.dll",
            "rva": "0x1010",
            "va": "0x10001010",
            "name": "test only",
            "symbol": "lhr_assets::rw_data::Handles",
            "tier": "contract",
            "evidence": "test only",
        })
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)
        # ...unless flagged construction-covered (name presence suffices).
        good = json.loads(json.dumps(overlay))
        good["functions"] = [
            f for f in good["functions"]
            if not (f["binary"] == "rw_data.dll" and f["rva"] == "0x1010")
        ]
        good["functions"].append({
            "binary": "rw_data.dll",
            "rva": "0x1010",
            "va": "0x10001010",
            "name": "test only",
            "symbol": "lhr_assets::rw_data::Handles",
            "tier": "contract",
            "evidence": "test only",
            "construction_covered": True,
        })
        script.validate_overlay(good, libraries, by_sha)
        # Construction-covered entries must never claim differential.
        diff = json.loads(json.dumps(good))
        diff["functions"][-1]["tier"] = "differential"
        with self.assertRaises(ValueError):
            script.validate_overlay(diff, libraries, by_sha)

    def test_overlay_rejects_rw_data_crt_and_accepts_lhr_assets(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        by_sha = script.ledger_ports_by_binary(ports)
        # CRT entries must never count: _strcmp is retained but not own.
        bad = json.loads(json.dumps(overlay))
        bad["functions"].append({
            "binary": "rw_data.dll",
            "rva": "0x9510",
            "va": "0x10009510",
            "name": "strcmp",
            "symbol": "lhr_assets::rw_data::Handles",
            "tier": "contract",
            "evidence": "test only",
        })
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)
        # rw_data Rust lives in lhr-assets: an own RVA with a real symbol
        # there validates (de_crypt exists in
        # crates/lhr-assets/src/rw_data.rs). Re-add it on a copy with the
        # live entry removed to avoid the duplicate tripwire.
        good = json.loads(json.dumps(overlay))
        good["functions"] = [
            f for f in good["functions"]
            if not (f["binary"] == "rw_data.dll" and f["rva"] == "0x4690")
        ]
        good["functions"].append({
            "binary": "rw_data.dll",
            "rva": "0x4690",
            "va": "0x10004690",
            "name": "DeCrypt",
            "symbol": "lhr_assets::rw_data::de_crypt",
            "tier": "contract",
            "evidence": "test only",
        })
        script.validate_overlay(good, libraries, by_sha)

    def test_path_submodule_symbols_resolve_through_path_attr_only(self):
        # itexture.rs items living in #[path] siblings (texture_factory.rs /
        # texture_surface.rs / texture_load_image.rs) validate via the
        # #[path]-scoped fallback; wrong-module paths must fail even when a
        # same-named file exists on disk.
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        by_sha = script.ledger_ports_by_binary(ports)
        good = json.loads(json.dumps(overlay))
        # 0x7a380 already overlayed -> expect duplicate, so drop it first.
        good["functions"] = [
            f for f in good["functions"]
            if not (f["binary"] == "LS3DF.dll" and f["rva"] == "0x7a380")
        ]
        good["functions"].append({
            "binary": "LS3DF.dll",
            "rva": "0x7a380",
            "va": "0x1007a380",
            "name": "CompressTexture test",
            "symbol": "lhr_engine::itexture::texture_surface::compress_texture",
            "tier": "contract",
            "evidence": "test only",
        })
        script.validate_overlay(good, libraries, by_sha)
        # #[path] sibling behind itexture validates under its real path.
        path_ok = json.loads(json.dumps(overlay))
        path_ok["functions"] = [
            f for f in path_ok["functions"]
            if not (f["binary"] == "LS3DF.dll" and f["rva"] == "0x78190")
        ]
        path_ok["functions"].append({
            "binary": "LS3DF.dll",
            "rva": "0x78190",
            "va": "0x10078190",
            "name": "presaved test",
            "symbol": "lhr_engine::itexture::presaved_dxt_filename",
            "tier": "contract",
            "evidence": "test only",
        })
        script.validate_overlay(path_ok, libraries, by_sha)
        # Wrong-module path must fail: texture_factory.rs exists on disk but
        # is not a crate module (it lives behind itexture.rs via #[path]).
        bad = json.loads(json.dumps(overlay))
        bad["functions"] = [
            f for f in bad["functions"]
            if not (f["binary"] == "LS3DF.dll" and f["rva"] == "0x78190")
        ]
        bad["functions"].append({
            "binary": "LS3DF.dll",
            "rva": "0x78190",
            "va": "0x10078190",
            "name": "presaved test",
            "symbol": "lhr_engine::texture_factory::presaved_dxt_filename",
            "tier": "contract",
            "evidence": "test only",
        })
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)
        # Nonexistent module must fail too.
        bad["functions"][-1]["symbol"] = "lhr_engine::nonexistent_mod::some_fn"
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)
        # Bogus intermediate segment must fail: load_image lives in
        # texture_load_image.rs, not in bogus_mid.
        bad["functions"][-1]["symbol"] = "lhr_engine::itexture::bogus_mid::load_image"
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)

    def test_svg_render_is_deterministic(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        first = script.aggregate(libraries, ports, overlay)
        second = script.aggregate(libraries, ports, overlay)
        self.assertEqual(
            [script.render_svg(card) for card in first],
            [script.render_svg(card) for card in second],
        )
        svg = script.render_svg(first[0])
        self.assertIn("Implemented", svg)
        self.assertIn("Verified", svg)

    def test_readme_check_passes_after_write(self):
        script = load_script()
        fragment, svgs, _ = script.build()
        self.assertIn("## Rewrite progress", fragment)
        self.assertIn("docs/rewrite/game.svg", fragment)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "README.md").write_text(
                "head\n" + script.START + "\n" + script.END + "\ntail\n"
            )
            text = (root / "README.md").read_text()
            before, rest = text.split(script.START, 1)
            _, after = rest.split(script.END, 1)
            (root / "README.md").write_text(
                before + script.START + "\n" + fragment + "\n" + script.END + after
            )
            reread = (root / "README.md").read_text()
            self.assertIn("## Rewrite progress", reread)
            self.assertTrue(reread.startswith("head\n"))
            self.assertTrue(reread.endswith("tail\n"))


if __name__ == "__main__":
    unittest.main()
