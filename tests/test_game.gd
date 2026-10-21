extends SceneTree
## Synthetic gameplay checks. These are headless logic/physics tests, not visual proof.

var passed := 0
var failed := 0
var game: Node3D

# Godot logs engine errors without changing script exit status. Capture them so
# lifecycle regressions fail even if the scene eventually recovers.
class ErrorCapture extends Logger:
	var errors: Array[String] = []
	func _log_error(_function: String, _file: String, _line: int, code: String,
			_rationale: String, _editor_notify: bool, _error_type: int,
			_script_backtraces: Array[ScriptBacktrace]) -> void:
		errors.append(code)

func _initialize() -> void:
	call_deferred("run")

func check(condition: bool, label: String) -> void:
	if condition:
		passed += 1
		print("PASS: ", label)
	else:
		failed += 1
		push_error("FAIL: " + label)

func frames(count: int) -> void:
	for _frame in count:
		await physics_frame

func fresh_game() -> void:
	paused = false
	for action in ["move_forward", "move_back", "move_left", "move_right", "brake", "sprint"]:
		if InputMap.has_action(action):
			Input.action_release(action)
	if is_instance_valid(game):
		game.queue_free()
		await process_frame
	game = load("res://scenes/main.tscn").instantiate()
	root.add_child(game)
	current_scene = game
	await frames(3)

func enter_vehicle() -> void:
	game.player.global_position = game.vehicle.global_position + Vector3(-2.5, 0.2, 0)
	game.interact()
	await frames(2)

