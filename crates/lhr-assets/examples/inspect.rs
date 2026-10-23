//! Read-only private validation helper: never emits source names or data.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let first = args.next().ok_or("expected asset path")?;
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
