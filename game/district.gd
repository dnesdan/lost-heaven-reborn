class_name SliceDistrict
extends Node3D
## Original, deterministic geometry. Runtime has no dependency on private importers.

func _ready() -> void:
	box("Ground", Vector3(260, 0.5, 260), Vector3(0, -0.3, 0), Color("716f63"))
	for coordinate: float in [-104.0, -52.0, 0.0, 52.0, 104.0]:
		box("Road", Vector3(260, 0.08, 12), Vector3(0, 0, coordinate), Color("393c40"), false)
		box("Road", Vector3(12, 0.08, 260), Vector3(coordinate, 0, 0), Color("393c40"), false)
		for offset: float in [-7.0, 7.0]:
			box("Pavement", Vector3(260, 0.18, 1.8), Vector3(0, 0.04, coordinate + offset), Color("a3a291"), false)
			box("Pavement", Vector3(1.8, 0.18, 260), Vector3(coordinate + offset, 0.04, 0), Color("a3a291"), false)
		for stripe: float in range(-120, 121, 10):
			box("LaneMark", Vector3(3.5, 0.01, 0.12), Vector3(stripe, 0.06, coordinate), Color("bdaf7e"), false)
			box("LaneMark", Vector3(0.12, 0.01, 3.5), Vector3(coordinate, 0.06, stripe), Color("bdaf7e"), false)
	var palette: Array[Color] = [Color("956d58"), Color("8f8a70"), Color("6d777a"), Color("ab9575")]
	var index := 0
	for x: float in [-78.0, -26.0, 26.0, 78.0]:
		for z: float in [-78.0, -26.0, 26.0, 78.0]:
			var height := 11.0 + (index % 4) * 3.0
			box("Building", Vector3(32, height, 32), Vector3(x, height / 2, z), palette[index % 4])
			box("Cornice", Vector3(33, 0.5, 33), Vector3(x, height - 0.3, z), Color("c8b89b"), false)
			for floor_number in range(1, int(height / 3)):
				for offset: float in [-11.0, -5.5, 0.0, 5.5, 11.0]:
					for side: float in [-1.0, 1.0]:
						box("Window", Vector3(1.7, 1.7, 0.08), Vector3(x + offset, floor_number * 3, z + side * 16.05), Color("344952"), false)
						box("Window", Vector3(0.08, 1.7, 1.7), Vector3(x + side * 16.05, floor_number * 3, z + offset), Color("344952"), false)
			index += 1
	for x: float in [-8.0, 44.0, 60.0]:
		for z: float in [-60.0, -8.0, 8.0]:
			box("Lamp", Vector3(0.18, 5.0, 0.18), Vector3(x, 2.5, z), Color("252c30"))
			box("LampTop", Vector3(0.7, 0.35, 0.7), Vector3(x, 5.1, z), Color("f4d9a4"), false)
	for boundary in [Vector3(-130, 1.5, 0), Vector3(130, 1.5, 0)]:
		box("Boundary", Vector3(1, 3, 260), boundary, Color("635d50"))
	for boundary in [Vector3(0, 1.5, -130), Vector3(0, 1.5, 130)]:
		box("Boundary", Vector3(260, 3, 1), boundary, Color("635d50"))
	sign_text("RIVERSIDE / SYNTHETIC DISTRICT", Vector3(0, 5, -12), 0)
	sign_text("ORIGINAL PLACEHOLDER GEOMETRY", Vector3(52, 4, -64), 0)

func box(label: String, size: Vector3, at: Vector3, color: Color, collision := true) -> void:
	var body: Node3D = StaticBody3D.new() if collision else Node3D.new()
	body.name = label
	body.position = at
	add_child(body)
	var visual := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	mesh.size = size
	visual.mesh = mesh
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.roughness = 0.95
	visual.material_override = material
	body.add_child(visual)
	if collision:
		var shape := CollisionShape3D.new()
		var box_shape := BoxShape3D.new()
		box_shape.size = size
		shape.shape = box_shape
		body.add_child(shape)

func sign_text(value: String, at: Vector3, yaw: float) -> void:
	var sign_label := Label3D.new()
	sign_label.text = value
	sign_label.font_size = 48
	sign_label.pixel_size = 0.012
	sign_label.position = at
	sign_label.rotation.y = yaw
	sign_label.modulate = Color("eee0bd")
	add_child(sign_label)
