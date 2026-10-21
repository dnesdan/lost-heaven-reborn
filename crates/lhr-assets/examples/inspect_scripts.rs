//! Read-only mission validation: emit counts, never proprietary program source/names.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = args.next().ok_or("expected original installation root")?;
    let mission = args.next().ok_or("expected mission asset directory")?;
    let probe = match args.next().as_deref() {
        Some("--control-probe") => true,
        None => false,
        _ => return Err("expected optional --control-probe".into()),
    };
    if !mission
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        || mission.is_empty()
        || mission.len() > 64
        || args.next().is_some()
    {
        return Err("invalid mission name or extra argument".into());
    }
    let installation = lhr_assets::Installation::open(root)?;
    let bytes = installation.read(&format!("missions/{mission}/scene2.bin"))?;
    let scene = lhr_assets::parse_scene2(&bytes)?;
    let mut statements = 0usize;
    let mut commands = std::collections::BTreeSet::new();
    let mut probe_blocked = 0;
    let mut probe_prefix = 0;
    let mut probe_statuses = std::collections::BTreeMap::new();
    for script in &scene.scripts {
        let program = lhr_assets::script::parse_script(&script.code)?;
        if probe {
            let mut vm = lhr_assets::script_vm::ControlVm::new(&script.code)?;
            match vm.tick(16, 256) {
                Ok(result) => {
                    *probe_statuses
                        .entry(format!("{:?}", result.status))
                        .or_insert(0) += 1
                }
                Err(error) if error.0.starts_with("unsupported program command ") => {
                    probe_blocked += 1
                }
                Err(error) => return Err(error.into()),
            }
            if vm.instruction_pointer() > 0 {
                probe_prefix += 1;
            }
        }
        statements += program.statements.len();
        for statement in program.statements {
            commands.insert(statement.command);
        }
    }
    println!(
        "programs={} statements={} command_kinds={} gameplay=unsupported",
        scene.scripts.len(),
        statements,
        commands.len()
    );
    if probe {
        println!(
            "control_probe_ticks=1 delta_units=16 per_program_budget=256 blocked={probe_blocked} advanced_programs={probe_prefix} statuses={probe_statuses:?}"
        );
    }
    Ok(())
}
