class_name SliceMission
extends Node3D

signal changed
var state := "active"
var checkpoint_index := 0
var remaining := 240.0
var failure_reason := ""
var checkpoint_positions: Array[Vector3] = [Vector3(-6, 0, 0), Vector3(0, 0, -52),
	Vector3(52, 0, -52), Vector3(52, 0, 0), Vector3(45, 0, 3)]
var instructions: Array[String] = ["Walk into the amber marker by the car.",
	"Enter the car [E]. Drive north to the first intersection.",
	"Turn right. Reach the east checkpoint.", "Turn right again. Return to the delivery street.",
	"Stop [Space], exit [E], and walk to the last marker."]
var marker: Node3D
var game: Node3D

func _ready() -> void:
	marker = Node3D.new()
	add_child(marker)
	var ring := MeshInstance3D.new()
	var mesh := CylinderMesh.new()
	mesh.top_radius = 3.5
	mesh.bottom_radius = 3.5
	mesh.height = 0.06
	ring.mesh = mesh
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(1.0, 0.64, 0.12, 0.6)
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.emission_enabled = true
	material.emission = Color(0.8, 0.35, 0.03)
	ring.material_override = material
	marker.add_child(ring)
	var flag := Label3D.new()
	flag.text = "CHECKPOINT"
	flag.position.y = 3.6
	flag.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	flag.font_size = 56
	flag.pixel_size = 0.015
	marker.add_child(flag)
	marker.position = checkpoint_positions[0] + Vector3.UP * 0.15

func _physics_process(delta: float) -> void:
	if state != "active" or game == null:
		return
	remaining -= delta
	if remaining <= 0:
		fail("Delivery time expired.")
		return
	if game.vehicle.integrity <= 0:
		fail("The car is wrecked.")
		return
	if game.player.global_position.y < -10:
		fail("The courier fell out of the district.")
		return
	var driving: bool = game.vehicle.driver != null
	var needs_car := checkpoint_index > 0 and checkpoint_index < 4
	var actor_position: Vector3 = game.vehicle.global_position if driving else game.player.global_position
	var distance := Vector2(actor_position.x, actor_position.z).distance_to(Vector2(marker.position.x, marker.position.z))
	if distance < 3.5 and driving == needs_car:
		checkpoint_index += 1
		if checkpoint_index == checkpoint_positions.size():
			state = "complete"
			marker.hide()
		else:
			marker.position = checkpoint_positions[checkpoint_index] + Vector3.UP * 0.15
		changed.emit()

func fail(reason: String) -> void:
	if state != "active":
		return
	state = "failed"
	failure_reason = reason
	marker.hide()
	changed.emit()

func objective() -> String:
	if state == "complete":
		return "DELIVERY COMPLETE — press R / gamepad Y to restart."
	if state == "failed":
		return "FAILED: " + failure_reason + " Press R / gamepad Y to restart."
	return instructions[checkpoint_index]
