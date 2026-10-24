//! Complete bounded semantic registry reached by C_car::Init's selector.
//! Original InitAnimIndex scans a larger compiled registry; these 20 distinct
//! IDs and names cover every result of GetAnimIDForUseCar, not all Game actors.
use crate::car_effects::{CarAnimationResource, CarTrackError, CarTrackResources};

pub struct CarInitAnimationCatalog;
impl CarInitAnimationCatalog {
    pub fn resource_name(id: u32) -> Result<String, CarTrackError> {
        if let Some(bits) = id.checked_sub(0x10f).filter(|bits| bits & !0x17 == 0) {
            let size = if bits & 4 == 0 { "Sm" } else { "Big" };
            let action = if bits & 16 == 0 { "Nas" } else { "Vys" };
            let side = ["FL", "FR", "BL", "BR"][(bits & 3) as usize];
            return Ok(format!("Auto{size}{action} {side}.i3d"));
        }
        if let Some(bits) = id.checked_sub(0x13c).filter(|bits| *bits < 4) {
            let action = if bits & 1 == 0 { "Nastup" } else { "Vystup" };
            let side = if bits & 2 == 0 { "L" } else { "R" };
            return Ok(format!("Formule{action}{side}.i3d"));
        }
        Err(CarTrackError::UnsupportedAnimationId(id))
    }
    pub fn resources() -> Result<CarTrackResources, CarTrackError> {
        let ids = (0x10f..=0x116).chain(0x11f..=0x126).chain(0x13c..=0x13f);
        let catalog = ids
            .map(|id| {
                Self::resource_name(id).map(|name| CarAnimationResource {
                    id,
                    name: name.into_bytes(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CarTrackResources::from_unique_catalog(catalog)?.restrict_to_supplied_catalog())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_catalog_keeps_outside_ids_unavailable() {
        let catalog = CarInitAnimationCatalog::resources().unwrap();
        for (id, name) in [
            (0x10f, "AutoSmNas FL.i3d"),
            (0x116, "AutoBigNas BR.i3d"),
            (0x11f, "AutoSmVys FL.i3d"),
            (0x126, "AutoBigVys BR.i3d"),
            (0x13c, "FormuleNastupL.i3d"),
            (0x13f, "FormuleVystupR.i3d"),
        ] {
            assert_eq!(
                catalog.animation_name(id).unwrap().unwrap(),
                name.as_bytes()
            );
        }
        assert_eq!(
            catalog.animation_name(0x117),
            Err(CarTrackError::UnsupportedAnimationId(0x117))
        );
    }
}
