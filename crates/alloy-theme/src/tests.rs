use crate::builtin::{alloy_amoled, alloy_dark, alloy_light};
use crate::color::Color;
use crate::loader::ThemeLoader;
use crate::resolver::ThemeResolver;
use crate::schema::*;

#[test]
fn test_color_parsing_and_luminance() {
    let black = Color::parse("#000000").unwrap();
    let white = Color::parse("#ffffff").unwrap();

    assert_eq!(black.relative_luminance(), 0.0);
    assert!((white.relative_luminance() - 1.0).abs() < 0.001);

    // WCAG contrast ratio for black on white is 21:1
    let ratio = black.contrast_ratio(&white);
    assert!((ratio - 21.0).abs() < 0.01);

    // Identical colors have contrast 1:1
    let gray = Color::parse("#808080").unwrap();
    assert!((gray.contrast_ratio(&gray) - 1.0).abs() < 0.001);
}

#[test]
fn test_color_parsing_formats() {
    // 3-digit hex
    assert!(Color::parse("#fff").is_ok());
    // 4-digit hex with alpha
    assert!(Color::parse("#fff8").is_ok());
    // 6-digit hex
    assert!(Color::parse("#18181b").is_ok());
    // 8-digit hex with alpha
    assert!(Color::parse("#18181b80").is_ok());
    // rgb()
    assert!(Color::parse("rgb(24, 24, 27)").is_ok());
    // rgba()
    assert!(Color::parse("rgba(24, 24, 27, 0.5)").is_ok());

    // Bad colors
    assert!(Color::parse("not-a-color").is_err());
    assert!(Color::parse("#12").is_err());
    assert!(Color::parse("#12345").is_err());
    assert!(Color::parse("#gggggg").is_err());
}

#[test]
fn test_rejection_of_unknown_keys() {
    let bad_json = r#"{
        "schema_version": 1,
        "id": "bad-theme",
        "name": "Bad Theme",
        "author": "Tester",
        "version": "1.0.0",
        "mode": "light",
        "unexpected_key": "should fail",
        "tokens": {}
    }"#;
    let err = ThemeLoader::load_from_str(bad_json);
    assert!(
        err.is_err(),
        "Expected deserialization error on unknown key"
    );

    let bad_token_json = r#"{
        "schema_version": 1,
        "id": "bad-tokens",
        "name": "Bad Tokens",
        "author": "Tester",
        "version": "1.0.0",
        "mode": "dark",
        "tokens": {
            "unknown_category": {}
        }
    }"#;
    assert!(ThemeLoader::load_from_str(bad_token_json).is_err());
}

#[test]
fn test_rejection_of_bad_colors() {
    let resolver = ThemeResolver::new();
    let mut theme = alloy_light();
    theme.id = "bad-color".into();
    if let Some(color) = &mut theme.tokens.color
        && let Some(text) = &mut color.text
    {
        text.primary = Some("invalid-hex-code".into());
    }
    let res = resolver.resolve(&theme);
    assert!(res.is_err());
}

#[test]
fn test_jsonc_comments_allowed() {
    let jsonc = r##"{
        // This is a single-line comment
        "schema_version": 1,
        "id": "jsonc-theme",
        "name": "JSONC Theme",
        "author": "Alloy",
        "version": "0.1.0",
        /* This is a multi-line
           comment block */
        "mode": "dark",
        "tokens": {
            "color": {
                "background": "#000000", // Trailing line comment
            },
        }, // Trailing comma allowed in JSONC
    }"##;
    let theme = ThemeLoader::load_from_str(jsonc).expect("JSONC should parse cleanly");
    assert_eq!(theme.id, "jsonc-theme");
    assert_eq!(theme.mode, ThemeMode::Dark);
}

#[test]
fn test_builtin_themes_resolve_and_pass_contrast() {
    let resolver = ThemeResolver::new();

    let light = resolver
        .resolve(&alloy_light())
        .expect("alloy-light resolves");
    assert!(
        light.contrast_report.passes_aa,
        "alloy-light must pass WCAG AA"
    );
    assert_eq!(light.get("color.surface.canvas"), Some("#ffffff"));
    assert_eq!(light.get("layout.message_style"), Some("bubble"));

    let dark = resolver
        .resolve(&alloy_dark())
        .expect("alloy-dark resolves");
    assert!(
        dark.contrast_report.passes_aa,
        "alloy-dark must pass WCAG AA"
    );
    assert_eq!(dark.get("color.surface.canvas"), Some("#18181b"));
    let amoled = resolver
        .resolve(&alloy_amoled())
        .expect("alloy-amoled resolves");
    assert!(
        amoled.contrast_report.passes_aa,
        "alloy-amoled must pass WCAG AA"
    );
    assert_eq!(amoled.get("color.surface.canvas"), Some("#000000"));
}

