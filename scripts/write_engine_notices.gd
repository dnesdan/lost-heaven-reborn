extends SceneTree
## Export the exact embedded engine/dependency license notices beside the package.

func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	if args.size() != 1:
		push_error("Expected one output path for engine notices")
		quit(1)
		return
	var output := FileAccess.open(args[0], FileAccess.WRITE)
	if output == null:
		push_error("Cannot create engine notices")
		quit(1)
		return
	output.store_string(Engine.get_license_text() + "\n\nThird-party copyright information:\n")
	output.store_string(JSON.stringify(Engine.get_copyright_info(), "\t") + "\n\n")
	var licenses := Engine.get_license_info()
	var names := licenses.keys()
	names.sort()
	for name in names:
		output.store_string(str(name) + "\n" + str(licenses[name]) + "\n\n")
	output.close()
	quit(0)
