class_name SlicePlayer
extends CharacterBody3D

var enabled := true
var camera_yaw := 0.0
var walk_phase := 0.0
var left_leg: MeshInstance3D
var right_leg: MeshInstance3D
var visual: Node3D

func _ready() -> void:
	collision_layer = 2
	collision_mask = 5
	var capsule := CapsuleShape3D.new()
	capsule.radius = 0.35
	capsule.height = 1.8
	var collider := CollisionShape3D.new()
	collider.shape = capsule
	collider.position.y = 0.9
	add_child(collider)
	visual = Node3D.new()
	add_child(visual)
	part(Vector3(0.65, 0.72, 0.36), Vector3(0, 1.18, 0), Color("514940"))
	part(Vector3(0.39, 0.40, 0.36), Vector3(0, 1.77, 0), Color("c79368"))
	part(Vector3(0.54, 0.12, 0.48), Vector3(0, 1.98, 0), Color("332e2b"))
	part(Vector3(0.21, 0.66, 0.26), Vector3(-0.45, 1.15, 0), Color("514940"))
	part(Vector3(0.21, 0.66, 0.26), Vector3(0.45, 1.15, 0), Color("514940"))
	left_leg = part(Vector3(0.25, 0.72, 0.28), Vector3(-0.19, 0.42, 0), Color("292c32"))
	right_leg = part(Vector3(0.25, 0.72, 0.28), Vector3(0.19, 0.42, 0), Color("292c32"))

func part(size: Vector3, at: Vector3, color: Color) -> MeshInstance3D:
	var item := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	mesh.size = size
	item.mesh = mesh
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	item.material_override = material
	item.position = at
	visual.add_child(item)
	return item

func _physics_process(delta: float) -> void:
	if not enabled:
		return
	var axes := Input.get_vector("move_left", "move_right", "move_forward", "move_back")
	var direction := Vector3(axes.x, 0, axes.y).rotated(Vector3.UP, camera_yaw)
	var speed := 8.0 if Input.is_action_pressed("sprint") else 4.8
	velocity.x = move_toward(velocity.x, direction.x * speed, delta * 30)
	velocity.z = move_toward(velocity.z, direction.z * speed, delta * 30)
	if not is_on_floor():
		velocity.y -= 24.0 * delta
	else:
		velocity.y = -0.2
	move_and_slide()
	if direction.length() > 0.1:
		visual.rotation.y = lerp_angle(visual.rotation.y, atan2(-direction.x, -direction.z), delta * 12)
		walk_phase += delta * speed * 2.3
	left_leg.rotation.x = sin(walk_phase) * 0.55 * axes.length()
	right_leg.rotation.x = -left_leg.rotation.x