#[test]
fn test_contrast_failure_detection_on_poor_theme() {
    let resolver = ThemeResolver::new();
    let mut poor = alloy_light();
    poor.id = "intentionally-poor".into();

    // Gray text on gray background (#777777 on #888888 -> contrast ratio ~ 1.3:1)
    if let Some(color) = &mut poor.tokens.color {
        if let Some(surface) = &mut color.surface {
            surface.canvas = Some("#888888".into());
        }
        if let Some(text) = &mut color.text {
            text.primary = Some("#777777".into());
        }
    }

    let resolved = resolver.resolve(&poor).expect("Resolves syntactically");
    assert!(
        !resolved.contrast_report.passes_aa,
        "Intentionally poor theme must fail AA"
    );
    let failures = resolved.contrast_report.failures();
    assert!(!failures.is_empty(), "Must list failing pairs");
    assert!(failures.iter().any(|f| f.name == "text_primary_on_canvas"));
}

#[test]
fn test_extends_and_deep_merge() {
    let mut resolver = ThemeResolver::new();

    let child_json = r##"{
        "schema_version": 1,
        "id": "my-custom-child",
        "name": "Custom Child",
        "author": "Community",
        "version": "1.0.0",
        "mode": "dark",
        "extends": "alloy-dark",
        "tokens": {
            "color": {
                "accent": {
                    "primary": "#ff007f"
                }
            },
            "layout": {
                "message_style": "gutter"
            }
        },
        "overrides": {
            "custom.token": "custom-val"
        }
    }"##;

    let child = ThemeLoader::load_from_str(child_json).unwrap();
    resolver.register(child.clone());

    let resolved = resolver.resolve(&child).expect("Child must resolve");

    // Inherited from alloy-dark base
    assert_eq!(resolved.get("color.surface.canvas"), Some("#18181b"));
    assert_eq!(
        resolved.get("typography.font_family.sans"),
        Some("Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
    );

    // Overridden by child
    assert_eq!(resolved.get("color.accent.primary"), Some("#ff007f"));
    assert_eq!(resolved.get("layout.message_style"), Some("gutter"));

    // Overrides map applied
    assert_eq!(resolved.get("custom.token"), Some("custom-val"));
}

#[test]
fn test_css_vars_snapshot_assertion() {
    let resolver = ThemeResolver::new();
    let light = resolver.resolve(&alloy_light()).unwrap();
    let css = light.to_css_vars();

    // Verify root block structure
    assert!(css.starts_with(":root {\n"));
    assert!(css.ends_with("}\n"));

    // Verify stable token paths converted to CSS variables
    assert!(css.contains("  --alloy-color-surface-canvas: #ffffff;\n"));
    assert!(css.contains("  --alloy-color-text-primary: #18181b;\n"));
    assert!(css.contains("  --alloy-color-accent-primary: #4f46e5;\n"));
    assert!(css.contains("  --alloy-typography-font-family-sans: Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;\n"));
    assert!(css.contains("  --alloy-typography-size-base: 14px;\n"));
    assert!(css.contains("  --alloy-spacing-density: normal;\n"));
    assert!(css.contains("  --alloy-radii-md: 8px;\n"));
    assert!(css.contains("  --alloy-layout-message-style: bubble;\n"));
    assert!(css.contains("  --alloy-layout-avatar-shape: circle;\n"));
}

#[test]
fn test_keybindings_loader() {
    let array_json = r#"[
        { "key": "mod+k", "command": "quick_switcher.open" },
        { "key": "enter", "command": "composer.send", "when": "composer_focused" }
    ]"#;
    let bindings = ThemeLoader::load_keybindings_from_str(array_json).unwrap();
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].key, "mod+k");
    assert_eq!(bindings[0].command, "quick_switcher.open");
    assert_eq!(bindings[0].when, None);
    assert_eq!(bindings[1].when, Some("composer_focused".into()));

    let object_jsonc = r#"{
        // Custom keybindings
        "schema_version": 1,
        "bindings": [
            { "key": "esc", "command": "dialog.close" },
        ]
    }"#;
    let bindings2 = ThemeLoader::load_keybindings_from_str(object_jsonc).unwrap();
    assert_eq!(bindings2.len(), 1);
    assert_eq!(bindings2[0].key, "esc");
}

