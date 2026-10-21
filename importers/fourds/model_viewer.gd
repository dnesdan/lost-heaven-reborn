# Private external GLB inspection tool; excluded from the game's resource allowlist.
extends SceneTree

var model_root: Node3D
var bounds := AABB()
var have_bounds := false
var mesh_count := 0
var surface_count := 0
var skeleton_bones := 0
var blend_shapes := 0
var animation_names: Array = []

func _initialize() -> void:
	call_deferred("build_scene")

func fail(message: String) -> void:
	push_error(message)
	quit(1)

func inspect_node(node: Node) -> void:
	if node is Skeleton3D:
		skeleton_bones += (node as Skeleton3D).get_bone_count()
	if node is AnimationPlayer:
		for animation_name in (node as AnimationPlayer).get_animation_list():
			animation_names.append(animation_name)
	if node is MeshInstance3D:
		var mesh := node as MeshInstance3D
		mesh_count += 1
		surface_count += mesh.mesh.get_surface_count()
		blend_shapes += mesh.mesh.get_blend_shape_count()
		var box := mesh.global_transform * mesh.get_aabb()
		bounds = bounds.merge(box) if have_bounds else box
		have_bounds = true
	for child in node.get_children():
		inspect_node(child)

func self_contained_glb(source: String) -> bool:
	# Refuse external URI references before GLTFDocument can access them.
	var file := FileAccess.open(source, FileAccess.READ)
	if file == null or file.get_length() < 28 or file.get_length() > 268435456:
		return false
	if file.get_32() != 0x46546c67 or file.get_32() != 2 or file.get_32() != file.get_length():
		return false
	var json_size := file.get_32()
	if file.get_32() != 0x4e4f534a or json_size > file.get_length() - 20:
		return false
	var parsed: Variant = JSON.parse_string(file.get_buffer(json_size).get_string_from_utf8())
	if not parsed is Dictionary:
		return false
	for collection in [parsed.get("buffers", []), parsed.get("images", [])]:
		if not collection is Array:
			return false
		for entry in collection:
			if not entry is Dictionary or entry.has("uri"):
				return false
	return true

func private_filesystem_path(value: String, repository: String) -> bool:
	# Direct/headless use must enforce the same policy as the Python wrapper.
	# Require physical absolute paths: resolving a symlink after a lexical
	# simplify_path check could access or create a file inside the checkout.
	if not value.is_absolute_path() or value.contains("://"):
		return false
	var normalized := value.replace("\\", "/")
	var components := normalized.split("/", false)
	for component in components:
		if component == "." or component == "..":
			return false
	var simple := normalized.simplify_path().to_lower()
	var checkout := repository.replace("\\", "/").to_lower()
	if simple == checkout or simple.begins_with(checkout + "/"):
		return false
	var current := "/"
	var first_component := 0
	if not normalized.begins_with("/"):
		# Windows drive roots are physical filesystem paths; resource and other
		# URI schemes have already been rejected above.
		if components.is_empty() or components[0].length() != 2 or not components[0].ends_with(":"):
			return false
		current = components[0] + "/"
		first_component = 1
	elif normalized.begins_with("//"):
		return false
	for index in range(first_component, components.size()):
		var directory := DirAccess.open(current)
		if directory == null or directory.is_link(components[index]):
			return false
		if index < components.size() - 1 and not directory.dir_exists(components[index]):
			return false
		current = current.path_join(components[index])
	return true

