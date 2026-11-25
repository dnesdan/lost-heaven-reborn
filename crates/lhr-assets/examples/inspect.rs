//! Read-only private validation helper: never emits source names or data.

use std::{
    ffi::OsString,
    io::{self, Write},
    process::ExitCode,
};

fn main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().ok_or("expected asset path")?;
    if first == "--check-installation" {
        return Ok(check_installation(args, &mut io::stdout().lock())?);
    }
    inspect_file(first, args)?;
    Ok(ExitCode::SUCCESS)
}

fn check_installation(
    mut args: impl Iterator<Item = OsString>,
    output: &mut impl Write,
) -> io::Result<ExitCode> {
    let (Some(root), Some(first_path)) = (args.next(), args.next()) else {
        writeln!(
            output,
            "usage: inspect --check-installation ROOT ASSET [ASSET ...]"
        )?;
        return Ok(ExitCode::from(2));
    };
    let Ok(installation) = lhr_assets::Installation::open(root) else {
        writeln!(output, "installation-error")?;
        return Ok(ExitCode::from(2));
    };
    let mut all_readable = true;
    for (index, path) in std::iter::once(first_path).chain(args).enumerate() {
        let entry = index + 1;
        match path
            .to_str()
            .map(|name| installation.read_optional_original_path(name))
        {
            Some(Ok(Some(bytes))) => {
                writeln!(
                    output,
                    "entry={entry} status=readable bytes={}",
                    bytes.len()
                )?;
            }
            Some(Ok(None)) => {
                all_readable = false;
                writeln!(output, "entry={entry} status=missing")?;
            }
            Some(Err(_)) | None => {
                all_readable = false;
                writeln!(output, "entry={entry} status=read-error")?;
            }
        }
    }
    Ok(if all_readable {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn inspect_file(
    first: OsString,
    mut args: impl Iterator<Item = OsString>,
) -> Result<(), Box<dyn std::error::Error>> {
    let scene2 = first == "--scene2";
    let cache = first == "--cache";
    let klz = first == "--klz";
    let bmp_original = first == "--bmp-original";
    let world = first == "--world";
    let path = if scene2 || world || cache || klz || bmp_original {
        args.next().ok_or("expected asset path")?
    } else {
        first
    };
    let bytes = lhr_assets::read_asset_file(path)?;
    if klz {
        use lhr_assets::klz::{Klz, PrimitiveKind};
        let collision = Klz::parse(&bytes)?;
        println!(
            "grid={}x{} cells={} links={} world_vertices={}",
            collision.grid.width,
            collision.grid.length,
            collision.cells.len(),
            collision.links.len(),
            collision.world_vertices.len(),
        );
        for (name, kind) in [
            ("triangles", PrimitiveKind::Triangle),
            ("aabbs", PrimitiveKind::Aabb),
            ("xtobbs", PrimitiveKind::XtObb),
            ("cylinders", PrimitiveKind::Cylinder),
            ("obbs", PrimitiveKind::Obb),
            ("spheres", PrimitiveKind::Sphere),
        ] {
            println!("{name}={}", collision.array(kind).len());
        }
        println!("Read-only KLZ intake; geometry binding and collision simulation are separate.");
        return Ok(());
    }
    if bmp_original {
        let t = lhr_assets::decode_bmp_original(&bytes)?;
        println!(
            "texture={}x{} rgba_bytes={}",
            t.width,
            t.height,
            t.rgba.len()
        );
        return Ok(());
    }
    if cache {
        let c = lhr_assets::parse_city_cache(&bytes)?;
        println!(
            "parts={} objects={} unknown_chunks={}",
            c.parts.len(),
            c.parts.iter().map(|p| p.objects.len()).sum::<usize>(),
            c.unknown_chunks
        );
        return Ok(());
    }
    if scene2 {
        let scene = lhr_assets::parse_scene2(&bytes)?;
        println!(
            "objects={} actors={} scripts={} unknown_chunks={}",
            scene.objects.len(),
            scene.actors.len(),
            scene.scripts.len(),
            scene.unknown_chunks
        );
        return Ok(());
    }
    let model = if world {
        lhr_assets::parse_four_ds_scene(&bytes)?
    } else {
        lhr_assets::parse_four_ds(&bytes)?
    };
    let vertices: usize = model
        .frames
        .iter()
        .flat_map(|f| &f.lods)
        .map(|l| l.vertices.len())
        .sum();
    let triangles: usize = model
        .frames
        .iter()
        .flat_map(|f| &f.lods)
        .flat_map(|l| &l.groups)
        .map(|g| g.indices.len() / 3)
        .sum();
    println!(
        "materials={} frames={} vertices={} triangles={}",
        model.materials.len(),
        model.frames.len(),
        vertices,
        triangles
    );
    if let Some(path) = args.next() {
        let texture = lhr_assets::decode_bmp(&lhr_assets::read_asset_file(path)?)?;
        println!(
            "texture={}x{} rgba_bytes={}",
            texture.width,
            texture.height,
            texture.rgba.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct InstallationDirectory(PathBuf);

    impl InstallationDirectory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "lhr-inspect-installation-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }

        fn check(&self, paths: &[&str]) -> (ExitCode, String) {
            let args = std::iter::once(self.0.clone().into_os_string())
                .chain(paths.iter().copied().map(OsString::from));
            let mut output = Vec::new();
            let code = check_installation(args, &mut output).unwrap();
            (code, String::from_utf8(output).unwrap())
        }
    }

    impl Drop for InstallationDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn installed_checks_distinguish_missing_invalid_and_readable_entries_in_order() {
        let fixture = InstallationDirectory::new();
        fs::write(fixture.0.join("not-a-directory"), b"synthetic").unwrap();
        fs::create_dir(fixture.0.join("data")).unwrap();
        fs::write(fixture.0.join("data/asset.bin"), b"sample").unwrap();
        fs::write(fixture.0.join("data/empty.bin"), []).unwrap();
        assert_eq!(
            fixture.check(&[
                "data/missing.bin",
                "not-a-directory/asset.bin",
                "DATA\\ASSET.BIN",
                "../asset.bin",
                "data/empty.bin",
            ]),
            (
                ExitCode::FAILURE,
                concat!(
                    "entry=1 status=missing\n",
                    "entry=2 status=read-error\n",
                    "entry=3 status=readable bytes=6\n",
                    "entry=4 status=read-error\n",
                    "entry=5 status=readable bytes=0\n",
                )
                .into(),
            )
        );
        assert_eq!(
            fixture.check(&["data/asset.bin"]),
            (
                ExitCode::SUCCESS,
                "entry=1 status=readable bytes=6\n".into()
            )
        );
    }

    #[test]
    fn usage_and_installation_failures_are_distinct_from_entry_statuses() {
        let fixture = InstallationDirectory::new();
        assert_eq!(fixture.check(&[]).0, ExitCode::from(2));
        let args = [
            fixture.0.join("missing-root").into_os_string(),
            OsString::from("asset.bin"),
        ];
        let mut output = Vec::new();
        assert_eq!(
            check_installation(args.into_iter(), &mut output).unwrap(),
            ExitCode::from(2)
        );
        assert_eq!(output, b"installation-error\n");
    }
}
