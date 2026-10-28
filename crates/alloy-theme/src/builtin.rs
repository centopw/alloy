use crate::schema::*;
use std::collections::BTreeMap;

pub fn alloy_light() -> Theme {
    Theme {
        schema_url: Some("https://alloy.chat/schemas/v1/theme.json".into()),
        schema_version: SchemaVersion::Number(1),
        id: "alloy-light".into(),
        name: "Alloy Light".into(),
        author: "Alloy Team".into(),
        version: "0.1.0".into(),
        mode: ThemeMode::Light,
        extends: None,
        tokens: Tokens {
            color: Some(ColorTokens {
                surface: Some(SurfaceColors {
                    canvas: Some("#ffffff".into()),
                    subtle: Some("#f4f4f5".into()),
                    panel: Some("#fafafa".into()),
                    elevated: Some("#ffffff".into()),
                    hover: Some("#e4e4e7".into()),
                    active: Some("#d4d4d8".into()),
                }),
                background: Some("#ffffff".into()),
                text: Some(TextColors {
                    primary: Some("#18181b".into()),
                    secondary: Some("#52525b".into()),
                    muted: Some("#71717a".into()),
                    link: Some("#2563eb".into()),
                    on_accent: Some("#ffffff".into()),
                }),
                accent: Some(AccentColors {
                    primary: Some("#4f46e5".into()),
                    secondary: Some("#6366f1".into()),
                    subtle: Some("#e0e7ff".into()),
                    hover: Some("#4338ca".into()),
                    fill: Some("#4f46e5".into()),
                }),
                status: Some(StatusColors {
                    online: Some("#16a34a".into()),
                    away: Some("#d97706".into()),
                    busy: Some("#dc2626".into()),
                    offline: Some("#71717a".into()),
                    error: Some("#dc2626".into()),
                    warning: Some("#d97706".into()),
                    info: Some("#2563eb".into()),
                    success: Some("#16a34a".into()),
                    mention: Some("#ea580c".into()),
                }),
                bubble: Some(BubbleColors {
                    own: Some(BubbleEndpointColors {
                        background: Some("#4f46e5".into()),
                        text: Some("#ffffff".into()),
                    }),
                    other: Some(BubbleEndpointColors {
                        background: Some("#f4f4f5".into()),
                        text: Some("#18181b".into()),
                    }),
                }),
                user: Some(UserColors {
                    c1: Some("#b91c1c".into()),
                    c2: Some("#c2410c".into()),
                    c3: Some("#b45309".into()),
                    c4: Some("#15803d".into()),
                    c5: Some("#0f766e".into()),
                    c6: Some("#0369a1".into()),
                    c7: Some("#1d4ed8".into()),
                    c8: Some("#4338ca".into()),
                    c9: Some("#6d28d9".into()),
                    c10: Some("#a21caf".into()),
                    c11: Some("#be123c".into()),
                    c12: Some("#0e7490".into()),
                }),
                border: Some(BorderColors {
                    subtle: Some("#e4e4e7".into()),
                    prominent: Some("#d4d4d8".into()),
                    floating: Some("#d4d4d8".into()),
                }),
                scrim: Some("#05070ab3".into()),
            }),
            typography: Some(TypographyTokens {
                font_family: Some(FontFamilyTokens {
                    sans: Some(
                        "Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
                            .into(),
                    ),
                    mono: Some("'JetBrains Mono', 'Fira Code', monospace".into()),
                    display: Some("Inter, -apple-system, BlinkMacSystemFont, sans-serif".into()),
                }),
                size: Some(FontSizeTokens {
                    xs: Some("11px".into()),
                    sm: Some("12px".into()),
                    base: Some("14px".into()),
                    lg: Some("16px".into()),
                    xl: Some("18px".into()),
                    xxl: Some("24px".into()),
                }),
                weight: Some(FontWeightTokens {
                    regular: Some("400".into()),
                    medium: Some("500".into()),
                    semibold: Some("600".into()),
                    bold: Some("700".into()),
                }),
                line_height: Some(LineHeightTokens {
                    tight: Some("1.25".into()),
                    normal: Some("1.5".into()),
                    relaxed: Some("1.75".into()),
                }),
            }),
            spacing: Some(SpacingTokens {
                density: Some(DensityPreset::Normal),
                presets: Some(SpacingPresets {
                    compact: Some(DensityPresetTokens {
                        message_padding: Some("4px".into()),
                        avatar_size: Some("28px".into()),
                        item_gap: Some("4px".into()),
                    }),
                    normal: Some(DensityPresetTokens {
                        message_padding: Some("8px".into()),
                        avatar_size: Some("36px".into()),
                        item_gap: Some("8px".into()),
                    }),
                    cozy: Some(DensityPresetTokens {
                        message_padding: Some("12px".into()),
                        avatar_size: Some("40px".into()),
                        item_gap: Some("12px".into()),
                    }),
                }),
                scale: Some({
                    let mut m = BTreeMap::new();
                    m.insert("1".into(), "4px".into());
                    m.insert("2".into(), "8px".into());
                    m.insert("3".into(), "12px".into());
                    m.insert("4".into(), "16px".into());
                    m.insert("5".into(), "20px".into());
                    m.insert("6".into(), "24px".into());
                    m.insert("7".into(), "32px".into());
                    m.insert("8".into(), "40px".into());
                    m
                }),
                message_padding: Some("8px".into()),
                avatar_size: Some("36px".into()),
            }),
            radii: Some(RadiiTokens {
                none: Some("0px".into()),
                sm: Some("4px".into()),
                md: Some("6px".into()),
                lg: Some("8px".into()),
                xl: Some("12px".into()),
                xxl: Some("16px".into()),
                full: Some("9999px".into()),
            }),
            elevation: Some(ElevationTokens {
                none: Some("none".into()),
                sm: Some("0 1px 2px 0 rgba(0, 0, 0, 0.05)".into()),
                md: Some(
                    "0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -2px rgba(0, 0, 0, 0.1)".into(),
                ),
                lg: Some(
                    "0 10px 15px -3px rgba(0, 0, 0, 0.1), 0 4px 6px -4px rgba(0, 0, 0, 0.1)".into(),
                ),
            }),
            motion: Some(MotionTokens {
                duration: Some(MotionDurationTokens {
                    fast: Some("100ms".into()),
                    normal: Some("200ms".into()),
                    slow: Some("350ms".into()),
                }),
                easing: Some(MotionEasingTokens {
                    ease: Some("ease".into()),
                    ease_in_out: Some("cubic-bezier(0.4, 0, 0.2, 1)".into()),
                }),
                reduced_motion: Some(false),
            }),
            layout: Some(LayoutTokens {
                message_style: Some(MessageStyle::Bubble),
                avatar_shape: Some(AvatarShape::Circle),
            }),
            chat: Some(ChatTokens {
                background: Some(ChatBackgroundTokens {
                    kind: Some("none".into()),
                    value: Some("none".into()),
                    opacity: Some("1".into()),
                }),
            }),
        },
        overrides: None,
    }
}

