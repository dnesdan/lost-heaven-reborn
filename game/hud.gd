extends CanvasLayer

var game: Node3D
var objective_label: Label
var status_label: Label
var hint_label: Label
var pause_panel: PanelContainer
var touch_controls: Control

func _ready() -> void:
	process_mode = Node.PROCESS_MODE_ALWAYS
	var root := Control.new()
	root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(root)
	var panel := PanelContainer.new()
	panel.position = Vector2(16, 16)
	panel.size = Vector2(760, 140)
	panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(panel)
	var column := VBoxContainer.new()
	panel.add_child(column)
	var title := Label.new()
	title.text = "LOST HEAVEN REBORN  |  SYNTHETIC PLAYABLE SLICE"
	title.add_theme_font_size_override("font_size", 22)
	column.add_child(title)
	objective_label = Label.new()
	objective_label.add_theme_font_size_override("font_size", 20)
	column.add_child(objective_label)
	status_label = Label.new()
	column.add_child(status_label)
	hint_label = Label.new()
	hint_label.text = "WASD move/drive · mouse/right stick camera · E/A enter/exit · Space/B brake · R/Y restart · Esc/Start pause"
	hint_label.add_theme_font_size_override("font_size", 14)
	column.add_child(hint_label)
	var notice := Label.new()
	notice.name = "Notice"
	notice.position = Vector2(18, 166)
	notice.add_theme_font_size_override("font_size", 19)
	root.add_child(notice)
	pause_panel = PanelContainer.new()
	pause_panel.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	pause_panel.position = Vector2(-190, -100)
	pause_panel.custom_minimum_size = Vector2(380, 200)
	root.add_child(pause_panel)
	var pause_column := VBoxContainer.new()
	pause_panel.add_child(pause_column)
	var pause_title := Label.new()
	pause_title.text = "PAUSED"
	pause_title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	pause_title.add_theme_font_size_override("font_size", 28)
	pause_column.add_child(pause_title)
	for entry: Array in [["Resume", game.toggle_pause], ["Restart delivery", game.restart], ["Quit", get_tree().quit]]:
		var button := Button.new()
		button.text = entry[0]
		button.custom_minimum_size.y = 42
		button.pressed.connect(entry[1])
		pause_column.add_child(button)
	var touch_toggle := Button.new()
	touch_toggle.text = "Touch controls"
	touch_toggle.set_anchors_and_offsets_preset(Control.PRESET_TOP_RIGHT)
	touch_toggle.position = Vector2(-150, 16)
	touch_toggle.size = Vector2(134, 40)
	root.add_child(touch_toggle)
	touch_controls = Control.new()
	touch_controls.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	touch_controls.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(touch_controls)
	touch_controls.visible = OS.has_feature("mobile")
	touch_toggle.pressed.connect(func(): touch_controls.visible = not touch_controls.visible; SliceInput.release_touch())
	# TouchScreenButton supports simultaneous fingers and writes the same Input actions.
	for entry: Array in [["move_forward", "↑", Vector2(96, -180), false],
		["move_back", "↓", Vector2(96, -88), false], ["move_left", "←", Vector2(16, -88), false],
		["move_right", "→", Vector2(176, -88), false], ["interact", "ENTER / EXIT", Vector2(-260, -88), true],
		["brake", "BRAKE", Vector2(-140, -88), true], ["pause", "PAUSE", Vector2(-140, -180), true],
		["restart", "RESTART", Vector2(-260, -180), true]]:
		add_touch_button(entry[0], entry[1], entry[2], entry[3])

func add_touch_button(action: String, text: String, offset: Vector2, right: bool) -> void:
	var anchor := Control.new()
	anchor.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT if right else Control.PRESET_BOTTOM_LEFT)
	anchor.mouse_filter = Control.MOUSE_FILTER_IGNORE
	anchor.position = offset
	touch_controls.add_child(anchor)
	var button := TouchScreenButton.new()
	button.action = action
	var shape := RectangleShape2D.new()
	shape.size = Vector2(76 if not right else 108, 68)
	button.shape = shape
	button.shape_centered = false
	anchor.add_child(button)
	var visual := ColorRect.new()
	visual.size = shape.size
	visual.color = Color(0.07, 0.1, 0.13, 0.7)
	visual.mouse_filter = Control.MOUSE_FILTER_IGNORE
	anchor.add_child(visual)
	var label := Label.new()
	label.text = text
	label.size = shape.size
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	anchor.add_child(label)

func _process(_delta: float) -> void:
	objective_label.text = game.mission.objective()
	status_label.text = "Checkpoint %d/5  |  Time %03d s  |  Speed %02d km/h  |  Car %03d%%  |  Handling: prototype" % [
		mini(game.mission.checkpoint_index + 1, 5), maxi(0, ceili(game.mission.remaining)),
		roundi(absf(game.vehicle.speed) * 3.6), roundi(game.vehicle.integrity)]
	get_child(0).get_node("Notice").text = game.notice if game.notice_time > 0 else ""
	pause_panel.visible = get_tree().paused
