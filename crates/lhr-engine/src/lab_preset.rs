//! Portable, strict host-inspection presets. No asset paths or held inputs.
use crate::lab_state::Settings;
use lhr_render::{DebugView, Weather};
use std::collections::BTreeMap;

pub fn encode(settings: &Settings) -> Result<String, String> {
    settings.validate()?;
    let mut text = String::from("lhr_lab_version=1\n");
    macro_rules! scalar {
        ($key:literal,$value:expr) => {
            text.push_str(&format!("{}={}\n", $key, $value));
        };
    }
    macro_rules! vector {
        ($key:literal,$value:expr) => {
            let v = $value;
            text.push_str(&format!("{}={},{},{}\n", $key, v[0], v[1], v[2]));
        };
    }
    scalar!("fov", settings.fov_degrees);
    scalar!("chase_distance", settings.chase.distance);
    scalar!("chase_height", settings.chase.height);
    scalar!("chase_look_ahead", settings.chase.look_ahead);
    scalar!("camera_speed", settings.camera_speed);
    scalar!("auto_orbit", settings.auto_orbit);
    scalar!("turntable_speed", settings.turntable_speed);
    scalar!("animate_preview", settings.animate_preview);
    vector!("translation", settings.transform.translation);
    vector!("rotation", settings.transform.rotation_degrees);
    scalar!("scale", settings.transform.scale);
    let e = &settings.environment;
    scalar!("environment_enabled", e.enabled);
    vector!("background", e.background);
    scalar!("ambient", e.ambient);
    scalar!("sun_intensity", e.sun_intensity);
    scalar!("sun_azimuth", e.sun_azimuth_degrees);
    scalar!("sun_elevation", e.sun_elevation_degrees);
    vector!("sun_color", e.sun_color);
    scalar!("exposure", e.exposure);
    scalar!("saturation", e.saturation);
    scalar!("fog_enabled", e.fog_enabled);
    scalar!("fog_start", e.fog_start);
    scalar!("fog_end", e.fog_end);
    vector!("fog_color", e.fog_color);
    scalar!(
        "weather",
        match e.weather {
            Weather::Clear => "clear",
            Weather::Rain => "rain",
            Weather::Snow => "snow",
        }
    );
    scalar!("precipitation", e.precipitation);
    scalar!("wind", e.wind);
    scalar!(
        "debug_view",
        match e.debug_view {
            DebugView::Source => "source",
            DebugView::Lit => "lit",
            DebugView::Normals => "normals",
            DebugView::Uv => "uv",
        }
    );
    Ok(text)
}