func build_scene() -> void:
	var args := OS.get_cmdline_user_args()
	if args.size() < 2 or args.size() > 3 or (args.size() == 3 and args[2] != "--verify-only"):
		fail("Expected an absolute external GLB path, external JSON report path and optional --verify-only.")
		return
	var source := args[0]
	var report_path := args[1]
	var repository := ProjectSettings.globalize_path("res://").trim_suffix("/")
	for external in [source, report_path]:
		if not private_filesystem_path(external, repository):
			fail("Model inputs and reports require physical absolute paths outside the checkout without symlink components.")
			return
	if FileAccess.file_exists(report_path):
		fail("Refusing to overwrite an existing model report.")
		return
	if not self_contained_glb(source):
		fail("Only self-contained GLB files without external buffer or image URIs are supported.")
		return
	var document := GLTFDocument.new()
	var state := GLTFState.new()
	var compression_key := "IMPORT_FORCE_DISABLE_MESH_COMPRESSION"
	if not ClassDB.class_has_integer_constant("EditorSceneFormatImporter", compression_key):
		fail("Godot cannot guarantee an import without mesh compression.")
		return
	var import_flags := ClassDB.class_get_integer_constant("EditorSceneFormatImporter", compression_key)
	if document.append_from_file(source, state, import_flags) != OK:
		fail("GLB could not be loaded.")
		return
	model_root = Node3D.new()
	root.add_child(model_root)
	# Experimental clips remain provisional; 100fps preserves a chosen 25fps
	# source's key boundaries. This inspector displays the rest pose only.
	var model := document.generate_scene(state, 100.0)
	if model == null:
		fail("GLB did not produce a scene.")
		return
	model_root.add_child(model)
	inspect_node(model)
	if not have_bounds or bounds.size.length() < 0.00001:
		fail("GLB has no inspectable geometry.")
		return
	var verify_only := args.size() == 3
	if not verify_only:
		var environment := WorldEnvironment.new()
		environment.environment = Environment.new()
		environment.environment.background_mode = Environment.BG_COLOR
		environment.environment.background_color = Color(0.055, 0.07, 0.09)
		environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
		environment.environment.ambient_light_color = Color(0.75, 0.8, 0.9)
		environment.environment.ambient_light_energy = 0.7
		model_root.add_child(environment)
		var light := DirectionalLight3D.new()
		light.rotation_degrees = Vector3(-40, -30, 0)
		light.light_energy = 1.4
		model_root.add_child(light)
		var camera := Camera3D.new()
		model_root.add_child(camera)
		camera.fov = 40
		camera.near = maxf(bounds.size.length() / 10000, 0.001)
		camera.far = maxf(bounds.size.length() * 10, 100)
		var center := bounds.get_center()
		camera.position = center + Vector3(1.3, 0.65, 1.7).normalized() * bounds.size.length() * 1.55
		camera.look_at(center)
		camera.current = true
		for _frame in 12:
			await process_frame
		await RenderingServer.frame_post_draw
	var report := {"check": "structure" if verify_only else "rendered", "glb_nodes": state.get_nodes().size(),
		"glb_meshes": state.get_meshes().size(), "glb_materials": state.get_materials().size(),
		"mesh_instances": mesh_count, "surfaces": surface_count,
		"skeleton_bones": skeleton_bones, "blend_shapes": blend_shapes,
		"animation_names": animation_names, "check_pose": "rest", "gltf_import_bake_fps": 100.0,
		"mesh_compression": "disabled", "gltf_import_flags": import_flags,
		"bounds_position": [bounds.position.x, bounds.position.y, bounds.position.z],
		"bounds_size": [bounds.size.x, bounds.size.y, bounds.size.z]}
	if not verify_only:
		report.merge({"rendering_method": RenderingServer.get_current_rendering_method(),
			"rendering_driver": RenderingServer.get_current_rendering_driver_name(),
			"video_adapter": RenderingServer.get_video_adapter_name()})
	var report_file := FileAccess.open(report_path, FileAccess.WRITE)
	if report_file == null:
		fail("Private model report could not be created.")
		return
	report_file.store_string(JSON.stringify(report, "  "))
	report_file.close()
	print("LHR_MODEL_CHECK " + JSON.stringify(report))
	if verify_only:
		quit(0)
	else:
		print("LHR_RENDER_READY")
