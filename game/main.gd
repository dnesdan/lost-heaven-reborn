extends Node3D

var player: SlicePlayer
var vehicle: SliceVehicle
var mission: SliceMission
var hud: CanvasLayer
var camera_rig: Node3D
var spring_arm: SpringArm3D
var camera: Camera3D
var yaw := 0.0
var pitch := -0.22
var touch_look_id := -1
var notice := ""
var notice_time := 0.0

func _ready() -> void:
	process_mode = Node.PROCESS_MODE_ALWAYS
	SliceInput.install()
	var world := Node3D.new()
	world.name = "World"
	world.process_mode = Node.PROCESS_MODE_PAUSABLE
	add_child(world)
	var district := SliceDistrict.new()
	world.add_child(district)
	var environment := WorldEnvironment.new()
	var settings := Environment.new()
	settings.background_mode = Environment.BG_COLOR
	settings.background_color = Color("839ca9")
	settings.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	settings.ambient_light_color = Color("bbc6d0")
	settings.ambient_light_energy = 0.7
	settings.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	environment.environment = settings
	world.add_child(environment)
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-48, -32, 0)
	sun.light_color = Color("ffe2b3")
	sun.light_energy = 1.35
	sun.shadow_enabled = true
	world.add_child(sun)
	player = SlicePlayer.new()
	player.name = "Player"
	player.position = Vector3(-9, 0.1, 8)
	world.add_child(player)
	vehicle = SliceVehicle.new()
	vehicle.name = "Vehicle"
	vehicle.position = Vector3(0, 0.1, 5)
	world.add_child(vehicle)
	mission = SliceMission.new()
	mission.name = "Mission"
	mission.game = self
	world.add_child(mission)
	camera_rig = Node3D.new()
	world.add_child(camera_rig)
	spring_arm = SpringArm3D.new()
	spring_arm.spring_length = 7.0
	spring_arm.margin = 0.25
	spring_arm.collision_mask = 1
	var camera_shape := SphereShape3D.new()
	camera_shape.radius = 0.25
	spring_arm.shape = camera_shape
	camera_rig.add_child(spring_arm)
	camera = Camera3D.new()
	camera.fov = 68
	camera.far = 400
	spring_arm.add_child(camera)
	camera.current = true
	camera_rig.position = player.position + Vector3.UP * 1.55
	hud = preload("res://game/hud.gd").new()
	hud.game = self
	add_child(hud)
	if not DisplayServer.get_name() == "headless" and not OS.has_feature("mobile"):
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
	if OS.get_cmdline_user_args().has("--capture-check"):
		_report_render_ready.call_deferred()

func _report_render_ready() -> void:
	await RenderingServer.frame_post_draw
	await RenderingServer.frame_post_draw
	print("LHR_RENDER_READY")

func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("pause"):
		toggle_pause()
		get_viewport().set_input_as_handled()
	elif event.is_action_pressed("restart"):
		get_viewport().set_input_as_handled()
		restart()
	elif not get_tree().paused and event.is_action_pressed("interact"):
		interact()
	elif not get_tree().paused and event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		look(event.relative * 0.003)
	elif event is InputEventScreenTouch:
		if event.pressed and event.position.x > get_viewport().get_visible_rect().size.x * 0.45:
			touch_look_id = event.index
		elif not event.pressed and event.index == touch_look_id:
			touch_look_id = -1
	elif event is InputEventScreenDrag and event.index == touch_look_id and not get_tree().paused:
		look(event.relative * 0.005)

func look(amount: Vector2) -> void:
	yaw -= amount.x
	pitch = clampf(pitch - amount.y, -1.1, 0.30)

func _process(delta: float) -> void:
	if get_tree().paused:
		return
	look(Input.get_vector("look_left", "look_right", "look_up", "look_down") * delta * 2.0)
	player.camera_yaw = yaw
	var target := vehicle.global_position + Vector3.UP * 2.0 if vehicle.driver else player.global_position + Vector3.UP * 1.55
	camera_rig.global_position = camera_rig.global_position.lerp(target, minf(1, delta * 12))
	camera_rig.rotation = Vector3(pitch, yaw, 0)
	spring_arm.spring_length = 9.0 if vehicle.driver else 6.2
	if vehicle.driver:
		player.global_position = vehicle.global_position
	notice_time = maxf(0, notice_time - delta)
	if mission.state == "failed":
		player.enabled = false
		vehicle.driver = null

func interact() -> void:
	if mission.state != "active" or get_tree().paused:
		return
	if absf(vehicle.speed) > 1.5:
		show_notice("Stop the car before entering or exiting.")
		return
	if vehicle.driver:
		var exit_position := vehicle.safe_exit_position()
		if exit_position == Vector3.INF:
			show_notice("Both doors are blocked. Move the car.")
			return
		vehicle.driver = null
		player.global_position = exit_position
		player.enabled = true
		player.show()
		player.set_collision_layer_value(2, true)
		player.velocity = Vector3.ZERO
	elif player.global_position.distance_to(vehicle.global_position) < 4.0:
		vehicle.driver = player
		player.enabled = false
		player.hide()
		player.set_collision_layer_value(2, false)
		show_notice("Driving prototype: W/S throttle, A/D steer, Space brake.")
	else:
		show_notice("Walk closer to the car to enter.")

func show_notice(value: String) -> void:
	notice = value
	notice_time = 3.0

func toggle_pause() -> void:
	get_tree().paused = not get_tree().paused
	SliceInput.release_touch()
	touch_look_id = -1
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE if get_tree().paused else Input.MOUSE_MODE_CAPTURED

func restart() -> void:
	get_tree().paused = false
	SliceInput.release_touch()
	# Scene replacement removes this node; finish input/signal dispatch first.
	get_tree().reload_current_scene.call_deferred()

func _notification(what: int) -> void:
	if what == NOTIFICATION_APPLICATION_FOCUS_OUT and is_inside_tree() and hud != null:
		if not get_tree().paused:
			toggle_pause()
