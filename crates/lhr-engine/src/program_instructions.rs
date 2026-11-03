//! Complete authored instruction keyword recognition for original C_program.
//!
//! The table preserves the original recognition order, IDs and terminal ASCII
//! spaces. It describes the instruction-language interface, not handler layouts.
//! DecodeInstruction's canonical PC entry RVA is 0x63960. This bounded byte
//! projection does not implement DecodeParams, Compile or instruction execution.

use crate::program_parser::{c_length, ParserLocale, ProgramParserError};

/// One original recognition-table entry. `name` includes its terminal space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstructionKeyword {
    pub name: &'static str,
    pub id: i32,
}

/// Which recognition table `program_compiler::compile` scans. Base covers the
/// 398 C_program keywords; Entity scans the 121-entry C_entity table first and
/// falls back to the base table, mirroring entity sources that mix both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstructionDecoder {
    Base,
    Entity,
}

/// All 398 original instruction keywords, in recognition order (not ID order).
pub const INSTRUCTION_KEYWORDS: [InstructionKeyword; 398] = [
    InstructionKeyword {
        name: "LABEL ",
        id: 0,
    },
    InstructionKeyword {
        name: "GOTO ",
        id: 1,
    },
    InstructionKeyword {
        name: "GOSUB ",
        id: 2,
    },
    InstructionKeyword {
        name: "RETURN ",
        id: 3,
    },
    InstructionKeyword {
        name: "WAIT ",
        id: 4,
    },
    InstructionKeyword {
        name: "BEEP ",
        id: 5,
    },
    InstructionKeyword {
        name: "END ",
        id: 6,
    },
    InstructionKeyword {
        name: "DIM_FLT ",
        id: 7,
    },
    InstructionKeyword {
        name: "LET ",
        id: 8,
    },
    InstructionKeyword { name: "IF ", id: 9 },
    InstructionKeyword {
        name: "RND ",
        id: 10,
    },
    InstructionKeyword {
        name: "DIM_FRM ",
        id: 11,
    },
    InstructionKeyword {
        name: "DIM_ACT ",
        id: 12,
    },
    InstructionKeyword {
        name: "FINDFRAME ",
        id: 13,
    },
    InstructionKeyword {
        name: "FINDACTOR ",
        id: 18,
    },
    InstructionKeyword {
        name: "FRM_SETON ",
        id: 14,
    },
    InstructionKeyword {
        name: "FRM_SETON1 ",
        id: 15,
    },
    InstructionKeyword {
        name: "FRM_LINKTO ",
        id: 17,
    },
    InstructionKeyword {
        name: "CALL ",
        id: 19,
    },
    InstructionKeyword {
        name: "IFFLTINRANGE ",
        id: 20,
    },
    InstructionKeyword {
        name: "PLAYSOUND ",
        id: 21,
    },
    InstructionKeyword {
        name: "PLAYSOUNDSTOP ",
        id: 22,
    },
    InstructionKeyword {
        name: "PLAYSOUNDEX ",
        id: 23,
    },
    InstructionKeyword {
        name: "STOPSOUND ",
        id: 24,
    },
    InstructionKeyword {
        name: "GETSOUNDTIME ",
        id: 25,
    },
    InstructionKeyword {
        name: "ACT_SETSTATE ",
        id: 26,
    },
    InstructionKeyword {
        name: "DETECTOR_ISSIGNAL ",
        id: 27,
    },
    InstructionKeyword {
        name: "DETECTOR_SETSIGNAL ",
        id: 28,
    },
    InstructionKeyword {
        name: "DETECTOR_INRANGE ",
        id: 29,
    },
    InstructionKeyword {
        name: "SETCOMPASS ",
        id: 30,
    },
    InstructionKeyword {
        name: "SETLMLEVEL ",
        id: 31,
    },
    InstructionKeyword {
        name: "DETECTOR_WAITFORUSE ",
        id: 32,
    },
    InstructionKeyword {
        name: "DETECTOR_WAITFORHIT ",
        id: 33,
    },
    InstructionKeyword {
        name: "COMMANDBLOCK ",
        id: 34,
    },
    InstructionKeyword {
        name: "CALLSUBROUTINE ",
        id: 35,
    },
    InstructionKeyword {
        name: "SOUNDFADE ",
        id: 36,
    },
    InstructionKeyword {
        name: "ENDOFMISSION ",
        id: 37,
    },
    InstructionKeyword {
        name: "PROGRAM_STORAGE ",
        id: 38,
    },
    InstructionKeyword {
        name: "DOOR_LOCK ",
        id: 39,
    },
    InstructionKeyword {
        name: "DOOR_OPEN ",
        id: 40,
    },
    InstructionKeyword {
        name: "DOOR_ENABLEUS ",
        id: 41,
    },
    InstructionKeyword {
        name: "RECCLEAR ",
        id: 42,
    },
    InstructionKeyword {
        name: "RECADDACTOR ",
        id: 43,
    },
    InstructionKeyword {
        name: "RECLOAD ",
        id: 44,
    },
    InstructionKeyword {
        name: "RECLOADFULL ",
        id: 45,
    },
    InstructionKeyword {
        name: "RECWAITFOREND ",
        id: 46,
    },
    InstructionKeyword {
        name: "RECUNLOAD ",
        id: 47,
    },
    InstructionKeyword {
        name: "LOADCOLLTREE ",
        id: 48,
    },
    InstructionKeyword {
        name: "EXPLOSION ",
        id: 49,
    },
    InstructionKeyword {
        name: "TIMERON ",
        id: 50,
    },
    InstructionKeyword {
        name: "TIMEROFF ",
        id: 51,
    },
    InstructionKeyword {
        name: "CHANGE_MISSION ",
        id: 52,
    },
    InstructionKeyword {
        name: "CAMERA_SETSWING ",
        id: 53,
    },
    InstructionKeyword {
        name: "AIRPLANE_START ",
        id: 54,
    },
    InstructionKeyword {
        name: "AIRPLANE_START2 ",
        id: 55,
    },
    InstructionKeyword {
        name: "DEBUG_TEXT ",
        id: 56,
    },
    InstructionKeyword {
        name: "GETACTORSDIST ",
        id: 57,
    },
    InstructionKeyword {
        name: "GETCARDAMAGE ",
        id: 58,
    },
    InstructionKeyword {
        name: "HUMAN_ADDWEAPON ",
        id: 59,
    },
    InstructionKeyword {
        name: "HUMAN_CANADDWEAPON ",
        id: 60,
    },
    InstructionKeyword {
        name: "HUMAN_DELWEAPON ",
        id: 61,
    },
    InstructionKeyword {
        name: "HUMAN_ISWEAPON ",
        id: 62,
    },
    InstructionKeyword {
        name: "PRELOADMODEL ",
        id: 67,
    },
    InstructionKeyword {
        name: "LOADDIFFERENCES ",
        id: 68,
    },
    InstructionKeyword {
        name: "CLEARDIFFERENCES ",
        id: 69,
    },
    InstructionKeyword { name: "{ ", id: 70 },
    InstructionKeyword { name: "} ", id: 71 },
    InstructionKeyword {
        name: "SETAIPRIORITY ",
        id: 72,
    },
    InstructionKeyword {
        name: "EVENT ",
        id: 73,
    },
    InstructionKeyword {
        name: "SETEVENT ",
        id: 74,
    },
    InstructionKeyword {
        name: "EVENT_USE_CB ",
        id: 75,
    },
    InstructionKeyword {
        name: "ENEMY_CHANGEANIM ",
        id: 76,
    },
    InstructionKeyword {
        name: "HUMAN_CHANGEANIM ",
        id: 77,
    },
    InstructionKeyword {
        name: "DIALOG_BEGIN ",
        id: 78,
    },
    InstructionKeyword {
        name: "DIALOG_END ",
        id: 79,
    },
    InstructionKeyword {
        name: "DIALOG_CAMSWITCH ",
        id: 80,
    },
    InstructionKeyword {
        name: "GAME_NIGHTMISSION ",
        id: 81,
    },
    InstructionKeyword {
        name: "HUMAN_LOOKTOACTOR ",
        id: 82,
    },
    InstructionKeyword {
        name: "HUMAN_LOOKTOFRAME ",
        id: 83,
    },
    InstructionKeyword {
        name: "GETMISSIONNUMBER ",
        id: 84,
    },
    InstructionKeyword {
        name: "SETMISSIONNUMBER ",
        id: 85,
    },
    InstructionKeyword {
        name: "HUMAN_LINKTOHAND ",
        id: 86,
    },
    InstructionKeyword {
        name: "HUMAN_UNLINKFROMHAND ",
        id: 87,
    },
    InstructionKeyword {
        name: "CREATEWEAPONFROMFRAME ",
        id: 88,
    },
    InstructionKeyword {
        name: "GARAGE_ADDCAR ",
        id: 89,
    },
    InstructionKeyword {
        name: "GARAGE_ADDCARIDX ",
        id: 90,
    },
    InstructionKeyword {
        name: "GARAGE_DELCAR ",
        id: 91,
    },
    InstructionKeyword {
        name: "GARAGE_GENERATECARS ",
        id: 92,
    },
    InstructionKeyword {
        name: "GARAGE_RELEASECARS ",
        id: 93,
    },
    InstructionKeyword {
        name: "GARAGE_CARMANAGER ",
        id: 94,
    },
    InstructionKeyword {
        name: "PLAYER_LOCKCONTROLS ",
        id: 95,
    },
    InstructionKeyword {
        name: "PERSON_PLAYANIM ",
        id: 96,
    },
    InstructionKeyword {
        name: "PERSON_STOPANIM ",
        id: 97,
    },
    InstructionKeyword {
        name: "GUNSHOP_MENU ",
        id: 98,
    },
    InstructionKeyword {
        name: "HUMAN_SET8SLOT ",
        id: 99,
    },
    InstructionKeyword {
        name: "ENEMY_FORCESCRIPT ",
        id: 100,
    },
    InstructionKeyword {
        name: "COMPAREOWNERWITHEX ",
        id: 101,
    },
    InstructionKeyword {
        name: "HUMAN_SETPROPERTY ",
        id: 102,
    },
    InstructionKeyword {
        name: "HUMAN_GETPROPERTY ",
        id: 103,
    },
    InstructionKeyword {
        name: "DISABLECOLLS ",
        id: 104,
    },
    InstructionKeyword {
        name: "HUMAN_GETOWNER ",
        id: 105,
    },
    InstructionKeyword {
        name: "HUMAN_FORCE_SETTOCAR ",
        id: 106,
    },
    InstructionKeyword {
        name: "CAR_SETDESTROYMOTOR ",
        id: 107,
    },
    InstructionKeyword {
        name: "ACTOR_SETDIR ",
        id: 108,
    },
    InstructionKeyword {
        name: "ACTOR_SETPLACEMENT ",
        id: 110,
    },
    InstructionKeyword {
        name: "ACTOR_SETPOS ",
        id: 109,
    },
    InstructionKeyword {
        name: "GARAGE_ENABLESTEAL ",
        id: 111,
    },
    InstructionKeyword {
        name: "SETTIMEOUTEVENT ",
        id: 112,
    },
    InstructionKeyword {
        name: "EMITPARTICLE ",
        id: 113,
    },
    InstructionKeyword {
        name: "STOPPARTICLE ",
        id: 114,
    },
    InstructionKeyword {
        name: "SETPLAYERFIREEVENT ",
        id: 115,
    },
    InstructionKeyword {
        name: "FINDNEARACTOR ",
        id: 116,
    },
    InstructionKeyword {
        name: "SETPLAYERHORNEVENT ",
        id: 117,
    },
    InstructionKeyword {
        name: "SETPLAYERWFALLEVENT ",
        id: 302,
    },
    InstructionKeyword {
        name: "HUMAN_HOLSTER ",
        id: 118,
    },
    InstructionKeyword {
        name: "CAMERA_LOCK ",
        id: 119,
    },
    InstructionKeyword {
        name: "CAMERA_UNLOCK ",
        id: 120,
    },
    InstructionKeyword {
        name: "SETMISSIONNAMEID ",
        id: 121,
    },
    InstructionKeyword {
        name: "HUMAN_ACTIVATEWEAPON ",
        id: 63,
    },
    InstructionKeyword {
        name: "HUMAN_ANYWEAPONINHAND ",
        id: 64,
    },
    InstructionKeyword {
        name: "HUMAN_ANYWEAPONININVENTORY ",
        id: 65,
    },
    InstructionKeyword {
        name: "WINGMAN_SETINDICATOR ",
        id: 122,
    },
    InstructionKeyword {
        name: "MODEL_CREATE ",
        id: 126,
    },
    InstructionKeyword {
        name: "MODEL_DESTROY ",
        id: 127,
    },
    InstructionKeyword {
        name: "MODEL_PLAYANIM ",
        id: 124,
    },
    InstructionKeyword {
        name: "MODEL_STOPANIM ",
        id: 125,
    },
    InstructionKeyword {
        name: "HUMAN_CHANGEMODEL ",
        id: 128,
    },
    InstructionKeyword {
        name: "AUTOSAVEGAME ",
        id: 129,
    },
    InstructionKeyword {
        name: "AUTOSAVEGAMEFULL ",
        id: 130,
    },
    InstructionKeyword {
        name: "HUMAN_DEATH ",
        id: 131,
    },
    InstructionKeyword {
        name: "HUMAN_TALK ",
        id: 132,
    },
    InstructionKeyword {
        name: "HUMAN_ERASEAB ",
        id: 133,
    },
    InstructionKeyword {
        name: "HUMAN_CREATEAB ",
        id: 134,
    },
    InstructionKeyword {
        name: "POLICEMANAGER_ON ",
        id: 135,
    },
    InstructionKeyword {
        name: "POLICEMANAGER_ADD ",
        id: 136,
    },
    InstructionKeyword {
        name: "POLICEMANAGER_DEL ",
        id: 137,
    },
    InstructionKeyword {
        name: "CAR_ENABLEUS ",
        id: 140,
    },
    InstructionKeyword {
        name: "CAR_SETDOOROPEN ",
        id: 141,
    },
    InstructionKeyword {
        name: "HUMAN_SETFIRETARGET ",
        id: 142,
    },
    InstructionKeyword {
        name: "DOOR_SETOPENPOSE ",
        id: 144,
    },
    InstructionKeyword {
        name: "AIRPLANE_ISDESTROYED ",
        id: 145,
    },
    InstructionKeyword {
        name: "HUMAN_RETURNFROMPANIC ",
        id: 146,
    },
    InstructionKeyword {
        name: "CAR_GETSPEED ",
        id: 147,
    },
    InstructionKeyword {
        name: "ISCARUSABLE ",
        id: 143,
    },
    InstructionKeyword {
        name: "CAR_BREAKMOTOR ",
        id: 150,
    },
    InstructionKeyword {
        name: "INVENTORY_PUSH ",
        id: 151,
    },
    InstructionKeyword {
        name: "INVENTORY_POP ",
        id: 152,
    },
    InstructionKeyword {
        name: "CHARACTER_PUSH ",
        id: 215,
    },
    InstructionKeyword {
        name: "CHARACTER_POP ",
        id: 216,
    },
    InstructionKeyword {
        name: "HUMAN_THROWITEM ",
        id: 156,
    },
    InstructionKeyword {
        name: "FUCKINGBOX_ADD ",
        id: 153,
    },
    InstructionKeyword {
        name: "FUCKINGBOX_RECOMPILE ",
        id: 154,
    },
    InstructionKeyword {
        name: "FUCKINGBOX_ADD_DEST ",
        id: 155,
    },
    InstructionKeyword {
        name: "FUCKINGBOX_GETNUMBOX ",
        id: 157,
    },
    InstructionKeyword {
        name: "FUCKINGBOX_GETNUMDEST ",
        id: 158,
    },
    InstructionKeyword {
        name: "FUCKINGBOX_MOVE ",
        id: 159,
    },
    InstructionKeyword {
        name: "WAGON_SETEVENT ",
        id: 160,
    },
    InstructionKeyword {
        name: "WAGON_GETLASTNODE ",
        id: 161,
    },
    InstructionKeyword {
        name: "CAR_SETSPECCOLL ",
        id: 162,
    },
    InstructionKeyword {
        name: "CAR_GETSPECCOLL ",
        id: 163,
    },
    InstructionKeyword {
        name: "MISSION_OBJECTIVES ",
        id: 164,
    },
    InstructionKeyword {
        name: "MISSION_OBJECTIVESREMOVE ",
        id: 165,
    },
    InstructionKeyword {
        name: "MISSION_OBJECTIVESCLEAR ",
        id: 166,
    },
    InstructionKeyword {
        name: "RACING_MISSION6_INIT ",
        id: 167,
    },
    InstructionKeyword {
        name: "RACING_MISSION6_START ",
        id: 168,
    },
    InstructionKeyword {
        name: "CAR_EXPLOSION ",
        id: 169,
    },
    InstructionKeyword {
        name: "HUMAN_FROMCAR ",
        id: 170,
    },
    InstructionKeyword {
        name: "[BREAK] ",
        id: 171,
    },
    InstructionKeyword {
        name: "CAR_REMOVE_DRIVER ",
        id: 172,
    },
    InstructionKeyword {
        name: "SETMODELTOCAR ",
        id: 173,
    },
    InstructionKeyword {
        name: "GETENEMYAISTATE ",
        id: 174,
    },
    InstructionKeyword {
        name: "GET_PM_STATE ",
        id: 175,
    },
    InstructionKeyword {
        name: "GET_PM_CRASHTIME ",
        id: 176,
    },
    InstructionKeyword {
        name: "GET_PM_FIRETIME ",
        id: 177,
    },
    InstructionKeyword {
        name: "GET_PM_NUMPREDATORS ",
        id: 178,
    },
    InstructionKeyword {
        name: "USE_LIGHTCACHE ",
        id: 180,
    },
    InstructionKeyword {
        name: "STREAM_CREATE ",
        id: 181,
    },
    InstructionKeyword {
        name: "STREAM_CONNECT ",
        id: 182,
    },
    InstructionKeyword {
        name: "STREAM_PLAY ",
        id: 183,
    },
    InstructionKeyword {
        name: "STREAM_PAUSE ",
        id: 184,
    },
    InstructionKeyword {
        name: "STREAM_STOP ",
        id: 185,
    },
    InstructionKeyword {
        name: "STREAM_DESTROY ",
        id: 186,
    },
    InstructionKeyword {
        name: "STREAM_FADEVOL ",
        id: 187,
    },
    InstructionKeyword {
        name: "STREAM_SETLOOP ",
        id: 188,
    },
    InstructionKeyword {
        name: "SETTANKHITCOUNT ",
        id: 191,
    },
    InstructionKeyword {
        name: "SETNOANIMHIT ",
        id: 192,
    },
    InstructionKeyword {
        name: "CARLIGHT_MAIN ",
        id: 193,
    },
    InstructionKeyword {
        name: "CARLIGHT_LIGHT ",
        id: 194,
    },
    InstructionKeyword {
        name: "CARLIGHT_INDIC_L ",
        id: 195,
    },
    InstructionKeyword {
        name: "CARLIGHT_INDIC_R ",
        id: 196,
    },
    InstructionKeyword {
        name: "CAR_MUSTSTEAL ",
        id: 198,
    },
    InstructionKeyword {
        name: "CAR_SETPROJECTOR ",
        id: 199,
    },
    InstructionKeyword {
        name: "GARAGE_CANSTEAL ",
        id: 200,
    },
    InstructionKeyword {
        name: "CAR_INWATER ",
        id: 203,
    },
    InstructionKeyword {
        name: "HUMAN_FORCEFALL ",
        id: 204,
    },
    InstructionKeyword {
        name: "CREATE_PHYSICALOBJECT ",
        id: 205,
    },
    InstructionKeyword {
        name: "WEATHER_RESET ",
        id: 207,
    },
    InstructionKeyword {
        name: "WEATHER_SETPARAM ",
        id: 208,
    },
    InstructionKeyword {
        name: "GETTICKTIME ",
        id: 209,
    },
    InstructionKeyword {
        name: "GETGAMETIME ",
        id: 210,
    },
    InstructionKeyword {
        name: "POLICEMANAGER_SETSPEED ",
        id: 138,
    },
    InstructionKeyword {
        name: "GARAGE_ADDLASTSTOLEN ",
        id: 201,
    },
    InstructionKeyword {
        name: "GARAGE_SHOW ",
        id: 202,
    },
    InstructionKeyword {
        name: "GETACTIVEPLAYER ",
        id: 211,
    },
    InstructionKeyword {
        name: "GETANGLEACTORTOACTOR ",
        id: 212,
    },
    InstructionKeyword {
        name: "POLICEMANAGER_FORCEARREST ",
        id: 139,
    },
    InstructionKeyword {
        name: "SETCITYTRAFFICVISIBLE ",
        id: 213,
    },
    InstructionKeyword {
        name: "CAR_SETSPEED ",
        id: 148,
    },
    InstructionKeyword {
        name: "SHOWCARDAMAGE ",
        id: 214,
    },
    InstructionKeyword {
        name: "CONSOLE_ADDTEXT ",
        id: 217,
    },
    InstructionKeyword {
        name: "CAMERA_SETRANGE ",
        id: 218,
    },
    InstructionKeyword {
        name: "ACTOR_DUPLICATE ",
        id: 219,
    },
    InstructionKeyword {
        name: "ACTOR_DELETE ",
        id: 220,
    },
    InstructionKeyword {
        name: "CAR_FORCESTOP ",
        id: 149,
    },
    InstructionKeyword {
        name: "WINGMAN_DELINDICATOR ",
        id: 123,
    },
    InstructionKeyword {
        name: "HUMAN_RESET ",
        id: 221,
    },
    InstructionKeyword {
        name: "HUMAN_HAVEBOX ",
        id: 222,
    },
    InstructionKeyword {
        name: "HUMAN_HAVEBODY ",
        id: 223,
    },
    InstructionKeyword {
        name: "HUMAN_STOPTALK ",
        id: 224,
    },
    InstructionKeyword {
        name: "ENEMY_BLOCK ",
        id: 225,
    },
    InstructionKeyword {
        name: "ENEMY_UNBLOCK ",
        id: 226,
    },
    InstructionKeyword {
        name: "GET_REMOTE_FLOAT ",
        id: 227,
    },
    InstructionKeyword {
        name: "GET_REMOTE_FRAME ",
        id: 228,
    },
    InstructionKeyword {
        name: "GET_REMOTE_ACTOR ",
        id: 229,
    },
    InstructionKeyword {
        name: "SET_REMOTE_FLOAT ",
        id: 230,
    },
    InstructionKeyword {
        name: "SET_REMOTE_FRAME ",
        id: 231,
    },
    InstructionKeyword {
        name: "SET_REMOTE_ACTOR ",
        id: 232,
    },
    InstructionKeyword {
        name: "GETACTORFRAME ",
        id: 233,
    },
    InstructionKeyword {
        name: "ZATMYSE ",
        id: 234,
    },
    InstructionKeyword {
        name: "CARDAMAGEVISIBLE ",
        id: 235,
    },
    InstructionKeyword {
        name: "[DEBUG_ME] ",
        id: 236,
    },
    InstructionKeyword {
        name: "[DEBUG_STOP] ",
        id: 237,
    },
    InstructionKeyword {
        name: "IFPLAYERSTEALCAR ",
        id: 238,
    },
    InstructionKeyword {
        name: "INTRO_SUBTITLE_ADD ",
        id: 239,
    },
    InstructionKeyword {
        name: "MATH_COS ",
        id: 241,
    },
    InstructionKeyword {
        name: "MATH_SIN ",
        id: 242,
    },
    InstructionKeyword {
        name: "MATH_ACOS ",
        id: 243,
    },
    InstructionKeyword {
        name: "MATH_ASIN ",
        id: 244,
    },
    InstructionKeyword {
        name: "MATH_ATAN ",
        id: 245,
    },
    InstructionKeyword {
        name: "MATH_ABS ",
        id: 246,
    },
    InstructionKeyword {
        name: "FRM_GETDIR ",
        id: 247,
    },
    InstructionKeyword {
        name: "FRM_SETDIR ",
        id: 248,
    },
    InstructionKeyword {
        name: "FRM_GETPOS ",
        id: 249,
    },
    InstructionKeyword {
        name: "FRM_SETPOS ",
        id: 250,
    },
    InstructionKeyword {
        name: "FRM_GETSCALE ",
        id: 251,
    },
    InstructionKeyword {
        name: "FRM_SETSCALE ",
        id: 252,
    },
    InstructionKeyword {
        name: "FRM_GETROT ",
        id: 253,
    },
    InstructionKeyword {
        name: "FRM_SETROT ",
        id: 254,
    },
    InstructionKeyword {
        name: "FRM_GETWORLDDIR ",
        id: 255,
    },
    InstructionKeyword {
        name: "FRM_GETWORLDPOS ",
        id: 256,
    },
    InstructionKeyword {
        name: "FRM_GETWORLDSCALE ",
        id: 257,
    },
    InstructionKeyword {
        name: "FRM_GETWORLDROT ",
        id: 258,
    },
    InstructionKeyword {
        name: "FRM_GETLOCALMATRIX ",
        id: 259,
    },
    InstructionKeyword {
        name: "FRM_GETWORLDMATRIX ",
        id: 260,
    },
    InstructionKeyword {
        name: "VECT_SET ",
        id: 261,
    },
    InstructionKeyword {
        name: "VECT_COPY ",
        id: 262,
    },
    InstructionKeyword {
        name: "VECT_MUL_SCL ",
        id: 263,
    },
    InstructionKeyword {
        name: "VECT_MUL_VECT ",
        id: 264,
    },
    InstructionKeyword {
        name: "VECT_MUL_QUAT ",
        id: 265,
    },
    InstructionKeyword {
        name: "VECT_MUL_MATRIX ",
        id: 266,
    },
    InstructionKeyword {
        name: "VECT_ADD_VECT ",
        id: 267,
    },
    InstructionKeyword {
        name: "VECT_SUB_VECT ",
        id: 268,
    },
    InstructionKeyword {
        name: "VECT_CROSS ",
        id: 269,
    },
    InstructionKeyword {
        name: "VECT_NORMALIZE ",
        id: 270,
    },
    InstructionKeyword {
        name: "VECT_MAGNITUDE ",
        id: 271,
    },
    InstructionKeyword {
        name: "VECT_INVERSE ",
        id: 272,
    },
    InstructionKeyword {
        name: "VECT_ANGLETO ",
        id: 273,
    },
    InstructionKeyword {
        name: "MATRIX_ZERO ",
        id: 276,
    },
    InstructionKeyword {
        name: "MATRIX_IDENTITY ",
        id: 277,
    },
    InstructionKeyword {
        name: "MATRIX_COPY ",
        id: 278,
    },
    InstructionKeyword {
        name: "MATRIX_INVERSE ",
        id: 279,
    },
    InstructionKeyword {
        name: "MATRIX_MUL ",
        id: 280,
    },
    InstructionKeyword {
        name: "QUAT_COPY ",
        id: 281,
    },
    InstructionKeyword {
        name: "QUAT_MAKE ",
        id: 282,
    },
    InstructionKeyword {
        name: "QUAT_SETDIR ",
        id: 283,
    },
    InstructionKeyword {
        name: "QUAT_EXTRACT ",
        id: 284,
    },
    InstructionKeyword {
        name: "QUAT_INVERSE ",
        id: 285,
    },
    InstructionKeyword {
        name: "QUAT_GETROTMATRIX ",
        id: 286,
    },
    InstructionKeyword {
        name: "QUAT_ADD ",
        id: 287,
    },
    InstructionKeyword {
        name: "QUAT_SUB ",
        id: 288,
    },
    InstructionKeyword {
        name: "QUAT_MUL_SCL ",
        id: 289,
    },
    InstructionKeyword {
        name: "QUAT_MUL_QUAT ",
        id: 290,
    },
    InstructionKeyword {
        name: "QUAT_DOT ",
        id: 291,
    },
    InstructionKeyword {
        name: "QUAT_NORMALIZE ",
        id: 292,
    },
    InstructionKeyword {
        name: "QUAT_ROTBYMATRIX ",
        id: 293,
    },
    InstructionKeyword {
        name: "QUAT_SLERP ",
        id: 294,
    },
    InstructionKeyword {
        name: "COLL_TESTLINE ",
        id: 295,
    },
    InstructionKeyword {
        name: "VLVP ",
        id: 296,
    },
    InstructionKeyword {
        name: "POLICEITCHFORPLAYER ",
        id: 306,
    },
    InstructionKeyword {
        name: "CTRL_READ ",
        id: 297,
    },
    InstructionKeyword {
        name: "CTRL_READEX ",
        id: 298,
    },
    InstructionKeyword {
        name: "GETFRAMEFROMACTOR ",
        id: 299,
    },
    InstructionKeyword {
        name: "ACTORUPDATEPLACEMENT ",
        id: 300,
    },
    InstructionKeyword {
        name: "RACING_AUTOINVISIBLE ",
        id: 301,
    },
    InstructionKeyword {
        name: "CAR_INVISIBLE ",
        id: 303,
    },
    InstructionKeyword {
        name: "CAR_SWITCHSHOWENERGY ",
        id: 304,
    },
    InstructionKeyword {
        name: "DEBUG_GETFRAMEINFO ",
        id: 305,
    },
    InstructionKeyword {
        name: "NOANIMPRELOAD ",
        id: 307,
    },
    InstructionKeyword {
        name: "DESTROY_PHYSICALOBJECT ",
        id: 206,
    },
    InstructionKeyword {
        name: "COMPAREACTORS ",
        id: 308,
    },
    InstructionKeyword {
        name: "SETNULLACTOR ",
        id: 309,
    },
    InstructionKeyword {
        name: "COMPAREFRAMES ",
        id: 310,
    },
    InstructionKeyword {
        name: "SETNULLFRAME ",
        id: 311,
    },
    InstructionKeyword {
        name: "SETFREERIDE ",
        id: 312,
    },
    InstructionKeyword {
        name: "ENABLEMAP ",
        id: 313,
    },
    InstructionKeyword {
        name: "FRAMEISTELEPHONE ",
        id: 314,
    },
    InstructionKeyword {
        name: "CAR_CALM ",
        id: 315,
    },
    InstructionKeyword {
        name: "BRIDGE_SHUTDOWN ",
        id: 316,
    },
    InstructionKeyword {
        name: "CARLIGHT_INDIC_OFF ",
        id: 197,
    },
    InstructionKeyword {
        name: "SETTRAFFSECTORSND ",
        id: 317,
    },
    InstructionKeyword {
        name: "CITYMUSIC_ON ",
        id: 319,
    },
    InstructionKeyword {
        name: "CITYMUSIC_OFF ",
        id: 318,
    },
    InstructionKeyword {
        name: "[DAN_DEBUG] ",
        id: 320,
    },
    InstructionKeyword {
        name: "FRM_GETPARENT ",
        id: 321,
    },
    InstructionKeyword {
        name: "ISPOINTINSECTOR ",
        id: 322,
    },
    InstructionKeyword {
        name: "ISPOINTINAREA ",
        id: 323,
    },
    InstructionKeyword {
        name: "GETACTIVECAMERA ",
        id: 324,
    },
    InstructionKeyword {
        name: "SOUND_GETVOLUME ",
        id: 325,
    },
    InstructionKeyword {
        name: "SOUND_SETVOLUME ",
        id: 326,
    },
    InstructionKeyword {
        name: "RACING_CHANGE_MODEL ",
        id: 327,
    },
    InstructionKeyword {
        name: "CITYCACHING_TICK ",
        id: 328,
    },
    InstructionKeyword {
        name: "ACTORLIGHTNESSON ",
        id: 329,
    },
    InstructionKeyword {
        name: "CAR_LOCK ",
        id: 330,
    },
    InstructionKeyword {
        name: "CAR_LOCK_ALL ",
        id: 331,
    },
    InstructionKeyword {
        name: "PM_SHOWSYMBOL ",
        id: 332,
    },
    InstructionKeyword {
        name: "PM_SHOWPROGRESS ",
        id: 333,
    },
    InstructionKeyword {
        name: "PM_SETPROGRESS ",
        id: 334,
    },
    InstructionKeyword {
        name: "SETFILMMUSIC ",
        id: 335,
    },
    InstructionKeyword {
        name: "GETFILMMUSIC ",
        id: 336,
    },
    InstructionKeyword {
        name: "HUMAN_GETSEATIDX ",
        id: 337,
    },
    InstructionKeyword {
        name: "HUMAN_ENTERTOTRUCK ",
        id: 338,
    },
    InstructionKeyword {
        name: "FLOATREG_PUSH ",
        id: 339,
    },
    InstructionKeyword {
        name: "FLOATREG_POP ",
        id: 340,
    },
    InstructionKeyword {
        name: "AIRPLANESHOWDAMAGE ",
        id: 341,
    },
    InstructionKeyword {
        name: "SETNPCKILLEVENT ",
        id: 342,
    },
    InstructionKeyword {
        name: "STREAM_SETPOS ",
        id: 189,
    },
    InstructionKeyword {
        name: "STREAM_GETPOS ",
        id: 190,
    },
    InstructionKeyword {
        name: "CAR_GETSEATCOUNT ",
        id: 343,
    },
    InstructionKeyword {
        name: "FRM_SETALPHA ",
        id: 344,
    },
    InstructionKeyword {
        name: "HUMAN_THROWGRENADE ",
        id: 345,
    },
    InstructionKeyword {
        name: "DETECTOR_CREATEDYNCOLL ",
        id: 346,
    },
    InstructionKeyword {
        name: "DETECTOR_ERASEDYNCOLL ",
        id: 347,
    },
    InstructionKeyword {
        name: "CAR_UNBREAKABLE ",
        id: 348,
    },
    InstructionKeyword {
        name: "PHOBJ_IMPULS ",
        id: 349,
    },
    InstructionKeyword {
        name: "WEATHER_PREPAREBUFFER ",
        id: 350,
    },
    InstructionKeyword {
        name: "SUBTITLE ",
        id: 351,
    },
    InstructionKeyword {
        name: "HUMAN_CANDIE ",
        id: 352,
    },
    InstructionKeyword {
        name: "VERSION_IS_GERMANY ",
        id: 353,
    },
    InstructionKeyword {
        name: "VERSION_IS_EDITOR ",
        id: 354,
    },
    InstructionKeyword {
        name: "GARAGE_NEZAHAZUJ ",
        id: 355,
    },
    InstructionKeyword {
        name: "DAN_INTERNAL_1 ",
        id: 396,
    },
    InstructionKeyword {
        name: "DAN_INTERNAL_2 ",
        id: 397,
    },
    InstructionKeyword {
        name: "POLICE_SUPPORT ",
        id: 356,
    },
    InstructionKeyword {
        name: "PLAY_AVI_INTRO ",
        id: 357,
    },
    InstructionKeyword {
        name: "CAMERA_SETFOV ",
        id: 358,
    },
    InstructionKeyword {
        name: "CAMERA_GETFOV ",
        id: 359,
    },
    InstructionKeyword {
        name: "DOOR_GETSTATE ",
        id: 360,
    },
    InstructionKeyword {
        name: "CAR_GETMAXLEVELS ",
        id: 361,
    },
    InstructionKeyword {
        name: "CAR_GETACTLEVEL ",
        id: 362,
    },
    InstructionKeyword {
        name: "CAR_SETACTLEVEL ",
        id: 363,
    },
    InstructionKeyword {
        name: "FREERIDE_SCOREON ",
        id: 364,
    },
    InstructionKeyword {
        name: "FREERIDE_SCORESET ",
        id: 365,
    },
    InstructionKeyword {
        name: "FREERIDE_SCOREGET ",
        id: 366,
    },
    InstructionKeyword {
        name: "FREERIDE_SCOREADD ",
        id: 367,
    },
    InstructionKeyword {
        name: "TAXIDRIVER_ENABLE ",
        id: 368,
    },
    InstructionKeyword {
        name: "HUMAN_RETURNTOTRAFF ",
        id: 369,
    },
    InstructionKeyword {
        name: "TIMER_GETINTERVAL ",
        id: 370,
    },
    InstructionKeyword {
        name: "TIMER_SETINTERVAL ",
        id: 371,
    },
    InstructionKeyword {
        name: "HUMAN_WAITTOREADY ",
        id: 374,
    },
    InstructionKeyword {
        name: "FRM_GETNUMCHILDREN ",
        id: 372,
    },
    InstructionKeyword {
        name: "FRM_GETCHILD ",
        id: 373,
    },
    InstructionKeyword {
        name: "ACTOR_ADDDORICI ",
        id: 375,
    },
    InstructionKeyword {
        name: "CAR_REPAIR ",
        id: 376,
    },
    InstructionKeyword {
        name: "POLICE_SPEED_FACTOR ",
        id: 377,
    },
    InstructionKeyword {
        name: "SUBTITLE_ADD ",
        id: 240,
    },
    InstructionKeyword {
        name: "CARTRIDGE_INVALIDATE ",
        id: 378,
    },
    InstructionKeyword {
        name: "NPC_SHUTUP ",
        id: 379,
    },
    InstructionKeyword {
        name: "FREERIDE_ENABLECAR ",
        id: 380,
    },
    InstructionKeyword {
        name: "GETCARLINENUMFROMTABLE ",
        id: 381,
    },
    InstructionKeyword {
        name: "ENEMY_GETHOSTILESTATE ",
        id: 382,
    },
    InstructionKeyword {
        name: "HUMAN_SHUTUP ",
        id: 383,
    },
    InstructionKeyword {
        name: "PUMPER_CANWORK ",
        id: 384,
    },
    InstructionKeyword {
        name: "GROUP_DISBAND ",
        id: 385,
    },
    InstructionKeyword {
        name: "CAR_DISABLE_UO ",
        id: 387,
    },
    InstructionKeyword {
        name: "INVENTORY_CLEAR ",
        id: 386,
    },
    InstructionKeyword {
        name: "GETLASTSAVENUM ",
        id: 388,
    },
    InstructionKeyword {
        name: "FREERIDE_TRAFFSETUP ",
        id: 389,
    },
    InstructionKeyword {
        name: "POCZKURVENYCHBEDENCAR ",
        id: 390,
    },
    InstructionKeyword {
        name: "HUMAN_GETACTANIMID ",
        id: 391,
    },
    InstructionKeyword {
        name: "CAR_SETAXIS ",
        id: 392,
    },
    InstructionKeyword {
        name: "FRM_ISON ",
        id: 16,
    },
    InstructionKeyword {
        name: "HUMAN_GETITEMINRHAND ",
        id: 66,
    },
    InstructionKeyword {
        name: "GET_PM_HUMANSTATE ",
        id: 179,
    },
    InstructionKeyword {
        name: "M7_ZASTAVZKURVENEPOLISE ",
        id: 393,
    },
    InstructionKeyword {
        name: "ISVALIDTAXIPASSENGER ",
        id: 394,
    },
    InstructionKeyword {
        name: "GRABER_SETFOCUS ",
        id: 395,
    },
    InstructionKeyword {
        name: "VECT_ANGLETO_2D ",
        id: 274,
    },
    InstructionKeyword {
        name: "VECT_ROTATE_2D ",
        id: 275,
    },
];