pub fn alloy_dark() -> Theme {
    Theme {
        schema_url: Some("https://alloy.chat/schemas/v1/theme.json".into()),
        schema_version: SchemaVersion::Number(1),
        id: "alloy-dark".into(),
        name: "Alloy Dark".into(),
        author: "Alloy Team".into(),
        version: "0.1.0".into(),
        mode: ThemeMode::Dark,
        extends: None,
        tokens: Tokens {
            color: Some(ColorTokens {
                surface: Some(SurfaceColors {
                    canvas: Some("#141821".into()),
                    subtle: Some("#1a1f2b".into()),
                    panel: Some("#1a1f2b".into()),
                    elevated: Some("#242a38".into()),
                    hover: Some("#212734".into()),
                    active: Some("#2c3442".into()),
                }),
                background: Some("#141821".into()),
                text: Some(TextColors {
                    primary: Some("#f1f5f9".into()),
                    secondary: Some("#94a3b8".into()),
                    muted: Some("#8491a6".into()),
                    link: Some("#60a5fa".into()),
                    on_accent: Some("#ffffff".into()),
                }),
                accent: Some(AccentColors {
                    primary: Some("#3b82f6".into()),
                    secondary: Some("#818cf8".into()),
                    subtle: Some("#1b2b47".into()),
                    hover: Some("#1d4ed8".into()),
                    fill: Some("#2563eb".into()),
                }),
                status: Some(StatusColors {
                    online: Some("#10b981".into()),
                    away: Some("#f59e0b".into()),
                    busy: Some("#ef4444".into()),
                    offline: Some("#64748b".into()),
                    error: Some("#ef4444".into()),
                    warning: Some("#f59e0b".into()),
                    info: Some("#3b82f6".into()),
                    success: Some("#10b981".into()),
                    mention: Some("#3b82f6".into()),
                }),
                bubble: Some(BubbleColors {
                    own: Some(BubbleEndpointColors {
                        background: Some("#1d4ed8".into()),
                        text: Some("#ffffff".into()),
                    }),
                    other: Some(BubbleEndpointColors {
                        background: Some("#242a38".into()),
                        text: Some("#f1f5f9".into()),
                    }),
                }),
                user: Some(UserColors {
                    c1: Some("#f87171".into()),
                    c2: Some("#fb923c".into()),
                    c3: Some("#fbbf24".into()),
                    c4: Some("#4ade80".into()),
                    c5: Some("#2dd4bf".into()),
                    c6: Some("#38bdf8".into()),
                    c7: Some("#60a5fa".into()),
                    c8: Some("#818cf8".into()),
                    c9: Some("#a78bfa".into()),
                    c10: Some("#e879f9".into()),
                    c11: Some("#fb7185".into()),
                    c12: Some("#22d3ee".into()),
                }),
                border: Some(BorderColors {
                    subtle: Some("#2b3242".into()),
                    prominent: Some("#3e4860".into()),
                    floating: Some("#3a4458".into()),
                }),
                scrim: Some("#05070ab3".into()),
            }),
            typography: Some(TypographyTokens {
                font_family: Some(FontFamilyTokens {
                    sans: Some(
                        "Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
                            .into(),
                    ),
                    mono: Some("'JetBrains Mono', 'Fira Code', monospace".into()),
                    display: Some("Inter, -apple-system, BlinkMacSystemFont, sans-serif".into()),
                }),
                size: Some(FontSizeTokens {
                    xs: Some("11px".into()),
                    sm: Some("12px".into()),
                    base: Some("14px".into()),
                    lg: Some("16px".into()),
                    xl: Some("18px".into()),
                    xxl: Some("24px".into()),
                }),
                weight: Some(FontWeightTokens {
                    regular: Some("400".into()),
                    medium: Some("500".into()),
                    semibold: Some("600".into()),
                    bold: Some("700".into()),
                }),
                line_height: Some(LineHeightTokens {
                    tight: Some("1.25".into()),
                    normal: Some("1.5".into()),
                    relaxed: Some("1.75".into()),
                }),
            }),
            spacing: Some(SpacingTokens {
                density: Some(DensityPreset::Normal),
                presets: Some(SpacingPresets {
                    compact: Some(DensityPresetTokens {
                        message_padding: Some("4px".into()),
                        avatar_size: Some("28px".into()),
                        item_gap: Some("4px".into()),
                    }),
                    normal: Some(DensityPresetTokens {
                        message_padding: Some("8px".into()),
                        avatar_size: Some("36px".into()),
                        item_gap: Some("8px".into()),
                    }),
                    cozy: Some(DensityPresetTokens {
                        message_padding: Some("12px".into()),
                        avatar_size: Some("40px".into()),
                        item_gap: Some("12px".into()),
                    }),
                }),
                scale: Some({
                    let mut m = BTreeMap::new();
                    m.insert("1".into(), "4px".into());
                    m.insert("2".into(), "8px".into());
                    m.insert("3".into(), "12px".into());
                    m.insert("4".into(), "16px".into());
                    m.insert("5".into(), "20px".into());
                    m.insert("6".into(), "24px".into());
                    m.insert("7".into(), "32px".into());
                    m.insert("8".into(), "40px".into());
                    m
                }),
                message_padding: Some("8px".into()),
                avatar_size: Some("36px".into()),
            }),
            radii: Some(RadiiTokens {
                none: Some("0px".into()),
                sm: Some("4px".into()),
                md: Some("6px".into()),
                lg: Some("8px".into()),
                xl: Some("12px".into()),
                xxl: Some("16px".into()),
                full: Some("9999px".into()),
            }),
            elevation: Some(ElevationTokens {
                none: Some("none".into()),
                sm: Some("0 1px 2px 0 rgba(0, 0, 0, 0.4)".into()),
                md: Some(
                    "0 4px 6px -1px rgba(0, 0, 0, 0.5), 0 2px 4px -2px rgba(0, 0, 0, 0.5)".into(),
                ),
                lg: Some(
                    "0 10px 15px -3px rgba(0, 0, 0, 0.6), 0 4px 6px -4px rgba(0, 0, 0, 0.6)".into(),
                ),
            }),
            motion: Some(MotionTokens {
                duration: Some(MotionDurationTokens {
                    fast: Some("100ms".into()),
                    normal: Some("200ms".into()),
                    slow: Some("350ms".into()),
                }),
                easing: Some(MotionEasingTokens {
                    ease: Some("ease".into()),
                    ease_in_out: Some("cubic-bezier(0.4, 0, 0.2, 1)".into()),
                }),
                reduced_motion: Some(false),
            }),
            layout: Some(LayoutTokens {
                message_style: Some(MessageStyle::Bubble),
                avatar_shape: Some(AvatarShape::Circle),
            }),
            chat: Some(ChatTokens {
                background: Some(ChatBackgroundTokens {
                    kind: Some("none".into()),
                    value: Some("none".into()),
                    opacity: Some("1".into()),
                }),
            }),
        },
        overrides: None,
    }
}

pub fn alloy_amoled() -> Theme {
    let mut theme = alloy_dark();
    theme.id = "alloy-amoled".into();
    theme.name = "Alloy AMOLED".into();
    theme.mode = ThemeMode::Amoled;
    if let Some(color) = &mut theme.tokens.color {
        color.background = Some("#000000".into());
        if let Some(surface) = &mut color.surface {
            surface.canvas = Some("#000000".into());
            surface.subtle = Some("#0a0a0a".into());
            surface.panel = Some("#050505".into());
            surface.elevated = Some("#121212".into());
            surface.hover = Some("#18181b".into());
        }
        if let Some(border) = &mut color.border {
            border.subtle = Some("#18181b".into());
            border.prominent = Some("#27272a".into());
        }
        if let Some(bubble) = &mut color.bubble
            && let Some(other) = &mut bubble.other
        {
            other.background = Some("#121212".into());
        }
    }
    theme
}
