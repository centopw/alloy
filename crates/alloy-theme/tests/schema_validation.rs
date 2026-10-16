use alloy_theme::{ThemeLoader, ThemeResolver};
use std::fs;
use std::path::Path;

#[test]
fn test_shipped_example_themes_against_json_schema() {
    let schema_str = fs::read_to_string("../../schemas/theme.schema.json")
        .or_else(|_| fs::read_to_string("schemas/theme.schema.json"))
        .expect("Read theme schema");

    let schema_json: serde_json::Value =
        serde_json::from_str(&schema_str).expect("Valid JSON schema");

    let validator = jsonschema::validator_for(&schema_json).expect("Compile JSON schema validator");

    let theme_files = [
        "packages/themes/alloy-light.json",
        "packages/themes/alloy-dark.json",
        "packages/themes/alloy-amoled.json",
        "packages/themes/midnight-ocean.json",
    ];

    for file_path in theme_files {
        let path = if Path::new(file_path).exists() {
            file_path.to_string()
        } else {
            format!("../../{}", file_path)
        };

        let content =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("Could not read {}: {}", path, e));
        let instance: serde_json::Value =
            serde_json::from_str(&content).expect("Parse instance JSON");

        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| format!("{}: {}", e.instance_path(), e))
            .collect();
        if !errors.is_empty() {
            panic!(
                "Validation of {} failed against theme.schema.json:\n{}",
                file_path,
                errors.join("\n")
            );
        }
    }
}

#[test]
fn test_keybindings_against_json_schema() {
    let schema_str = fs::read_to_string("../../schemas/keybindings.schema.json")
        .or_else(|_| fs::read_to_string("schemas/keybindings.schema.json"))
        .expect("Read keybindings schema");

    let schema_json: serde_json::Value =
        serde_json::from_str(&schema_str).expect("Valid JSON schema");

    let validator =
        jsonschema::validator_for(&schema_json).expect("Compile keybindings JSON schema validator");

    // Test array form
    let array_form = serde_json::json!([
        { "key": "mod+k", "command": "quick_switcher.open" },
        { "key": "ctrl+shift+p", "command": "command_palette.open" },
        { "key": "enter", "command": "composer.send", "when": "composer_focused" }
    ]);
    assert!(
        validator.validate(&array_form).is_ok(),
        "Array keybindings must validate"
    );

    // Test object form
    let object_form = serde_json::json!({
        "schema_version": 1,
        "bindings": [
            { "key": "esc", "command": "modal.close" },
            { "key": "up", "command": "composer.edit_last_message", "when": "composer_empty" }
        ]
    });
    assert!(
        validator.validate(&object_form).is_ok(),
        "Object keybindings must validate"
    );

    // Test rejection of missing required key
    let invalid_missing_command = serde_json::json!([
        { "key": "mod+k" }
    ]);
    assert!(
        validator.validate(&invalid_missing_command).is_err(),
        "Keybinding without command must fail validation"
    );
}

#[test]
fn test_shipped_themes_load_resolve_and_pass_aa() {
    let theme_files = [
        "packages/themes/alloy-light.json",
        "packages/themes/alloy-dark.json",
        "packages/themes/alloy-amoled.json",
        "packages/themes/midnight-ocean.json",
    ];

    let mut resolver = ThemeResolver::new();

    // First load all into memory
    let mut loaded_themes = Vec::new();
    for file_path in theme_files {
        let path = if Path::new(file_path).exists() {
            file_path.to_string()
        } else {
            format!("../../{}", file_path)
        };

        let theme = ThemeLoader::load_from_path(&path)
            .unwrap_or_else(|e| panic!("Failed loading {}: {:?}", path, e));
        resolver.register(theme.clone());
        loaded_themes.push((file_path, theme));
    }

    // Now resolve each one and check contrast
    for (name, theme) in loaded_themes {
        let resolved = resolver
            .resolve(&theme)
            .unwrap_or_else(|e| panic!("Failed resolving {}: {:?}", name, e));

        if !resolved.contrast_report.passes_aa {
            let failures: Vec<String> = resolved
                .contrast_report
                .failures()
                .iter()
                .map(|f| {
                    format!(
                        "Pair '{}' ({} on {}): ratio {:.2} < min {:.2}",
                        f.name, f.foreground_name, f.background_name, f.ratio, f.min_aa
                    )
                })
                .collect();
            panic!(
                "Theme '{}' failed WCAG AA contrast check:\n{}",
                name,
                failures.join("\n")
            );
        }

        // Verify CSS vars generation works
        let css = resolved.to_css_vars();
        assert!(
            css.contains("--alloy-color-surface-canvas:"),
            "Theme '{}' missing canvas css var",
            name
        );
    }
}