/// Original C_entity::DecodeInstruction recognition table (GAME 0x51cdd0).
///
/// 121 entries in table order: `{name[32], id}` records at stride 0x24 with the
/// ID at +32, entry 0 at the table base. Most commands occupy an ENEMY_/ENTITY_
/// pair sharing one ID; eight commands are singles with their own ID
/// (COMPAREOWNERWITH 0x1009, FINDCORNER 0x100a, BLUESCREEN 0x100f, TEXT 0x1010,
/// ACTION_MOVE 0x1011, DEBUG_GRAPHS 0x1032, ACTION_CARJUMP 0x1033), and
/// NEVYHAZUJ 0x103d stores its single ENEMY_ spelling twice. Entry/slot order,
/// IDs and spellings are capstone-verified against the shipped binary; the two
/// spellings of each pair decode identically. This describes recognition only,
/// not DecodeParams or execution.
pub const ENTITY_INSTRUCTION_KEYWORDS: [InstructionKeyword; 121] = [
    InstructionKeyword {
        name: "ENEMY_MOVE ",
        id: 0x1000,
    },
    InstructionKeyword {
        name: "ENTITY_MOVE ",
        id: 0x1000,
    },
    InstructionKeyword {
        name: "ENEMY_PLAYANIM ",
        id: 0x1001,
    },
    InstructionKeyword {
        name: "ENTITY_PLAYANIM ",
        id: 0x1001,
    },
    InstructionKeyword {
        name: "ENEMY_STOPANIM ",
        id: 0x1002,
    },
    InstructionKeyword {
        name: "ENTITY_STOPANIM ",
        id: 0x1002,
    },
    InstructionKeyword {
        name: "ENEMY_LOOKTO ",
        id: 0x1003,
    },
    InstructionKeyword {
        name: "ENTITY_LOOKTO ",
        id: 0x1003,
    },
    InstructionKeyword {
        name: "ENEMY_WAIT ",
        id: 0x1004,
    },
    InstructionKeyword {
        name: "ENTITY_WAIT ",
        id: 0x1004,
    },
    InstructionKeyword {
        name: "ENEMY_LOOK ",
        id: 0x1005,
    },
    InstructionKeyword {
        name: "ENTITY_LOOK ",
        id: 0x1005,
    },
    InstructionKeyword {
        name: "ENTITY_TALK ",
        id: 0x1006,
    },
    InstructionKeyword {
        name: "ENEMY_TALK ",
        id: 0x1006,
    },
    InstructionKeyword {
        name: "ENTITY_STOP ",
        id: 0x1007,
    },
    InstructionKeyword {
        name: "ENEMY_STOP ",
        id: 0x1007,
    },
    InstructionKeyword {
        name: "ENEMY_USECAR ",
        id: 0x1008,
    },
    InstructionKeyword {
        name: "ENTITY_USECAR ",
        id: 0x1008,
    },
    InstructionKeyword {
        name: "COMPAREOWNERWITH ",
        id: 0x1009,
    },
    InstructionKeyword {
        name: "ENTITY_FINDCORNER ",
        id: 0x100a,
    },
    InstructionKeyword {
        name: "ENEMY_MOVE_TO_CAR ",
        id: 0x100b,
    },
    InstructionKeyword {
        name: "ENTITY_MOVE_TO_CAR ",
        id: 0x100b,
    },
    InstructionKeyword {
        name: "ENEMY_CAR_HUNT ",
        id: 0x100c,
    },
    InstructionKeyword {
        name: "ENTITY_CAR_HUNT ",
        id: 0x100c,
    },
    InstructionKeyword {
        name: "ENEMY_CAR_ESCAPE ",
        id: 0x100d,
    },
    InstructionKeyword {
        name: "ENTITY_CAR_ESCAPE ",
        id: 0x100d,
    },
    InstructionKeyword {
        name: "ENEMY_CAR_MOVETO ",
        id: 0x100e,
    },
    InstructionKeyword {
        name: "ENTITY_CAR_MOVETO ",
        id: 0x100e,
    },
    InstructionKeyword {
        name: "ENTITY_BLUESCREEN ",
        id: 0x100f,
    },
    InstructionKeyword {
        name: "ENTITY_TEXT ",
        id: 0x1010,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_MOVE ",
        id: 0x1011,
    },
    InstructionKeyword {
        name: "ENEMY_ACTION_FOLLOW ",
        id: 0x1012,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_FOLLOW ",
        id: 0x1012,
    },
    InstructionKeyword {
        name: "ENEMY_ENTER_DOOR ",
        id: 0x1013,
    },
    InstructionKeyword {
        name: "ENTITY_ENTER_DOOR ",
        id: 0x1013,
    },
    InstructionKeyword {
        name: "ENEMY_ACTION_FIRE ",
        id: 0x1014,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_FIRE ",
        id: 0x1014,
    },
    InstructionKeyword {
        name: "ENEMY_USE_RAILWAY ",
        id: 0x1015,
    },
    InstructionKeyword {
        name: "ENTITY_USE_RAILWAY ",
        id: 0x1015,
    },
    InstructionKeyword {
        name: "ENEMY_ACTION_RUNAWAY ",
        id: 0x1016,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_RUNAWAY ",
        id: 0x1016,
    },
    InstructionKeyword {
        name: "ENEMY_ACTION_SIDELOOK ",
        id: 0x1017,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_SIDELOOK ",
        id: 0x1017,
    },
    InstructionKeyword {
        name: "ENEMY_ACTION_BACKLOOK ",
        id: 0x1018,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_BACKLOOK ",
        id: 0x1018,
    },
    InstructionKeyword {
        name: "ENEMY_ACTION_RUNCLOSERTO ",
        id: 0x1019,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_RUNCLOSERTO ",
        id: 0x1019,
    },
    InstructionKeyword {
        name: "ENEMY_VIDIM ",
        id: 0x101a,
    },
    InstructionKeyword {
        name: "ENTITY_VIDIM ",
        id: 0x101a,
    },
    InstructionKeyword {
        name: "ENEMY_EXIT_RAILWAY ",
        id: 0x101b,
    },
    InstructionKeyword {
        name: "ENTITY_EXIT_RAILWAY ",
        id: 0x101b,
    },
    InstructionKeyword {
        name: "ENEMY_MOVE_TO_FRAME ",
        id: 0x101c,
    },
    InstructionKeyword {
        name: "ENTITY_MOVE_TO_FRAME ",
        id: 0x101c,
    },
    InstructionKeyword {
        name: "ENEMY_GETSTATE ",
        id: 0x101d,
    },
    InstructionKeyword {
        name: "ENTITY_GETSTATE ",
        id: 0x101d,
    },
    InstructionKeyword {
        name: "ENEMY_BOX_GOTO ",
        id: 0x101e,
    },
    InstructionKeyword {
        name: "ENTITY_BOX_GOTO ",
        id: 0x101e,
    },
    InstructionKeyword {
        name: "ENEMY_BOX_TAKE ",
        id: 0x101f,
    },
    InstructionKeyword {
        name: "ENTITY_BOX_TAKE ",
        id: 0x101f,
    },
    InstructionKeyword {
        name: "ENEMY_BOX_PUT ",
        id: 0x1020,
    },
    InstructionKeyword {
        name: "ENTITY_BOX_PUT ",
        id: 0x1020,
    },
    InstructionKeyword {
        name: "ENEMY_ACTION_STEPAWAY ",
        id: 0x1021,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_STEPAWAY ",
        id: 0x1021,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_STRAFE ",
        id: 0x1022,
    },
    InstructionKeyword {
        name: "ENEMY_ACTION_STRAFE ",
        id: 0x1022,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_JUMP ",
        id: 0x1023,
    },
    InstructionKeyword {
        name: "ENEMY_ACTION_JUMP ",
        id: 0x1023,
    },
    InstructionKeyword {
        name: "ENTITY_USE_DETECTOR ",
        id: 0x1024,
    },
    InstructionKeyword {
        name: "ENEMY_USE_DETECTOR ",
        id: 0x1024,
    },
    InstructionKeyword {
        name: "ENTITY_SETSTATE ",
        id: 0x1025,
    },
    InstructionKeyword {
        name: "ENEMY_SETSTATE ",
        id: 0x1025,
    },
    InstructionKeyword {
        name: "ENTITY_LOCKSTATE ",
        id: 0x1026,
    },
    InstructionKeyword {
        name: "ENEMY_LOCKSTATE ",
        id: 0x1026,
    },
    InstructionKeyword {
        name: "ENTITY_TURNBACK ",
        id: 0x1027,
    },
    InstructionKeyword {
        name: "ENEMY_TURNBACK ",
        id: 0x1027,
    },
    InstructionKeyword {
        name: "ENTITY_BRAINWASH ",
        id: 0x1028,
    },
    InstructionKeyword {
        name: "ENEMY_BRAINWASH ",
        id: 0x1028,
    },
    InstructionKeyword {
        name: "ENTITY_KICKDRIVER ",
        id: 0x1029,
    },
    InstructionKeyword {
        name: "ENEMY_KICKDRIVER ",
        id: 0x1029,
    },
    InstructionKeyword {
        name: "ENTITY_GROUP_NEW ",
        id: 0x102a,
    },
    InstructionKeyword {
        name: "ENEMY_GROUP_NEW ",
        id: 0x102a,
    },
    InstructionKeyword {
        name: "ENTITY_GROUP_ADD ",
        id: 0x102b,
    },
    InstructionKeyword {
        name: "ENEMY_GROUP_ADD ",
        id: 0x102b,
    },
    InstructionKeyword {
        name: "ENTITY_GROUP_DEL ",
        id: 0x102c,
    },
    InstructionKeyword {
        name: "ENEMY_GROUP_DEL ",
        id: 0x102c,
    },
    InstructionKeyword {
        name: "ENTITY_ARREST_PLAYER ",
        id: 0x102d,
    },
    InstructionKeyword {
        name: "ENEMY_ARREST_PLAYER ",
        id: 0x102d,
    },
    InstructionKeyword {
        name: "ENTITY_NASERSE ",
        id: 0x102e,
    },
    InstructionKeyword {
        name: "ENEMY_NASERSE ",
        id: 0x102e,
    },
    InstructionKeyword {
        name: "ENTITY_ACTIONSCLEAR ",
        id: 0x102f,
    },
    InstructionKeyword {
        name: "ENEMY_ACTIONSCLEAR ",
        id: 0x102f,
    },
    InstructionKeyword {
        name: "ENTITY_BLOCK_CHECKPOINT ",
        id: 0x1030,
    },
    InstructionKeyword {
        name: "ENEMY_BLOCK_CHECKPOINT ",
        id: 0x1030,
    },
    InstructionKeyword {
        name: "ENTITY_SHUT_UP ",
        id: 0x1031,
    },
    InstructionKeyword {
        name: "ENEMY_SHUT_UP ",
        id: 0x1031,
    },
    InstructionKeyword {
        name: "ENTITY_DEBUG_GRAPHS ",
        id: 0x1032,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_CARJUMP ",
        id: 0x1033,
    },
    InstructionKeyword {
        name: "ENEMY_PODVADIM_JAK ",
        id: 0x1034,
    },
    InstructionKeyword {
        name: "ENTITY_PODVADIM_JAK ",
        id: 0x1034,
    },
    InstructionKeyword {
        name: "ENEMY_WATCH ",
        id: 0x1035,
    },
    InstructionKeyword {
        name: "ENTITY_WATCH ",
        id: 0x1035,
    },
    InstructionKeyword {
        name: "ENEMY_GROUP_ADDCAR ",
        id: 0x1036,
    },
    InstructionKeyword {
        name: "ENTITY_GROUP_ADDCAR ",
        id: 0x1036,
    },
    InstructionKeyword {
        name: "ENTITY_GROUP_CHCIPNI_HAJZLE ",
        id: 0x1037,
    },
    InstructionKeyword {
        name: "ENEMY_GROUP_CHCIPNI_HAJZLE ",
        id: 0x1037,
    },
    InstructionKeyword {
        name: "ENEMY_NAROHYSEVYSER ",
        id: 0x1038,
    },
    InstructionKeyword {
        name: "ENTITY_NAROHYSEVYSER ",
        id: 0x1038,
    },
    InstructionKeyword {
        name: "ENTITY_ACTION_PHONECALL ",
        id: 0x1039,
    },
    InstructionKeyword {
        name: "ENEMY_ACTION_PHONECALL ",
        id: 0x1039,
    },
    InstructionKeyword {
        name: "ENTITY_NAPRDELIVAUTE ",
        id: 0x103a,
    },
    InstructionKeyword {
        name: "ENEMY_NAPRDELIVAUTE ",
        id: 0x103a,
    },
    InstructionKeyword {
        name: "ENTITY_FOLLOWPLAYER ",
        id: 0x103b,
    },
    InstructionKeyword {
        name: "ENEMY_FOLLOWPLAYER ",
        id: 0x103b,
    },
    InstructionKeyword {
        name: "ENTITY_AZ_PO_PANICE_HOVNO ",
        id: 0x103c,
    },
    InstructionKeyword {
        name: "ENEMY_AZ_PO_PANICE_HOVNO ",
        id: 0x103c,
    },
    InstructionKeyword {
        name: "ENEMY_NEVYHAZUJ ",
        id: 0x103d,
    },
    InstructionKeyword {
        name: "ENEMY_NEVYHAZUJ ",
        id: 0x103d,
    },
    InstructionKeyword {
        name: "ENEMY_IGNORE_CORPSE ",
        id: 0x103e,
    },
    InstructionKeyword {
        name: "ENTITY_IGNORE_CORPSE ",
        id: 0x103e,
    },
    InstructionKeyword {
        name: "ENEMY_BLASTFIRE ",
        id: 0x103f,
    },
    InstructionKeyword {
        name: "ENTITY_BLASTFIRE ",
        id: 0x103f,
    },
];

