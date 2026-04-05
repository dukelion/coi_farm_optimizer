use crate::AppWindow;

pub fn baseline_editor_range_for_fertilizer(selection: &str) -> (i32, i32) {
    let max = match selection {
        "Fertilizer I" => 120,
        "Fertilizer II" => 140,
        _ => 100,
    };
    (60, max)
}

pub fn quantize_slider_value(value: f32, _min: i32, max: i32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }

    let slider_max = (max - 60 + 10).max(10);
    (((value / 10.0).round() as i32) * 10).clamp(10, slider_max) as f32
}

pub fn slider_value_to_baseline(value: f32, min: i32, max: i32) -> f32 {
    let snapped = quantize_slider_value(value, min, max);
    if snapped <= 0.0 {
        return 0.0;
    }
    let baseline = (snapped as i32 - 10) + min;
    baseline.clamp(min, max) as f32
}

pub fn baseline_to_slider_value(value: f32, min: i32, max: i32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }
    let clamped = value.clamp(min as f32, max as f32) as i32;
    let slider_value = (clamped - min) + 10;
    quantize_slider_value(slider_value as f32, min, max)
}

pub fn current_baseline_label(app: &AppWindow, building: &str) -> Option<String> {
    let value = match building {
        "FarmT1" => app.get_baseline_farm_t1().to_string(),
        "FarmT2" => app.get_baseline_farm_t2().to_string(),
        "FarmT3" => app.get_baseline_farm_t3().to_string(),
        "FarmT4" => app.get_baseline_farm_t4().to_string(),
        _ => return None,
    };
    Some(value)
}

pub fn set_building_baseline_label(app: &AppWindow, building: &str, value: String) {
    match building {
        "FarmT1" => app.set_baseline_farm_t1(value.into()),
        "FarmT2" => app.set_baseline_farm_t2(value.into()),
        "FarmT3" => app.set_baseline_farm_t3(value.into()),
        "FarmT4" => app.set_baseline_farm_t4(value.into()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        baseline_editor_range_for_fertilizer, baseline_to_slider_value, quantize_slider_value,
        slider_value_to_baseline,
    };

    #[test]
    fn slider_round_trip_preserves_baseline_steps() {
        let (min, max) = baseline_editor_range_for_fertilizer("Fertilizer II");
        let slider = baseline_to_slider_value(120.0, min, max);
        assert_eq!(slider_value_to_baseline(slider, min, max), 120.0);
    }

    #[test]
    fn zero_slider_maps_to_off() {
        let (min, max) = baseline_editor_range_for_fertilizer("Organic Fert");
        assert_eq!(quantize_slider_value(-1.0, min, max), 0.0);
        assert_eq!(slider_value_to_baseline(0.0, min, max), 0.0);
    }
}
