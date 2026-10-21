class_name SliceVehicle
extends CharacterBody3D
## Deliberate arcade kinematic prototype, not validated original vehicle physics.

var driver: SlicePlayer
var speed := 0.0
var integrity := 100.0
var steering := 0.0
var damage_cooldown := 0.0
var wheels: Array[MeshInstance3D] = []
const MAX_FORWARD := 24.0
const MAX_REVERSE := 8.0

func _ready() -> void:
	collision_layer = 4
	collision_mask = 3
	var collider := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(2.05, 1.5, 4.4)
	collider.shape = box
	collider.position.y = 0.8
	add_child(collider)
	part(Vector3(1.95, 0.68, 4.3), Vector3(0, 0.9, 0), Color("355e58"))
	part(Vector3(1.7, 0.85, 2.1), Vector3(0, 1.62, 0.25), Color("294847"))
	part(Vector3(1.5, 0.55, 0.04), Vector3(0, 1.65, -0.82), Color("91b0b2"))
	part(Vector3(1.5, 0.52, 0.04), Vector3(0, 1.65, 1.32), Color("91b0b2"))
	part(Vector3(2.12, 0.16, 0.16), Vector3(0, 0.65, -2.24), Color("b1b4b0"))
	part(Vector3(2.12, 0.16, 0.16), Vector3(0, 0.65, 2.24), Color("b1b4b0"))
	for x: float in [-0.73, 0.73]:
		part(Vector3(0.33, 0.26, 0.08), Vector3(x, 1.04, -2.18), Color("f9e7ad"))
		part(Vector3(0.22, 0.17, 0.08), Vector3(x, 0.96, 2.18), Color("a43329"))
	for x: float in [-1.04, 1.04]:
		for z: float in [-1.4, 1.4]:
			var wheel := part(Vector3(0.3, 0.72, 0.72), Vector3(x, 0.48, z), Color("202227"))
			wheels.append(wheel)

func part(size: Vector3, at: Vector3, color: Color) -> MeshInstance3D:
	var item := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	mesh.size = size
	item.mesh = mesh
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.roughness = 0.68
	item.material_override = material
	item.position = at
	add_child(item)
	return item

func _physics_process(delta: float) -> void:
	damage_cooldown = maxf(0.0, damage_cooldown - delta)
	var throttle := Input.get_axis("move_back", "move_forward") if driver else 0.0
	var target_steer := Input.get_axis("move_left", "move_right") if driver else 0.0
	steering = move_toward(steering, target_steer, delta * 3.2)
	var braking := driver != null and Input.is_action_pressed("brake")
	# Opposite throttle brakes to rest before engaging reverse; coasting has drag.
	if braking or (speed * throttle < -0.1):
		speed = move_toward(speed, 0.0, delta * 16.0)
	elif absf(throttle) > 0.05:
		speed = clampf(speed + throttle * delta * 7.0, -MAX_REVERSE, MAX_FORWARD)
	else:
		speed = move_toward(speed, 0.0, delta * (1.5 if driver else 7.0))
	# Bicycle model: reduced steering lock with speed and correct reverse sign.
	var wheel_angle := steering * lerpf(0.58, 0.24, clampf(absf(speed) / MAX_FORWARD, 0, 1))
	rotate_y(-speed / 2.8 * tan(wheel_angle) * delta)
	var forward := -global_transform.basis.z
	velocity.x = forward.x * speed
	velocity.z = forward.z * speed
	velocity.y = -1.0 if is_on_floor() else velocity.y - 24.0 * delta
	var speed_before := absf(speed)
	move_and_slide()
	for i in get_slide_collision_count():
		var collision := get_slide_collision(i)
		if absf(collision.get_normal().y) < 0.55:
			var impact := absf(forward.dot(collision.get_normal())) * speed_before
			if impact > 2.0:
				speed *= 0.15
				if impact > 7.0 and damage_cooldown <= 0:
					integrity = maxf(0, integrity - (impact - 4) * 3.0)
					damage_cooldown = 0.8
	for wheel in wheels:
		if wheel.position.z < 0:
			wheel.rotation.y = -wheel_angle

func safe_exit_position() -> Vector3:
	# Probe both doors; avoid spawning inside a wall, obstacle or the car.
	for side: float in [-1.0, 1.0]:
		var candidate := global_position + global_transform.basis.x * side * 2.3
		candidate.y += 0.15
		var shape := CapsuleShape3D.new()
		shape.radius = 0.4
		shape.height = 1.8
		var query := PhysicsShapeQueryParameters3D.new()
		query.shape = shape
		query.transform = Transform3D(Basis.IDENTITY, candidate + Vector3.UP * 0.9)
		query.collision_mask = 5
		query.exclude = [get_rid()]
		if get_world_3d().direct_space_state.intersect_shape(query).is_empty():
			return candidate
	return Vector3.INF
