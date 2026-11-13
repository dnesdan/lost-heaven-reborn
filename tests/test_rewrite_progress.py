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
        # 0x5b6e0 / LightnessClear 0x5b5780 (remediated from misfiled 0x5b5180)
        # 0x5b9fa0 (remediated from misfiled 0x5f9a0) / InternTempActDel 0x5a79f0
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
        # +12 traffic/rail differential (5 dossiers + 7 walks; 8 STUBs
        # corovacka/car_ex/rail-gameinit-done-ai stay contract).
        # +13 human/camera/police batch (inventory/duplicate/savedata/menu/
        # winmain/fromcar/enter-guard/setmode/begin/end/stopanim/add-del;
        # Ghidra bodies 2-377 insns).
        # +4 actor/collision/menu batch (GetPos 0x7fe0 / TestPanak2 0x1d59c0 /
        # Intern_UseCar 0x17e020 / OnCreate 0x1e0690; 6 others already
        # differential, no double-credit).
        # +12 indicators/dabing/menu batch (ctor/Create/Destroy/Reset/Timer/
        # GameInit/GameDone 0x1f6b70-0x1f74a0, Dabing Init/Done/FreeCh
        # 0x1f4bc0-0x1f4ca0, OnBeforeCreate/OnChange walks; 10 dossiers +
        # 2 walks; 0x1f9d10 ledger, 5 unported, scopes held).
        # +5 music/speed/ctor batch (SetSpeed 0x7b4f0 / mission ctor 0x13f010 /
        # IsSpeaking 0x1f5c40 / LoadCityMusicAreas 0x1adee0 / CityMusicOff
        # 0x1ae130; bodies 5-59 insns; markers/scopes held).
        # +7 ishow/registry/timer batch (CityMusicPaused 0x1ae1b0 / IShow
        # store-loop-setvol 0x1b6520-0x1b6600 / DelSemaphore-Telephones /
        # TimerSetInterval; bodies 6-67 insns; tick/speak/decode held).
        # +9 indicators setters batch (Tachometr 0x1f86b0 / Ammo-Accuracy-
        # Wingman 0x1f8910-0x1f8970 / Subtitles clears / Compass /
        # WideScreen; bodies 3-147 insns; SetOn deferred disclosed).
        # +10 overlay/radar/menu batch (Fade/Texture/Clear/Edges/Bonus/
        # Diamond/Parhelia 0x1fa410-0x204a80 + Tick/guard; bodies 5-312
        # insns; vertices/draw deferred disclosed).
        # +10 menu/rotate/ghost batch (ActivateByID/FindPrev-Next 0x1eb210-
        # 0x1eb4e0 + RotateDir 0x84d0 + ghost push/empty + mult merge scope +
        # light pairing; bodies 39-825 insns; call plumbing deferred noted).
        # +9 police/traffic batch (TickDoRici/Add/Del/Free/Firing/Boxing/
        # ForceArrest/CrashTime 0x9ea80-0x9d3e0 + SetVisible 0x4e960;
        # bodies 16-249 insns).
        # +4 vehicle/menu/loadsave batch (SetEngineOn 0xcb5b0 / glass worker
        # 0xd6670 / FreerideStoreA 0x20e920 / Menu Create 0x1ea2e0;
        # bodies 11-69 insns).
        # +17 menu/traffic/door/vehicle batch (CloseAll/Open/Exchange/
        # Disabled/Activate 0x1eae40-0x1eb1c0 + IsCarHasOcas/PlayerInTaxi/
        # EnableTaxi/Clear/Free 0x4f090-0x50860 + door SetState/OpenAngle/
        # ChangeState/EnableUsing + OpenWindow/SpecialColls; 16 dossiers +
        # 1 walk; suites green).
        # +6 vehicle/coll/text batch (SetDynamicCollsOn 0xcdcc0 / InOwner
        # probe 0x17d0e0 / TextDatabase ctor/dtor/Load/Unload; 5 dossiers +
        # 1 walk; Movement/Dispatch/Init steps held as partial).
        # +10 options/police batch (SetGameEffects/Sounds/Shadows/GetShadows/
        # SetParticles 0x145ba0-0x145cb0 + NextNaSracky/GetFireTime/Pocet/
        # Nevi 0x9d220-0x9d560; bodies 11-48 insns).
        # +10 options/housekeeping batch (SetLevelOfDetail/DrawDistance /
        # width lane / cbEnumSectors/PreloadAnim / cbFreeFramePlacement /
        # InvalidateFrame/cbDelFrame / Inventory-FloatStackClear; 6-169 insns).
        # +10 scene/vehicle batch (Add-FinishTimeTransparent/Update-Restore
        # Settings/FrmActBuff_StoreModel + SetFuel/Unstable/Size/Pivot/Lock;
        # bodies 4-239 insns).
        # +10 movement/coll batch (RecordKey/cbCollTester walks + Go_Front/
        # Back/Left/Right + Do_Aimed/InventoryTruncate + Add-DelFrameColl;
        # 2 walks + 8 dossiers 32-181 insns).
        # +4 vehicle/score/mission batch (SetVehicleMaterials 0xc5c10 /
        # ScoreSet 0x1b9fe0 / ShowProgress 0x140950 / InternTempActDel
        # 0x1a79f0; bodies 9-173 insns; large callbacks held).
        # +9 text/house/vehicle/compare batch (GetTextOrNull/CharacterStack/
        # WheelCamPos/AddSector x2/AngleTo2/RotateDirNoNorm/compare_anim/
        # CompareShadows; 2 walks + 7 dossiers; SEH cbFrameMap held).
        # +9 controls/logo/head batch (ctor/dtor/Close/Init/MouseScale/Clear/
        # SetControl 0xefb40-0xf02b0 + PlayLogo + GetHeadPos; bodies 2-279
        # insns; SetParticles data-area held).
        # +7 load/session batch (cbSectorBitFill/CloseVehicleMaterials/
        # GetTShift/LoadTables/ConstructSound + Break_Glass/SceneErrFnc
        # walks; 5 dossiers + 2 walks; SEH cbFrameMap held).
        # +10 mission actor-list batch (c_str_less/Add-Del-ClearAnimModel/
        # DelActor(s)/DelNotMe/IsOwn/FindByName/ChangeActState; bodies
        # 42-186 insns; suites green).
        # +10 misc batch (FreeTables/rnd x2/DonOcas/DestroyIShowAll/SetHidden/
        # detector Init walk/UpdateControls/ShowProgress/PrepareBaseBlocks;
        # 9 dossiers + 1 walk; STUBs held).
        # +15 actor/human/player contracts (GetAlive/Rendered/Subtype/Frame/
        # Unknown65/Release/SetFromMission/GlobalActors + SetLookFrame/Wingman/
        # FireTarget/NoAnimHit/IdleAnims/ShutUp + SetCanCarDamage;
        # marker-only, tests green; differential after Ghidra bodies).
        # +14 actor/human/player differential (latches/stores 2-5 insns +
        # Release walk; SetFromMission 426-insn partial held as contract).
        # +15 vehicle small-accessor contracts (lights/projectors/wheels +
        # tank/must-steal/destroy/show/unbreakable/record 0x7b1f0-0x7b4a0;
        # subagent VA-form fixed before filing; marker-only, tests green).
        # +15 vehicle small-accessor differential (Ghidra bodies 2-17 insns;
        # pin tests vehicle_small 15 pass).
        # +13 human/player action contracts (Go diagonals/Force/Turn/Corner/
        # Shock/DeadBodyDrop + NaviPoint/FadeReset; Do_Crouched ledger-held).
        # +13 human/player differential (Ghidra bodies 2-147 insns; suites
        # human_actor 32 / player_actor 4 pass).
        # +14 vehicle accessor contracts (SpecialDynamicColls/Velocity/AngVel/
        # ModelName/Lock/Temporary/ShowEnergy/Unknown/Axis/DynOff + EngineOn/
        # AutoGearing/Horn/Siren; VA-form ok; marker-only, tests green).
        # +14 vehicle accessor differential (Ghidra bodies 2-56 insns; pin
        # tests vehicle_small 29 pass).
        # +3 cutscene flag contracts (SetCutsceneRecord/Play/UpdateFlags
        # 0x1c27b0-0x1c27f0; bytes verified in image; tests green; 11
        # C_effects RVAs withheld (0x44xxxx outside our image)).
        # +15 housekeeping contracts (GetPlayer/ClearDiff/GameTime/InitFrames/
        # Timeout/Fire/Horn/WFall/MissionEnd/Race6/MissionPassed + Telephone/
        # Pump/Sebrali STUBs; VA-form ok; marker-only, tests green).
        # +13 housekeeping differential (Ghidra bodies 2-32 insns; pin tests
        # housekeeping 35 pass; 2 STUBs held as contract).
        # +15 loadsave contracts (ReplaySet/GetNext/PutNext + RaceCars x4 +
        # Objectives x4 + Garage x4; VA-form ok; marker-only, tests green).
        # +15 loadsave differential (Ghidra bodies 2-172 insns; pin tests
        # game_loadsave 24 pass).
        # +3 cutscene flag differential (complete both-branch walks: record
        # 0x8/0x2 + play 0x4/0x1 rotates; pin tests cutscene 19 pass).
        # +15 police/traffic contracts (GetActive + 8 sledovacka + SetGlobal/
        # PlayerHoMa/TryGoTo/GenPassenger/AddFree/Vyfun; VA-form fixed;
        # marker-only, tests green).
        # +3 police/traffic differential (GetActive/SetGlobal/ActionStatus
        # no-chase; Ghidra bodies 2-38 insns; 12 STUBs held as contract).
        # +12 human flag/action contracts (IsReady/AnimIDForDeath/HitFilter/
        # ResetActions/CanAimed/CarClaimant/CornerFire(x2)/BoundPoints/
        # Castrate/HardHead/ShockStart; marker-only, tests green).
        # +11 human differential (Ghidra bodies 2-127 insns; pin tests
        # human_actor 44 pass; bound-points loop held as contract).
        # +1 detector GameInit (complete walk with disclosed base-call/coll
        # deferrals; owned latch-nulling slice + pin test).
        # +16 menu contracts (Exit/Options/FreeRide/LoadGame/GameFailed/
        # QuestionBox/Component/Stack text-id; marker-only, tests green).
        # +16 menu differential (14 complete walks + 2 Ghidra dossiers;
        # pin tests menu 45 / main_menu 25).
        # +15 garage/loadsave contracts (RoadBlocks/Taxi/Scream/SignalMark/
        # Hydrants/Talk x3 STUBs + Garage x4 + Steal/IsCarIn; marker-only).
        # +11 garage/loadsave differential (Ghidra bodies 2-290 insns; pin
        # tests housekeeping 45 / loadsave 30; 4 STUBs held as contract).
        # +15 human colls/shock contracts (Activate/Deactivate/ShockDown/
        # DeadAnim/ResetWeapon/SpecialStroke/NewScream/CutsceneLink/
        # GetActorSpeed/PoseValidate/DropOut x2/Sounds/FF x2; marker-only).
        # +8 human colls/shock differential (Ghidra dossiers 31-311 insns; pin
        # tests human_actor 59 pass; 6 STUBs + GetActorSpeed held as contract).
        # +6 menu definition differential (MessageBox/LoadProgress/GameFailed/
        # GunShop/GarageSummary/OptionsControls-OnClose; 2-insn leaves +
        # small dispatches with complete walks; pin tests menu/main_menu).
        # +10 dabing/ishow/spatial/collision batch (SetHuman/Reset/Speak/Tick +
        # CreateIShow/OpenIShow/cbCityMusic + spatial publish/StopAll +
        # TestPanak2InVehicle; 9 dossiers + 1 walk; deferrals disclosed).
        # +15 inline-state/counter contracts (GetCamera/UsingObject/Sledovacka/
        # SoundArena/Shvestky + Lightness/AnimPreload/WeatherBuffer + Score +
        # FreeSemaphores + MissionNumber x3/SkipFrames x2; marker-only).
        # +15 inline-state/counter differential (Ghidra bodies 2-66 insns; pin
        # tests housekeeping 62 / mission_actor_lists 25 pass).
        # +12 game-lifecycle contracts (Race6Result/DropStacks/TempActors/
        # ShadowModel-STUB/ShadowSelect/EntityBlock/StopSounds/DeathFade/
        # Pocasi/InterCity/DiffData-STUB; set_night_mission ledger-held +
        # box-count fragment unfiled; marker-only).
        # +10 game-lifecycle differential (Ghidra bodies 3-385 insns; pin
        # tests housekeeping 83 pass; 2 STUBs held as contract).
        # +11 police contracts (dtor/drop/drive/steal/status + 4 STUBs +
        # sledovacka ctor/done + init-bridges STUB; marker-only, tests green).
        # +7 police differential (Ghidra bodies 12-72 insns; pin tests
        # police_state 61 pass; 4 STUBs held as contract).
        # +1 C_human::IsPossibleColl 0x178c60 differential (complete x87
        # discriminant walk entry to RET 0x28; pin tests 4+2 pass).
        # +11 housekeeping STUB contracts (ScreamAreas/DeathFade/Invisible/
        # FindVect/CutsceneParticles x2/CreatedFrame x2/CamRot/Cartridges/
        # CarSound; cbFindTaxiPoints VA-form RVA fixed 0x5b6e0->0x1b6b60;
        # all STUBs held as contract).
        # +1 program Compile differential (Ghidra body 287/53; pin tests
        # program_compiler 20 / compile 35 pass).
        # +14 traffic-car/housekeeping STUB contracts (ctor/dtor/Init/
        # ChangeState/GameInit/GameDone + 8 scene stubs; all STUBs held
        # as contract).
        # +14 traffic-generator/don_ocas STUB contracts (ctor/dtor/Duplicate/
        # SaveData/LoadData/SaveGame x3/GameInit/GameDone/AI + GameInit/
        # IsPossibleColl/Collision; all STUBs held as contract).
        # +14 vehicle small-accessor contracts (C_Vehicle Get*/Is* + C_car
        # levels/owner/protect/wheel-coll/speed/parking/force-feedback;
        # marker-only, tests green).
        # +14 human/player action contracts (BezZasobniku/FakeWeapon/Cocot/
        # bridge/ChangeAnim/RepairGate/Holster/WeaponDrop/BoxDrop + 4 player
        # STUBs; marker-only, tests green).
        # +6 police/traffic contracts (GameDone/AI/Update/Hit/IsPossibleColl/
        # LoadData STUBs; marker-only, tests green).
        # +9 actor/human contracts (GetActorSpeed/GetMaterialID/IsPossibleColl/
        # Collision/AddToCutscene x2/GetRecordPos/GetRecordRot + 1 STUB;
        # 6 working-name helpers uncredited; marker-only, tests green).
        # +3 mission getter contracts (GetParticles/GetActors/GetEffects;
        # ScoreSetOn wrong-address remediated 0x5f9a0->0x1b9fa0; 8 working-name
        # helpers uncredited; marker-only, tests green).
        # +7 FrmActBuff contracts (StoreActor/StoreActive/RemoveActive/
        # IsActive/StoreSound/GetSound/ClearAll; 4 working-name helpers
        # uncredited; marker-only, tests green).
        # +12 human gate contracts (ChangeWeapon/Reload/PlayAnim/Jump/Climb/
        # SideJump/ClimbInCarLR/Shoot/ThrowGrenade/FuckingBox x2/DeadBody entry
        # gates; full bodies deferred; 1 working-name uncredited).
        # +2 GlobalProgram contracts (GameInit/GameDone loops; 13 working-name
        # helpers uncredited; marker-only, tests green).
        # +12 stack/death/placement contracts (Inventory/Float/Character x2,
        # DeathActor x2, GetActorFromGuid, AddFramePlacement, InternTempActAdd,
        # LightnessTick STUBs; FrmActBuff_GetModel ledger-duplicate held;
        # LightnessClear wrong-address remediated 0x1b5180->0x1b5780).
        # +12 actor accessor contracts (GetPos/SetPos/GetDir/SetDir/
        # SetOwnFrame/GetWorldDir/Rebirth/SaveGameSize/FrameActor x2 STUBs/
        # IsGlobalActor/shot AddToCutscene; 1 working-name uncredited).
        # +8 placement/shot contracts (InternalPlacementReset/ResetPlacement/
        # SaveGameLoadGuid/DebustizeFrame+NoShadowFrame STUBs/shot SetWeapon+
        # PlayRecord/human SaveGameGetSize; 4 working-name uncredited).
        # +13 police pursuit contracts (road-block/alerted/illegal-path/
        # destroy-signal/hit/known-car/sledovacka/state; 2 STUBs; working
        # names, sample-decoded as code; marker-only, tests green).
        # +13 ai-loc/blob/char-struct contracts (ai_loc x3, blob x3,
        # ground-delta STUBs x2, ReadOldCharStruct x2, dtor/death/push STUBs;
        # 2 subagent test bugs fixed; marker-only, tests green).
        # +10 vehicle setter contracts (sound-volume clean + 9 STUBs; 4 ledger
        # duplicates held unfiled with docs+tests clarified).
        # +12 Tshift/ctor/shadow-sound contracts (TshiftElem/Store/tick/
        # clearCarSeats/GameCtor/add-remove-temporary/NewShoot/NewExplosion/
        # Stop3DSound/Add-DelFrameForShadow STUBs; 2 working-name uncredited).
        # +15 human anim STUB contracts (ShiftAnimation/ChangeState/DynColls x3/
        # PoseSetPose x2/ChangeWeaponModel/DropOutItems/Zasrate x2/ForceDeath/
        # ThrowMeFromCar/Activate/Deactivate; all STUBs).
        # +13 weapon/use STUB contracts (FreeHand/WeaponEffect x2/EineMeine/
        # ChangeModel/Use_Actor/settle/slide/notify/EnterProtect/Energy/
        # Repair/Placement; all STUBs held as contract).
        # +11 lifecycle STUB contracts (Death/ExternAI/UseCar x2/Truck x2/
        # Done/Shooting/Recompile/PlayStep/DumpRecords; 2 working-name
        # empties uncredited; all STUBs held as contract).
        # +14 record/cutscene STUB contracts (RecordKey/PlayRecord/PlaceAtTime/
        # GetRecordPos/GetRecordRot x2 + GetMaterialID/ChangeState/Sounds/
        # SaveLoad/PlayStep/AddToCutscene/GetInvModelMatrix car STUBs;
        # all held as contract).
        # +14 program register contracts (Get/SetFloat/Frame/ActorReg,
        # InvalidateActorReg/IsEventID/SetAIPriority/STUBs x5;
        # owned_runtime symbols shortened per validator module rules).
        # +12 lifecycle STUB contracts (ai_loc::Update/ctor/Init/LoadData/
        # SaveGame x2/AI/Update/PersonAnim/Hit/HitInCar/Collision;
        # all STUBs held as contract).
        # +2 cutscene size contracts (GetBlockRecordsSize/GetRecordsSize;
        # 11 working-name helpers uncredited; marker-only, tests green).
        # +9 program register differential (Get/SetFloat/Frame/ActorReg,
        # InvalidateActorReg/IsEventID/SetAIPriority complete walks; pin
        # tests owned_runtime 165 pass).
        # +14 game/housekeeping contracts (DelFramePlacement/Timer/PFire/
        # PHorn/WFall/NPCKill x2/GetTangents/NextTrackProjector/
        # ClearProjectors/InvalidateColl/ClearFireHole/SetHuman/DelDriverMan).
        # +13 player/controls contracts (SaveGameGetSize/GameDone/Collision/
        # Hit/UpdateControls/ShootDeAimed/InvalidateCarDamage/ShowWeaponName/
        # SetBoxer + GetValues/GetValue/GetControl/SaveJoyStates;
        # ShootDeAimed companions noted, one entry per RVA).
        # +4 controls differential (GetValues/GetValue/GetControl/
        # SaveJoyStates complete walks; pin tests game_control 18 pass).
        # +14 traffic/railway STUB contracts (Scipni/Explosion/NewPanic/
        # PlayerPromluvil/ActivateFree/SetNewNode/EndMavani/Deactivate/
        # Lock/Unlock/GetVagon/GetEntry/SetRainDummy/Start; subagent counter
        # bug fixed to STUB; all held as contract).
        # +11 door/pumper/detector contracts (Dtor/GetBoundPoints2D/CanPump/
        # Start/SaveGame x3 + SaveGame x3/UseDetector; 0x81840 ledger-dup held,
        # 3 working-name uncredited; all held as contract).
        # +14 indicators/menu contracts (TimerGetTime/IntroSubtitles x2/
        # MissionHelp x3/ConsoleAddText/WideScreen + Destroy/ForceUpdate/
        # SetString/SetPointer/GetRangeFloat/SetRangeDword; marker-only).
        # +14 mission/recorder contracts (Dtor/Init/Ease/GetState/GetCurTime/
        # GetScaledTime/AnimNames ctor+Clear/SpeedKeys x3/RecClear/vfunc_46/
        # SetShowStart; marker-only, tests green).
        # +15 camera contracts (GetCar/SetCamera/SetPlayer/SetCar/Link/
        # SetSwing/LookAround/LookFromEyes/LezeDoToyoty/EnableFreeLook/
        # Unlock/IsInDialog/SetLookTo/SetSniperFov/SetSniperMode; marker-only).
        # +14 airplane/railway/turnout contracts (IsDestroyed/SaveGameGetSize/
        # GetMaterialID/Start x2/ShowDamage/ChangeState + Add/RemoveOuter/
        # InnerEntries/ChangeState + dtor/SaveGameSave; 0xbc490 RVA fixed;
        # 0xaaa00 already credited, held; all held as contract).
        # +13 ai/don_ocas contracts (ai_logs Clear/EntityAi getters x6/
        # DelUsingObject/AddFreeActor/GetBaseJob/GetCarFactor + Hit/
        # GetBoundPoints2D; marker-only, tests green).
        # +6 police/sledovacka STUB contracts (Zlikviduj/Coruju/Vypicuj/
        # SaveGameGetSize/Save/Load; 8 working-name uncredited; marker-only).
        # +13 indicators/menu leftovers (FadeInOut x2/Radar x2/Thief/Console/
        # BloodSplash + GM_Component ctor x2/Insert x2/GM_Menu ctor/dtor;
        # marker-only, tests green).
        # +2 vehicle leftovers (GetRotations/GetUSindex; 11 working-name
        # uncredited; marker-only, tests green).
        # +1 RecomputeLightCache differential (0x1b5d40 countdown-plus-cull
        # scope; complete 99-insn REA walk + pinning tests; -1 stores in
        # UseLightCache arm + session driver).
        # +1 DoDamage breakage dispatch differential (0xd6580; complete
        # 76-insn REA walk + DoDamage lamp-arm walk + pinning tests;
        # record/flag writes in sim backend, virtuals staged for the
        # future component owner).
        # +2 gifCollision owner contracts (DeleteCollsLevel 0x1c28d0,
        # LoadCollision 0x1c2b70; synthetic pin tests, live MISE01 stop).
        self.assertEqual(game["ported"], 149 + 1013)
        self.assertEqual(game["verified"], 149 + 517)
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
        # +22 tree tranche G (VC6 _Tree begin/erase/insert/Inc/ctor/Min/Erase/
        # Init/Lrotate/Rrotate/iter/Buynode/Dec + OFFD rebuild; Ghidra bodies
        # 5-362 insns; working-name TEMPLATE note kept; EnumFFTree held).
        # +7 driver tranche H (ctor/dtor/IsInit/CreateSource(x2)/CreateSound/
        # GetCaps; 5 dossiers + 2 walks; Close/InitBuffer/SetVolume held).
        # +6 driver tranche I (Create3DSound(x2)/CreateListener(x2)/InitDSound
        # repair/Init from-record; 5 dossiers + 1 walk; Init mark held).
        # +1 driver SetVolume (complete 60-insn walk: init-flag + MRG_ZERO +
        # clamp [0.0..1.0] via FCOMP + voice loop OR 0x8; close/initbuf held).
        # +3 lit_object tranche J (GetNumRectInfos/GetRectInfos/Destruct;
        # complete both-branch walks; SetActiveLevel loop held).
        # +1 driver Close (complete ~70-insn walk: voice loop + 7 releases +
        # flag clear; InitBuffer held).
        # +2 InitBuffer/SetActiveLevel (complete walks: WAVEFORMATEX+HRESULT
        # map + 10-loop lighting; frame/anim low-RVA pools held).
        # +7 3dsound tranche K (SetPos/Dir/Velocity walks + SetDistances/Open/
        # Close dossiers + CreateSound out-variant; SetCone/OutVolume tails +
        # GetSource held).
        # +2 I3D_sound GetPlayTime/ForceUpdate (complete walks: null->0/vcall
        # + wrapper CALL 873-insn worker covered through entry; pin tests).
        # +4 ISND_sound lifecycle (GetSource walk + Open/Close/Stop dossiers;
        # Play walks + Get/SetCurrTime/Volume held).
        # +1 ISND_sound Release (complete walk both branches; pin test
        # release_paths).
        # +2 3dsound SetCone/SetOutVolume (complete walks all branches:
        # null->0, FMUL/ftol + vcall + HRESULT; pin tests).
        # +4 ISND listener/source batch (Update/CommitDeferred/Lock/Unlock
        # contracts + complete walks; 9 working-name helpers held).
        # +4 ISND_sound volume/freq/pan batch (SetVolume float/int + SetFreq/
        # SetPanning complete walks with clamps + vcalls; pin tests).
        # +1 ISND_sound Play-with-source (complete walk: source+play vcalls;
        # pin test).
        # +7 ISND source lifecycle batch (ctor/dtor/Release/Close dossiers +
        # IsOpen/GetFileName/IsPlaying walks; pin tests isound_source 14).
        # +4 texture/visual batch (CompressTexture dossier + NumFaces/GetLOD/
        # NumInterpolators complete walks; itexture/visual 129/143 pass).
        # +3 frame batch (Release/GetChild/SetCallback complete walks;
        # pin tests model_loading 112 / frame_transform 55).
        # +2 weather dummy batch (GetDummyCnt/GetDummies complete 3-insn
        # walks; pin tests weather_dummy 3 / dummy 18; loops/dispatch held).
        # +3 EnumProc link batch (sector/occluder/area ring links, complete
        # 6-insn walks; 11 working-name helpers held uncredited).
        # +1 singlemesh UpdateVertices(index) (complete 7-insn walk: forward +
        # counter bump; pin tests singlemesh 33 pass).
        # +2 lit_object UpdateVertices batch (all + indexed, complete loop
        # walks entry to RET; pin tests lit_ 67 / i3d_object 76).
        # +4 IPol/pose-blend batch (GetInterpolators/GetPoseBlend/SetPoseBlend
        # x2 complete delegation walks; pin tests 23/21/2).
        # +12 mixed small-getter contracts (area/landscape/part/sector/scene/
        # material; marker-only, tests green; working-name helper unfiled).
        # +9 mixed small-getter differential (complete 1-7 insn walks; pin
        # tests green; 2 STUBs + outside-memory held as contract).
        # +2 mesh-level batch (GetFGroup/GetStats complete walks; pin tests
        # visual_resources 147 pass).
        # +2 EnumProc/link batch (MakeReadySectors EnumProc + billboard
        # SetSpace; 1 complete walk + 1 outside-memory hold; pin tests).
        # +1 ISND SetVolume (complete walk all branches; pin tests 3+1).
        # +3 model/scene batch (ResetAllPoses/SetName/SetCallback complete
        # walks; pin tests reset 3 / model_loading 119 / scene_ownership 39).
        # +1 area SetSwitchFadeTime contract (outside-memory, held;
        # 11 working-name helpers uncredited).
        # +2 AnimIsOn/GetName batch (complete walks; pin tests 1/7/1;
        # SetName shared-helper held as contract).
        # +5 scene-anim batch (GetAnimationSet/GetAnimBlend/GetAnimTime/
        # AnimIsOn/NumInterpolators complete walks; pin tests 10/8).
        # +7 IGraph keyboard batch (ReadCharBuffer/ClearCharBuffer/ReadChar/
        # TestKey/KeyShift/KeyCtrl/KeyAlt complete walks; pin tests 5/6;
        # ReadKey/ReadKeys/GetShiftKeys/TranslateKey loops held).
        # +5 pose batch (Clear/SetRot/SetRotAbs/SetPos/SetScale complete
        # walks; pin tests animation_pose 12 pass).
        # +12 driver/projector STUB contracts (DrawPoint/Point2D/Line/Lines/
        # Triangles/Circle2D/Text/Text2D/Sprite/Box/Sphere + AddFGroups;
        # device owners deferred; marker-only, tests green).
        # +1 area LoadImage STUB contract (folded with sound-LoadImage;
        # 14 working-name helpers uncredited; marker-only, tests green).
        # +4 pose SetDir/GetFrameName differential (complete walks; port
        # flag words 0x4/0x8 match native; pin tests animation_pose 12).
        # +7 collision/driver STUB contracts (enumSubtree/TestColGeo/
        # TestColSphere/CTPFirst/CullThruPortal/UpdateSound/landpatch Render;
        # phase slices + device STUBs; marker-only, tests green).
        # +1 FrameType2String trampoline contract (0x19a10; 12 working-name
        # helpers uncredited; 2 subagent mut bugs fixed).
        # +2 collision static STUB contracts (testVisual/sphTestVisual;
        # 13 colf working-name predicates uncredited; marker-only, green).
        # +12 IGraph/global contracts (TEV/GetIGraph/drivers/HWND x2/AppName x2/
        # DlgHWND x2/SetTexture STUB + Tick-empty tail; VA-form fixed in docs;
        # 11 working-name uncredited; marker-only, tests green).
        self.assertEqual(ls3df["ported"], 145 + 887)
        self.assertEqual(ls3df["verified"], 145 + 458)

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
