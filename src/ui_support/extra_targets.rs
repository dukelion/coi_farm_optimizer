use std::collections::BTreeMap;

use crate::AppWindow;

pub fn set_extra_target_rows(app: &AppWindow, extra_requirements: &BTreeMap<String, f64>) {
    clear_extra_target_rows(app);
    for (index, (name, value)) in extra_requirements.iter().take(4).enumerate() {
        set_extra_target_slot(app, (index + 1) as i32, Some((name.as_str(), *value)));
    }
    sync_extra_requirements_json(app);
}

pub fn add_or_update_extra_target(app: &AppWindow, target: &str) {
    for index in 1..=4 {
        if extra_target_name(app, index) == target {
            return;
        }
    }
    for index in 1..=4 {
        if !extra_target_visible(app, index) {
            set_extra_target_slot(app, index, Some((target, 1.0)));
            return;
        }
    }
}

pub fn remove_extra_target_at(app: &AppWindow, index: i32) {
    let mut entries = extra_target_entries(app);
    entries.remove((index - 1).max(0) as usize);
    clear_extra_target_rows(app);
    for (slot, (name, value)) in entries.iter().enumerate() {
        set_extra_target_slot(app, (slot + 1) as i32, Some((name.as_str(), *value)));
    }
}

pub fn extra_requirements_from_ui(app: &AppWindow) -> Result<BTreeMap<String, f64>, String> {
    let mut requirements = BTreeMap::new();
    for index in 1..=4 {
        if !extra_target_visible(app, index) {
            continue;
        }
        let name = extra_target_name(app, index);
        let value_text = extra_target_value(app, index);
        let value = value_text
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("Extra target value for {name} must be a number."))?;
        requirements.insert(name, value);
    }
    Ok(requirements)
}

pub fn sync_extra_requirements_json(app: &AppWindow) {
    let json = serde_json::to_string_pretty(
        &extra_target_entries(app)
            .into_iter()
            .collect::<BTreeMap<String, f64>>(),
    )
    .unwrap_or_else(|_| "{}".to_owned());
    app.set_extra_requirements_json(json.into());
}

fn clear_extra_target_rows(app: &AppWindow) {
    set_extra_target_slot(app, 1, None);
    set_extra_target_slot(app, 2, None);
    set_extra_target_slot(app, 3, None);
    set_extra_target_slot(app, 4, None);
}

fn set_extra_target_slot(app: &AppWindow, index: i32, entry: Option<(&str, f64)>) {
    let (visible, name, value) = match entry {
        Some((name, value)) => (true, name.to_owned(), format_number_for_ui(value)),
        None => (false, String::new(), "1".to_owned()),
    };
    match index {
        1 => {
            app.set_extra_target_1_visible(visible);
            app.set_extra_target_1_name(name.into());
            app.set_extra_target_1_value(value.into());
        }
        2 => {
            app.set_extra_target_2_visible(visible);
            app.set_extra_target_2_name(name.into());
            app.set_extra_target_2_value(value.into());
        }
        3 => {
            app.set_extra_target_3_visible(visible);
            app.set_extra_target_3_name(name.into());
            app.set_extra_target_3_value(value.into());
        }
        4 => {
            app.set_extra_target_4_visible(visible);
            app.set_extra_target_4_name(name.into());
            app.set_extra_target_4_value(value.into());
        }
        _ => {}
    }
}

fn extra_target_entries(app: &AppWindow) -> Vec<(String, f64)> {
    let mut entries = Vec::new();
    for index in 1..=4 {
        if !extra_target_visible(app, index) {
            continue;
        }
        let name = extra_target_name(app, index);
        let value_text = extra_target_value(app, index);
        let value = value_text.trim().parse::<f64>().unwrap_or(1.0);
        entries.push((name, value));
    }
    entries
}

fn extra_target_visible(app: &AppWindow, index: i32) -> bool {
    match index {
        1 => app.get_extra_target_1_visible(),
        2 => app.get_extra_target_2_visible(),
        3 => app.get_extra_target_3_visible(),
        4 => app.get_extra_target_4_visible(),
        _ => false,
    }
}

fn extra_target_name(app: &AppWindow, index: i32) -> String {
    match index {
        1 => app.get_extra_target_1_name().to_string(),
        2 => app.get_extra_target_2_name().to_string(),
        3 => app.get_extra_target_3_name().to_string(),
        4 => app.get_extra_target_4_name().to_string(),
        _ => String::new(),
    }
}

fn extra_target_value(app: &AppWindow, index: i32) -> String {
    match index {
        1 => app.get_extra_target_1_value().to_string(),
        2 => app.get_extra_target_2_value().to_string(),
        3 => app.get_extra_target_3_value().to_string(),
        4 => app.get_extra_target_4_value().to_string(),
        _ => String::new(),
    }
}

fn format_number_for_ui(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}