pub fn decode(text: &str) -> Result<Settings, String> {
    if text.len() > 65536 {
        return Err("Lab preset exceeds 64 KiB".into());
    }
    let mut values = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("Invalid preset line {}", index + 1))?;
        let key = key.trim();
        let value = value.trim();
        if values.insert(key, value).is_some() {
            return Err(format!("Duplicate preset key: {key}"));
        }
    }
    if take(&mut values, "lhr_lab_version")? != "1" {
        return Err("Unsupported Lab preset version".into());
    }
    let mut settings = Settings::default();
    macro_rules! scalar {
        ($key:literal,$field:expr) => {
            $field = take(&mut values, $key)?
                .parse()
                .map_err(|_| format!("Invalid preset value: {}", $key))?;
        };
    }
    macro_rules! vector {
        ($key:literal,$field:expr) => {
            $field = parse_vector(take(&mut values, $key)?, $key)?;
        };
    }
    scalar!("fov", settings.fov_degrees);
    scalar!("chase_distance", settings.chase.distance);
    scalar!("chase_height", settings.chase.height);
    scalar!("chase_look_ahead", settings.chase.look_ahead);
    scalar!("camera_speed", settings.camera_speed);
    scalar!("auto_orbit", settings.auto_orbit);
    scalar!("turntable_speed", settings.turntable_speed);
    scalar!("animate_preview", settings.animate_preview);
    vector!("translation", settings.transform.translation);
    vector!("rotation", settings.transform.rotation_degrees);
    scalar!("scale", settings.transform.scale);
    let e = &mut settings.environment;
    scalar!("environment_enabled", e.enabled);
    vector!("background", e.background);
    scalar!("ambient", e.ambient);
    scalar!("sun_intensity", e.sun_intensity);
    scalar!("sun_azimuth", e.sun_azimuth_degrees);
    scalar!("sun_elevation", e.sun_elevation_degrees);
    vector!("sun_color", e.sun_color);
    scalar!("exposure", e.exposure);
    scalar!("saturation", e.saturation);
    scalar!("fog_enabled", e.fog_enabled);
    scalar!("fog_start", e.fog_start);
    scalar!("fog_end", e.fog_end);
    vector!("fog_color", e.fog_color);
    e.weather = match take(&mut values, "weather")? {
        "clear" => Weather::Clear,
        "rain" => Weather::Rain,
        "snow" => Weather::Snow,
        _ => return Err("Unknown preset weather".into()),
    };
    scalar!("precipitation", e.precipitation);
    scalar!("wind", e.wind);
    e.debug_view = match take(&mut values, "debug_view")? {
        "source" => DebugView::Source,
        "lit" => DebugView::Lit,
        "normals" => DebugView::Normals,
        "uv" => DebugView::Uv,
        _ => return Err("Unknown preset debug view".into()),
    };
    if let Some(key) = values.keys().next() {
        return Err(format!("Unknown preset key: {key}"));
    }
    settings.validate()?;
    Ok(settings)
}
fn take<'a>(values: &mut BTreeMap<&'a str, &'a str>, key: &str) -> Result<&'a str, String> {
    values
        .remove(key)
        .ok_or_else(|| format!("Missing preset key: {key}"))
}
fn parse_vector(text: &str, key: &str) -> Result<[f32; 3], String> {
    let values: Vec<_> = text.split(',').collect();
    if values.len() != 3 {
        return Err(format!("Preset {key} requires three components"));
    }
    let mut result = [0.; 3];
    for (output, value) in result.iter_mut().zip(values) {
        *output = value
            .trim()
            .parse()
            .map_err(|_| format!("Invalid preset vector: {key}"))?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_keeps_preview_and_excludes_live_input_and_asset_selection() {
        let mut settings = Settings::default();
        settings.environment.enabled = true;
        settings.environment.weather = Weather::Snow;
        settings.environment.debug_view = DebugView::Normals;
        settings.transform.translation = [1.25, -2., 3.];
        settings.transform.rotation_degrees = [-90., 45., 180.];
        settings.auto_orbit = 12.;
        let expected = settings;
        settings.manual.throttle = 1.;
        settings.manual.steer = -1.;
        settings.control_source = crate::lab_state::ControlSource::Manual;
        settings.selected_draw = Some(12);
        settings.isolate_draw = true;
        settings.hide_selected = true;
        assert_eq!(decode(&encode(&settings).unwrap()).unwrap(), expected);
    }
    #[test]
    fn strict_input_rejects_unknown_duplicate_missing_nonfinite_and_bad_ranges() {
        let text = encode(&Settings::default()).unwrap();
        for invalid in [
            format!("{text}future_key=1\n"),
            format!("{text}fov=60\n"),
            text.replace("lhr_lab_version=1", "lhr_lab_version=2"),
            text.replace("fov=50\n", ""),
            text.replace("fov=50", "fov=NaN"),
            text.replace("scale=1\n", "scale=0\n"),
            text.replace("weather=clear", "weather=hail"),
            text.replace("translation=0,0,0", "translation=1,2,3,4"),
        ] {
            assert!(decode(&invalid).is_err(), "Accepted {invalid}");
        }
        assert!(decode(&"a".repeat(65537)).is_err());
    }
}