/// Original DecodeInstruction under explicit ASCII/C-locale admission.
///
/// Inspect only the first NUL-terminated string. A bare keyword or a keyword
/// followed by an ASCII space matches case-insensitively; tab is not a keyword
/// separator and leading spaces are not trimmed here. Return the first matching
/// entry's ID, or -1 if no entry matches.
///
/// Every eligible entry writes its comparison length before comparison, including
/// unsuccessful comparisons. An unknown instruction can therefore change the
/// caller's output; if no entry is eligible, that output retains its prior value.
/// Missing NUL, empty input, excessive length and non-ASCII bytes are admission
/// errors before any output write. Empty input is excluded because the native
/// routine reads before its start. The input and bytes after its NUL are untouched.
pub fn decode_instruction(
    source: &[u8],
    compared_length: &mut usize,
    locale: ParserLocale,
) -> Result<i32, ProgramParserError> {
    decode_table(source, compared_length, locale, &INSTRUCTION_KEYWORDS)
}

/// Original C_entity::DecodeInstruction recognition (GAME 0x51cdd0) under the
/// same ASCII/C-locale admission as the base decoder.
///
/// The entity table is scanned first in table order; a miss falls back to the
/// base table. The fallback is inferred, not directly evidenced: embedded
/// entity programs mix base commands (LABEL/GOTO/WAIT/DIM_ACT/...) with entity
/// commands, and entity DecodeParams explicitly delegates unknown IDs to base
/// DecodeParams (GAME 0x461c70), so entity sources must resolve both. The two
/// namespaces are disjoint (entity names start with ENEMY_/ENTITY_/
/// COMPAREOWNERWITH; the shared COMPAREOWNERWITH/COMPAREOWNERWITHEX prefix
/// pair provably diverges before either terminator), making the scan order
/// unobservable on admitted inputs.
pub fn decode_entity_instruction(
    source: &[u8],
    compared_length: &mut usize,
    locale: ParserLocale,
) -> Result<i32, ProgramParserError> {
    match decode_table(
        source,
        compared_length,
        locale,
        &ENTITY_INSTRUCTION_KEYWORDS,
    )? {
        -1 => decode_table(source, compared_length, locale, &INSTRUCTION_KEYWORDS),
        id => Ok(id),
    }
}