#[test]
fn test_user_colors_meet_contrast_requirements() {
    let resolver = ThemeResolver::new();

    // Test light theme
    let light = alloy_light();
    let resolved_light = resolver
        .resolve(&light)
        .expect("Light theme should resolve");

    for i in 1..=12 {
        let color_path = format!("color.user.{}", i);
        assert!(
            resolved_light.tokens.contains_key(&color_path),
            "Light theme should have user color {}",
            i
        );
    }

    // Ensure contrast report for light theme passes AA
    assert!(
        resolved_light.contrast_report.passes_aa,
        "Light theme user colors must pass WCAG AA. Failures: {:?}",
        resolved_light.contrast_report.failures()
    );

    // Test dark theme
    let dark = alloy_dark();
    let resolved_dark = resolver.resolve(&dark).expect("Dark theme should resolve");

    for i in 1..=12 {
        let color_path = format!("color.user.{}", i);
        assert!(
            resolved_dark.tokens.contains_key(&color_path),
            "Dark theme should have user color {}",
            i
        );
    }

    // Ensure contrast report for dark theme passes AA
    assert!(
        resolved_dark.contrast_report.passes_aa,
        "Dark theme user colors must pass WCAG AA. Failures: {:?}",
        resolved_dark.contrast_report.failures()
    );

    // Test AMOLED theme
    let amoled = alloy_amoled();
    let resolved_amoled = resolver
        .resolve(&amoled)
        .expect("AMOLED theme should resolve");

    for i in 1..=12 {
        let color_path = format!("color.user.{}", i);
        assert!(
            resolved_amoled.tokens.contains_key(&color_path),
            "AMOLED theme should have user color {} (inherited from dark)",
            i
        );
    }

    assert!(
        resolved_amoled.contrast_report.passes_aa,
        "AMOLED theme user colors must pass WCAG AA. Failures: {:?}",
        resolved_amoled.contrast_report.failures()
    );
}

#[test]
fn test_chat_background_defaults() {
    let resolver = ThemeResolver::new();
    let light = alloy_light();
    let resolved = resolver
        .resolve(&light)
        .expect("Light theme should resolve");

    // Chat background should have all required sub-tokens
    assert_eq!(
        resolved.tokens.get("chat.background").map(|s| s.as_str()),
        Some("none"),
        "Default chat background kind should be 'none'"
    );
    assert_eq!(
        resolved
            .tokens
            .get("chat.background.kind")
            .map(|s| s.as_str()),
        Some("none"),
        "Chat background.kind should default to 'none'"
    );
    assert_eq!(
        resolved
            .tokens
            .get("chat.background.value")
            .map(|s| s.as_str()),
        Some("none"),
        "Chat background.value should default to 'none'"
    );
    assert_eq!(
        resolved
            .tokens
            .get("chat.background.opacity")
            .map(|s| s.as_str()),
        Some("1"),
        "Chat background.opacity should default to '1'"
    );
}

#[test]
fn test_user_colors_in_css_vars() {
    let resolver = ThemeResolver::new();
    let light = alloy_light();
    let resolved = resolver
        .resolve(&light)
        .expect("Light theme should resolve");

    let css_vars = resolved.css_vars();

    // Check that all 12 user colors appear as CSS custom properties
    for i in 1..=12 {
        let var_name = format!("--alloy-color-user-{}", i);
        assert!(
            css_vars.contains_key(&var_name),
            "CSS vars should contain {}",
            var_name
        );
    }

    // Check chat background
    assert!(css_vars.contains_key("--alloy-chat-background"));
    assert!(css_vars.contains_key("--alloy-chat-background-kind"));
    assert!(css_vars.contains_key("--alloy-chat-background-value"));
    assert!(css_vars.contains_key("--alloy-chat-background-opacity"));
}

#[test]
fn test_contrast_fails_if_user_palette_entry_drops_below_aa() {
    let resolver = ThemeResolver::new();
    let mut theme = alloy_light();
    theme.id = "bad-palette-test".into();

    // Intentionally set user color 1 to white (#ffffff) which has 1.0:1 contrast against #ffffff canvas!
    if let Some(color) = &mut theme.tokens.color
        && let Some(user) = &mut color.user
    {
        user.c1 = Some("#ffffff".into());
    }

    let resolved = resolver
        .resolve(&theme)
        .expect("Theme should still resolve");
    assert!(
        !resolved.contrast_report.passes_aa,
        "Contrast report MUST fail AA if any palette entry drops below 4.5:1"
    );

    let failures = resolved.contrast_report.failures();
    assert!(
        failures.iter().any(|f| f.foreground_name == "color.user.1"),
        "Failures list must contain user color 1 failure"
    );
}
