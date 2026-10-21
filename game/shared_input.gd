class_name SliceInput
extends RefCounted
## One action vocabulary for keyboard, gamepad and HUD touch buttons.

static var installed := false

static func install() -> void:
	if installed:
		return
	installed = true
	var keys := {
		"move_forward": [KEY_W, KEY_UP], "move_back": [KEY_S, KEY_DOWN],
		"move_left": [KEY_A, KEY_LEFT], "move_right": [KEY_D, KEY_RIGHT],
		"interact": [KEY_E], "brake": [KEY_SPACE], "pause": [KEY_ESCAPE, KEY_P],
		"restart": [KEY_R], "sprint": [KEY_SHIFT], "look_left": [],
		"look_right": [], "look_up": [], "look_down": [],
	}
	for action: String in keys:
		if not InputMap.has_action(action):
			InputMap.add_action(action, 0.18)
		for key: int in keys[action]:
			var event := InputEventKey.new()
			event.physical_keycode = key
			InputMap.action_add_event(action, event)
	for entry: Array in [["interact", JOY_BUTTON_A], ["brake", JOY_BUTTON_B],
		["pause", JOY_BUTTON_START], ["restart", JOY_BUTTON_Y], ["sprint", JOY_BUTTON_LEFT_STICK]]:
		var event := InputEventJoypadButton.new()
		event.button_index = entry[1]
		InputMap.action_add_event(entry[0], event)
	for entry: Array in [["move_left", JOY_AXIS_LEFT_X, -1.0], ["move_right", JOY_AXIS_LEFT_X, 1.0],
		["move_forward", JOY_AXIS_LEFT_Y, -1.0], ["move_back", JOY_AXIS_LEFT_Y, 1.0],
		["look_left", JOY_AXIS_RIGHT_X, -1.0], ["look_right", JOY_AXIS_RIGHT_X, 1.0],
		["look_up", JOY_AXIS_RIGHT_Y, -1.0], ["look_down", JOY_AXIS_RIGHT_Y, 1.0]]:
		var event := InputEventJoypadMotion.new()
		event.axis = entry[1]
		event.axis_value = entry[2]
		InputMap.action_add_event(entry[0], event)

static func release_touch() -> void:
	for action: StringName in ["move_forward", "move_back", "move_left", "move_right", "brake", "sprint"]:
		Input.action_release(action)
