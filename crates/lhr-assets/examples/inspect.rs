//! Read-only private validation helper: never emits source names or data.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("expected model path")?;
    let bytes = lhr_assets::read_asset_file(path)?;
    let model = lhr_assets::parse_four_ds(&bytes)?;
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