fn decode_table(
    source: &[u8],
    compared_length: &mut usize,
    locale: ParserLocale,
    keywords: &[InstructionKeyword],
) -> Result<i32, ProgramParserError> {
    let ParserLocale::AsciiC = locale;
    let length = c_length(source)?;
    if length == 0 {
        return Err(ProgramParserError::EmptyInstruction);
    }
    if let Some((offset, &byte)) = source[..length]
        .iter()
        .enumerate()
        .find(|(_, byte)| !byte.is_ascii())
    {
        return Err(ProgramParserError::UnsupportedCrtByte { offset, byte });
    }
    let input = &source[..length];
    let bare_allowance = usize::from(input[length - 1] != b' ');
    for keyword in keywords {
        let name = keyword.name.as_bytes();
        if length < name.len() - bare_allowance {
            continue;
        }
        *compared_length = name.len().min(length);
        if input[..*compared_length].eq_ignore_ascii_case(&name[..*compared_length]) {
            return Ok(keyword.id);
        }
    }
    Ok(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(input: &[u8], length: &mut usize) -> i32 {
        decode_instruction(input, length, ParserLocale::AsciiC).unwrap()
    }

    #[test]
    fn registry_is_complete_unique_and_preserves_original_order() {
        let mut seen = [false; 398];
        for keyword in INSTRUCTION_KEYWORDS {
            assert!(keyword.name.is_ascii());
            assert!(keyword.name.ends_with(' '));
            assert!(!keyword.name[..keyword.name.len() - 1].contains(' '));
            assert!(keyword.name.len() < 32);
            assert!(!seen[keyword.id as usize]);
            seen[keyword.id as usize] = true;
        }
        assert!(seen.into_iter().all(|value| value));
        assert_eq!(
            INSTRUCTION_KEYWORDS[14],
            InstructionKeyword {
                name: "FINDACTOR ",
                id: 18
            }
        );
        assert_eq!(
            INSTRUCTION_KEYWORDS[15],
            InstructionKeyword {
                name: "FRM_SETON ",
                id: 14
            }
        );
        assert_eq!(
            INSTRUCTION_KEYWORDS[390],
            InstructionKeyword {
                name: "FRM_ISON ",
                id: 16
            }
        );
        assert_eq!(
            INSTRUCTION_KEYWORDS[397],
            InstructionKeyword {
                name: "VECT_ROTATE_2D ",
                id: 275
            }
        );
    }

    #[test]
    fn every_keyword_accepts_bare_spaced_arguments_and_mixed_case() {
        for keyword in INSTRUCTION_KEYWORDS {
            let name = keyword.name.as_bytes();
            for (bytes, expected_length) in [
                (&name[..name.len() - 1], name.len() - 1),
                (name, name.len()),
            ] {
                let mut input = bytes.to_vec();
                input.push(0);
                let mut length = usize::MAX;
                assert_eq!(decode(&input, &mut length), keyword.id, "{}", keyword.name);
                assert_eq!(length, expected_length);
            }
            let mut input = name.to_vec();
            for (index, byte) in input.iter_mut().enumerate() {
                if index % 2 == 0 {
                    *byte = byte.to_ascii_lowercase();
                }
            }
            input.extend_from_slice(b"arg, 17\0");
            let mut length = usize::MAX;
            assert_eq!(decode(&input, &mut length), keyword.id, "{}", keyword.name);
            assert_eq!(length, name.len());
        }
    }

    #[test]
    fn every_keyword_requires_its_exact_nonspace_boundary() {
        for keyword in INSTRUCTION_KEYWORDS {
            for separator in *b"X\t\r\n" {
                let mut input = keyword.name.as_bytes()[..keyword.name.len() - 1].to_vec();
                input.extend_from_slice(&[separator, 0]);
                let mut length = usize::MAX;
                assert_eq!(
                    decode(&input, &mut length),
                    -1,
                    "{} {separator:?}",
                    keyword.name
                );
            }
        }
    }

    #[test]
    fn recognition_keeps_prefixes_and_close_names_distinct() {
        for (input, expected, expected_length) in [
            (&b"FRM_SETON\0"[..], 14, 9),
            (b"FRM_SETON1\0", 15, 10),
            (b"PLAYsoundstop a\0", 22, 14),
            (b"IF\0", 9, 2),
            (b"I\0", -1, 1),
            (b" IF\0", -1, 2),
        ] {
            let mut length = usize::MAX;
            assert_eq!(decode(input, &mut length), expected);
            assert_eq!(length, expected_length);
        }
    }

    #[test]
    fn failures_keep_the_last_eligible_comparison_write() {
        let mut length = 991;
        assert_eq!(decode(b" \0", &mut length), -1);
        assert_eq!(length, 991); // No entry's terminal space fits.
        assert_eq!(decode(b"x\0", &mut length), -1);
        assert_eq!(length, 1); // Both one-character punctuation rows are tried.
        assert_eq!(decode(b"not_an_instruction_at_all\0", &mut length), -1);
        assert_eq!(length, 15); // Last eligible row: VECT_ROTATE_2D plus space.
    }

    fn decode_entity(input: &[u8], length: &mut usize) -> i32 {
        decode_entity_instruction(input, length, ParserLocale::AsciiC).unwrap()
    }

    #[test]
    fn entity_registry_preserves_table_order_ids_and_spellings() {
        assert_eq!(ENTITY_INSTRUCTION_KEYWORDS.len(), 121);
        for keyword in ENTITY_INSTRUCTION_KEYWORDS {
            assert!(keyword.name.is_ascii());
            assert!(keyword.name.ends_with(' '));
            assert!(!keyword.name[..keyword.name.len() - 1].contains(' '));
            assert!((0x1000..=0x103f).contains(&keyword.id));
        }
        // Table head, singles, and tail pin the verified order.
        assert_eq!(
            ENTITY_INSTRUCTION_KEYWORDS[0],
            InstructionKeyword {
                name: "ENEMY_MOVE ",
                id: 0x1000
            }
        );
        assert_eq!(
            ENTITY_INSTRUCTION_KEYWORDS[1],
            InstructionKeyword {
                name: "ENTITY_MOVE ",
                id: 0x1000
            }
        );
        assert_eq!(
            ENTITY_INSTRUCTION_KEYWORDS[18],
            InstructionKeyword {
                name: "COMPAREOWNERWITH ",
                id: 0x1009
            }
        );
        assert_eq!(
            ENTITY_INSTRUCTION_KEYWORDS[19],
            InstructionKeyword {
                name: "ENTITY_FINDCORNER ",
                id: 0x100a
            }
        );
        assert_eq!(
            ENTITY_INSTRUCTION_KEYWORDS[95],
            InstructionKeyword {
                name: "ENTITY_DEBUG_GRAPHS ",
                id: 0x1032
            }
        );
        assert_eq!(
            ENTITY_INSTRUCTION_KEYWORDS[115],
            InstructionKeyword {
                name: "ENEMY_NEVYHAZUJ ",
                id: 0x103d
            }
        );
        assert_eq!(
            ENTITY_INSTRUCTION_KEYWORDS[116],
            InstructionKeyword {
                name: "ENEMY_NEVYHAZUJ ",
                id: 0x103d
            }
        );
        assert_eq!(
            ENTITY_INSTRUCTION_KEYWORDS[120],
            InstructionKeyword {
                name: "ENTITY_BLASTFIRE ",
                id: 0x103f
            }
        );
    }

    #[test]
    fn entity_recognition_shares_pair_ids_and_falls_back_to_base() {
        let mut length = 0;
        // Both spellings of every sampled pair decode identically.
        for (enemy, entity, id) in [
            ("ENEMY_MOVE ", "ENTITY_MOVE ", 0x1000),
            ("ENEMY_TALK ", "ENTITY_TALK ", 0x1006),
            ("ENEMY_USECAR ", "ENTITY_USECAR ", 0x1008),
            ("ENEMY_CAR_HUNT ", "ENTITY_CAR_HUNT ", 0x100c),
            ("ENEMY_VIDIM ", "ENTITY_VIDIM ", 0x101a),
            ("ENEMY_NASERSE ", "ENTITY_NASERSE ", 0x102e),
            ("ENEMY_SHUT_UP ", "ENTITY_SHUT_UP ", 0x1031),
            ("ENEMY_BLASTFIRE ", "ENTITY_BLASTFIRE ", 0x103f),
        ] {
            assert_eq!(
                decode_entity(format!("{enemy}\0").as_bytes(), &mut length),
                id
            );
            assert_eq!(
                decode_entity(format!("{entity}\0").as_bytes(), &mut length),
                id
            );
            // Recognition is case-insensitive like the base table.
            assert_eq!(
                decode_entity(
                    format!("{}\0", enemy.to_ascii_lowercase()).as_bytes(),
                    &mut length
                ),
                id
            );
        }
        // Base commands resolve through the fallback in entity sources.
        assert_eq!(decode_entity(b"LABEL \0", &mut length), 0);
        assert_eq!(decode_entity(b"goto foo\0", &mut length), 1);
        assert_eq!(decode_entity(b"WAIT \0", &mut length), 4);
        // Unknown lines refuse in both tables.
        assert_eq!(decode_entity(b"enemy_no_such_command\0", &mut length), -1);
        // The shared COMPAREOWNERWITH/COMPAREOWNERWITHEX prefix diverges
        // before either terminator: the entity entry must not shadow the
        // base command, which still resolves through the fallback.
        assert_eq!(
            decode_entity(b"COMPAREOWNERWITH 1, 2, 3\0", &mut length),
            0x1009
        );
        assert_eq!(
            decode_entity(b"COMPAREOWNERWITHEX 1, 2\0", &mut length),
            101
        );
        let mut base_length = 0;
        assert_eq!(decode(b"COMPAREOWNERWITH \0", &mut base_length), -1);
    }

    #[test]
    fn c_string_bounds_and_admission_preserve_output_and_input() {
        let input = b"end\0\xffafter";
        let mut length = 991;
        assert_eq!(decode(input, &mut length), 6);
        assert_eq!(length, 3);
        assert_eq!(input, b"end\0\xffafter");
        for (input, expected) in [
            (&b"END"[..], ProgramParserError::MissingTerminator),
            (b"\0END\0", ProgramParserError::EmptyInstruction),
            (
                b"END \xff\0",
                ProgramParserError::UnsupportedCrtByte {
                    offset: 4,
                    byte: 255,
                },
            ),
        ] {
            let mut length = 991;
            let original = input.to_vec();
            assert_eq!(
                decode_instruction(input, &mut length, ParserLocale::AsciiC),
                Err(expected)
            );
            assert_eq!(length, 991);
            assert_eq!(input, original);
        }
    }
}