func run() -> void:
	var error_capture := ErrorCapture.new()
	OS.add_logger(error_capture)
	await fresh_game()
	check(game.mission.state == "active" and game.mission.checkpoint_index == 0,
		"new game starts active at first checkpoint")
	var initial_position: Vector3 = game.player.global_position
	Input.action_press("move_forward")
	await frames(30)
	Input.action_release("move_forward")
	check(game.player.global_position.distance_to(initial_position) > 1.0,
		"walking input moves the player through physics")

	await fresh_game()
	game.player.global_position = game.mission.checkpoint_positions[0]
	await frames(3)
	check(game.mission.checkpoint_index == 1, "first checkpoint advances on foot")
	game.player.global_position = game.mission.checkpoint_positions[1]
	await frames(3)
	check(game.mission.checkpoint_index == 1, "driving checkpoint rejects a pedestrian")
	await enter_vehicle()
	check(game.vehicle.driver == game.player and not game.player.enabled and not game.player.visible,
		"enter transfers control and hides the walking body")
	for index in range(1, 4):
		game.vehicle.speed = 0.0
		game.vehicle.global_position = game.mission.checkpoint_positions[index]
		await frames(3)
		check(game.mission.checkpoint_index == index + 1,
			"driving checkpoint %d advances in the vehicle" % index)
	game.vehicle.global_position = game.mission.checkpoint_positions[4]
	await frames(3)
	check(game.mission.state == "active", "delivery completion requires leaving the vehicle")
	game.interact()
	check(game.vehicle.driver == null and game.player.enabled and game.player.visible,
		"exit returns visible walking control")
	game.player.global_position = game.mission.checkpoint_positions[4]
	await frames(3)
	check(game.mission.state == "complete", "final foot checkpoint completes the task")

	await fresh_game()
	game.mission.remaining = 0.01
	await frames(3)
	check(game.mission.state == "failed", "expired mission fails")
	await fresh_game()
	game.vehicle.integrity = 0.0
	await frames(3)
	check(game.mission.state == "failed", "destroyed vehicle fails the mission")
	game.restart()
	await frames(5)
	game = current_scene
	check(game.mission.state == "active" and game.mission.checkpoint_index == 0
		and is_equal_approx(game.vehicle.integrity, 100.0),
		"restart reloads progression and vehicle integrity")

	var remaining_before: float = game.mission.remaining
	var position_before: Vector3 = game.player.global_position
	Input.action_press("brake")
	game.toggle_pause()
	check(not Input.is_action_pressed("brake"), "pause releases held shared touch actions")
	Input.action_press("move_forward")
	await frames(10)
	check(paused and is_equal_approx(game.mission.remaining, remaining_before)
		and game.player.global_position.is_equal_approx(position_before),
		"pause freezes timer and walking physics")
	game.toggle_pause()
	await frames(10)
	Input.action_release("move_forward")
	check(not paused and game.mission.remaining < remaining_before,
		"resume restores mission updates")
	Input.action_press("brake")
	game.toggle_pause()
	game.restart()
	await frames(5)
	game = current_scene
	check(not paused and not Input.is_action_pressed("brake") and game.mission.state == "active",
		"restart from pause clears held actions and resumes a fresh mission")
	game.mission.fail("Synthetic input dispatch fixture")
	var restart_key := InputEventKey.new()
	restart_key.physical_keycode = KEY_R
	restart_key.pressed = true
	Input.parse_input_event(restart_key)
	await frames(5)
	restart_key = InputEventKey.new()
	restart_key.physical_keycode = KEY_R
	restart_key.pressed = false
	Input.parse_input_event(restart_key)
	game = current_scene
	check(game.mission.state == "active" and game.mission.checkpoint_index == 0,
		"restart keyboard event dispatch safely replaces the active scene")
	var replacement_count := [0]
	var count_replacement := func(): replacement_count[0] += 1
	scene_changed.connect(count_replacement)
	for _press in 3:
		var repeated_key := InputEventKey.new()
		repeated_key.physical_keycode = KEY_R
		repeated_key.pressed = true
		game._unhandled_input(repeated_key)
	await frames(5)
	game = current_scene
	scene_changed.disconnect(count_replacement)
	check(replacement_count[0] == 1 and game != null and game.mission.state == "active",
		"same-frame restart inputs replace the scene exactly once")
	var active_scene := game
	current_scene = null
	game.restart()
	await frames(2)
	check(current_scene == null and is_instance_valid(active_scene),
		"restart without an active current scene safely ignores the stale request")
	current_scene = active_scene
	game.restart()
	current_scene = null
	root.remove_child(game)
	await frames(2)
	check(current_scene == null and is_instance_valid(game),
		"detached scene ignores its deferred restart request")
	root.add_child(game)
	current_scene = game

	await fresh_game()
	await enter_vehicle()
	var vehicle_start: Vector3 = game.vehicle.global_position
	Input.action_press("move_forward")
	await frames(60)
	Input.action_release("move_forward")
	var speed_before: float = game.vehicle.speed
	check(speed_before > 4.0 and game.vehicle.global_position.distance_to(vehicle_start) > 2.0,
		"throttle accelerates and moves the vehicle")
	Input.action_press("brake")
	await frames(30)
	Input.action_release("brake")
	check(absf(game.vehicle.speed) < speed_before * 0.5,
		"brake reduces speed substantially")
	game.vehicle.speed = 6.0
	var yaw_before: float = game.vehicle.rotation.y
	Input.action_press("move_right")
	await frames(20)
	Input.action_release("move_right")
	check(absf(angle_difference(yaw_before, game.vehicle.rotation.y)) > 0.05,
		"steering changes vehicle heading while moving")
	game.vehicle.speed = 5.0
	game.interact()
	check(game.vehicle.driver == game.player, "exit is rejected at unsafe speed")
	game.vehicle.speed = 0.0
	await frames(2)
	game.interact()
	check(game.vehicle.driver == null, "exit succeeds at rest")

	await fresh_game()
	await enter_vehicle()
	# Isolated physical barrier in an open road tests real collision response.
	var barrier := StaticBody3D.new()
	barrier.collision_layer = 1
	var collider := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(12, 4, 1)
	collider.shape = box
	barrier.add_child(collider)
	game.add_child(barrier)
	barrier.global_position = game.vehicle.global_position + Vector3(0, 1, -5)
	await frames(2)
	game.vehicle.speed = 18.0
	Input.action_press("move_forward")
	await frames(25)
	Input.action_release("move_forward")
	check(game.vehicle.integrity < 100.0 and game.vehicle.global_position.z > barrier.global_position.z,
		"solid collision blocks the vehicle and applies impact damage")

	paused = false
	if is_instance_valid(game):
		game.queue_free()
	await process_frame
	OS.remove_logger(error_capture)
	check(error_capture.errors.is_empty(), "gameplay and restart lifecycle emit no engine errors")
	print("GAME TESTS: %d passed, %d failed, 0 skipped" % [passed, failed])
	quit(1 if failed else 0)
