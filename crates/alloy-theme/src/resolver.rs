use crate::color::Color;
use crate::error::{Result, ThemeError};
use crate::schema::{Theme, ThemeMode, Tokens};
use std::collections::BTreeMap;

/// Single contrast check result between foreground and background.
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastCheck {
    pub name: String,
    pub foreground_name: String,
    pub foreground_color: String,
    pub background_name: String,
    pub background_color: String,
    pub ratio: f64,
    pub min_aa: f64,
    pub min_aaa: f64,
    pub passes_aa: bool,
    pub passes_aaa: bool,
}

/// Aggregated contrast report for a resolved theme.
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastReport {
    pub checks: Vec<ContrastCheck>,
    pub passes_aa: bool,
    pub passes_aaa: bool,
}

impl ContrastReport {
    pub fn new() -> Self {
        Self {
            checks: Vec::new(),
            passes_aa: true,
            passes_aaa: true,
        }
    }

    pub fn add_check(&mut self, check: ContrastCheck) {
        if !check.passes_aa {
            self.passes_aa = false;
        }
        if !check.passes_aaa {
            self.passes_aaa = false;
        }
        self.checks.push(check);
    }

    /// List of checks that failed WCAG AA criteria.
    pub fn failures(&self) -> Vec<&ContrastCheck> {
        self.checks.iter().filter(|c| !c.passes_aa).collect()
    }
}

impl Default for ContrastReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Fully resolved theme: flat token path -> CSS value map, with CSS custom property emitter.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    pub id: String,
    pub name: String,
    pub mode: ThemeMode,
    /// Flat map of canonical token path (e.g. `color.surface.canvas`) -> CSS value string.
    pub tokens: BTreeMap<String, String>,
    /// WCAG contrast report on critical foreground/background pairs.
    pub contrast_report: ContrastReport,
}

impl ResolvedTheme {
    /// Return the value of a token by path.
    pub fn get(&self, path: &str) -> Option<&str> {
        self.tokens.get(path).map(|s| s.as_str())
    }

    /// Convert the flat token map to a map of CSS variable name -> value.
    /// Example: `color.surface.canvas` -> `--alloy-color-surface-canvas`.
    pub fn css_vars(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        for (path, value) in &self.tokens {
            let var_name = Self::token_path_to_css_var(path);
            map.insert(var_name, value.clone());
        }
        map
    }

    /// Emit CSS custom properties formatted in a `:root` block.
    pub fn to_css_vars(&self) -> String {
        let mut css = String::from(":root {\n");
        for (var_name, value) in self.css_vars() {
            css.push_str(&format!("  {}: {};\n", var_name, value));
        }
        css.push_str("}\n");
        css
    }

    /// Convert dot-separated token path to a kebab-case CSS custom property name.
    ///
    /// Rules:
    /// - Dot `.` becomes dash `-`
    /// - Underscore `_` becomes dash `-`
    /// - Prepend `--alloy-`
    ///
    /// Example: `color.surface.canvas` -> `--alloy-color-surface-canvas`
    pub fn token_path_to_css_var(path: &str) -> String {
        let kebab = path.replace(['.', '_'], "-").to_lowercase();
        format!("--alloy-{}", kebab)
    }
}

/// Theme resolver: manages base themes, extends hierarchy, and deep-merging.
pub struct ThemeResolver {
    registry: BTreeMap<String, Theme>,
}

impl ThemeResolver {
    /// Create a new resolver pre-populated with first-party base themes:
    /// `alloy-light`, `alloy-dark`, `alloy-amoled`.
    pub fn new() -> Self {
        let mut registry = BTreeMap::new();
        let light = crate::builtin::alloy_light();
        let dark = crate::builtin::alloy_dark();
        let amoled = crate::builtin::alloy_amoled();

        registry.insert(light.id.clone(), light);
        registry.insert(dark.id.clone(), dark);
        registry.insert(amoled.id.clone(), amoled);

        Self { registry }
    }

    /// Register a theme in the resolver registry so subsequent themes can extend it.
    pub fn register(&mut self, theme: Theme) {
        self.registry.insert(theme.id.clone(), theme);
    }

    /// Resolve a theme:
    /// 1. If `theme.extends` is set, lookup base theme in registry and resolve it first.
    ///    Otherwise, if theme is not a built-in, fallback to the default base theme for its mode.
    /// 2. Deep-merge theme tokens onto base tokens.
    /// 3. Apply any explicit overrides.
    /// 4. Validate color strings.
    /// 5. Validate presence of required tokens.
    /// 6. Perform WCAG contrast checks and produce `ContrastReport`.
    pub fn resolve(&self, theme: &Theme) -> Result<ResolvedTheme> {
        let mut flat_tokens = BTreeMap::new();

        // 1. Determine base theme to inherit from
        let base_id = match &theme.extends {
            Some(id) => Some(id.as_str()),
            None => {
                // If not a built-in base theme itself, fallback to built-in for this mode
                if theme.id != "alloy-light"
                    && theme.id != "alloy-dark"
                    && theme.id != "alloy-amoled"
                {
                    match theme.mode {
                        ThemeMode::Light => Some("alloy-light"),
                        ThemeMode::Dark => Some("alloy-dark"),
                        ThemeMode::Amoled => Some("alloy-amoled"),
                    }
                } else {
                    None
                }
            }
        };

        if let Some(base_key) = base_id {
            if base_key == theme.id {
                return Err(ThemeError::Validation(format!(
                    "Theme '{}' cannot extend itself",
                    theme.id
                )));
            }
            let base_theme = self.registry.get(base_key).ok_or_else(|| {
                ThemeError::Validation(format!(
                    "Theme '{}' extends unknown theme '{}'",
                    theme.id, base_key
                ))
            })?;
            let base_resolved = self.resolve(base_theme)?;
            flat_tokens.extend(base_resolved.tokens);
        }

        // 2. Deep-merge child tokens
        Self::flatten_tokens(&theme.tokens, &mut flat_tokens)?;

        // 3. Apply overrides if present
        if let Some(overrides) = &theme.overrides {
            for (key, val) in overrides {
                flat_tokens.insert(key.clone(), val.clone());
            }
        }

        // 4. Validate all color tokens parse correctly
        Self::validate_colors(&flat_tokens)?;

        // 5. Validate required tokens exist
        Self::validate_required_tokens(&flat_tokens)?;

        // 6. WCAG contrast checking
        let contrast_report = Self::check_contrast(&flat_tokens, theme.mode)?;

        Ok(ResolvedTheme {
            id: theme.id.clone(),
            name: theme.name.clone(),
            mode: theme.mode,
            tokens: flat_tokens,
            contrast_report,
        })
    }

    /// Flatten strongly-typed token structure into dot-separated paths.
    fn flatten_tokens(tokens: &Tokens, out: &mut BTreeMap<String, String>) -> Result<()> {
        if let Some(c) = &tokens.color {
            if let Some(s) = &c.surface {
                if let Some(v) = &s.canvas {
                    out.insert("color.surface.canvas".into(), v.clone());
                }
                if let Some(v) = &s.subtle {
                    out.insert("color.surface.subtle".into(), v.clone());
                }
                if let Some(v) = &s.panel {
                    out.insert("color.surface.panel".into(), v.clone());
                }
                if let Some(v) = &s.elevated {
                    out.insert("color.surface.elevated".into(), v.clone());
                }
                if let Some(v) = &s.hover {
                    out.insert("color.surface.hover".into(), v.clone());
                }
                if let Some(v) = &s.active {
                    out.insert("color.surface.active".into(), v.clone());
                }
            }
            if let Some(v) = &c.background {
                out.insert("color.background".into(), v.clone());
            }
            if let Some(v) = &c.scrim {
                out.insert("color.scrim".into(), v.clone());
            }
            if let Some(t) = &c.text {
                if let Some(v) = &t.primary {
                    out.insert("color.text.primary".into(), v.clone());
                }
                if let Some(v) = &t.secondary {
                    out.insert("color.text.secondary".into(), v.clone());
                }
                if let Some(v) = &t.muted {
                    out.insert("color.text.muted".into(), v.clone());
                }
                if let Some(v) = &t.link {
                    out.insert("color.text.link".into(), v.clone());
                }
                if let Some(v) = &t.on_accent {
                    out.insert("color.text.on_accent".into(), v.clone());
                }
            }
            if let Some(a) = &c.accent {
                if let Some(v) = &a.primary {
                    out.insert("color.accent.primary".into(), v.clone());
                }
                if let Some(v) = &a.secondary {
                    out.insert("color.accent.secondary".into(), v.clone());
                }
                if let Some(v) = &a.subtle {
                    out.insert("color.accent.subtle".into(), v.clone());
                }
                if let Some(v) = &a.hover {
                    out.insert("color.accent.hover".into(), v.clone());
                }
                if let Some(v) = a.fill.as_ref().or(a.primary.as_ref()) {
                    out.insert("color.accent.fill".into(), v.clone());
                }
            }
            if let Some(st) = &c.status {
                if let Some(v) = &st.online {
                    out.insert("color.status.online".into(), v.clone());
                }
                if let Some(v) = &st.away {
                    out.insert("color.status.away".into(), v.clone());
                }
                if let Some(v) = &st.busy {
                    out.insert("color.status.busy".into(), v.clone());
                }
                if let Some(v) = &st.offline {
                    out.insert("color.status.offline".into(), v.clone());
                }
                if let Some(v) = &st.error {
                    out.insert("color.status.error".into(), v.clone());
                }
                if let Some(v) = &st.warning {
                    out.insert("color.status.warning".into(), v.clone());
                }
                if let Some(v) = &st.info {
                    out.insert("color.status.info".into(), v.clone());
                }
                if let Some(v) = &st.success {
                    out.insert("color.status.success".into(), v.clone());
                }
                if let Some(v) = &st.mention {
                    out.insert("color.status.mention".into(), v.clone());
                }
            }
            if let Some(b) = &c.bubble {
                if let Some(own) = &b.own {
                    if let Some(v) = &own.background {
                        out.insert("color.bubble.own.background".into(), v.clone());
                    }
                    if let Some(v) = &own.text {
                        out.insert("color.bubble.own.text".into(), v.clone());
                    }
                }
                if let Some(other) = &b.other {
                    if let Some(v) = &other.background {
                        out.insert("color.bubble.other.background".into(), v.clone());
                    }
                    if let Some(v) = &other.text {
                        out.insert("color.bubble.other.text".into(), v.clone());
                    }
                }
            }
            if let Some(bd) = &c.border {
                if let Some(v) = &bd.subtle {
                    out.insert("color.border.subtle".into(), v.clone());
                }
                if let Some(v) = &bd.prominent {
                    out.insert("color.border.prominent".into(), v.clone());
                }
                if let Some(v) = &bd.floating {
                    out.insert("color.border.floating".into(), v.clone());
                }
            }
            if let Some(u) = &c.user {
                if let Some(v) = &u.c1 {
                    out.insert("color.user.1".into(), v.clone());
                }
                if let Some(v) = &u.c2 {
                    out.insert("color.user.2".into(), v.clone());
                }
                if let Some(v) = &u.c3 {
                    out.insert("color.user.3".into(), v.clone());
                }
                if let Some(v) = &u.c4 {
                    out.insert("color.user.4".into(), v.clone());
                }
                if let Some(v) = &u.c5 {
                    out.insert("color.user.5".into(), v.clone());
                }
                if let Some(v) = &u.c6 {
                    out.insert("color.user.6".into(), v.clone());
                }
                if let Some(v) = &u.c7 {
                    out.insert("color.user.7".into(), v.clone());
                }
                if let Some(v) = &u.c8 {
                    out.insert("color.user.8".into(), v.clone());
                }
                if let Some(v) = &u.c9 {
                    out.insert("color.user.9".into(), v.clone());
                }
                if let Some(v) = &u.c10 {
                    out.insert("color.user.10".into(), v.clone());
                }
                if let Some(v) = &u.c11 {
                    out.insert("color.user.11".into(), v.clone());
                }
                if let Some(v) = &u.c12 {
                    out.insert("color.user.12".into(), v.clone());
                }
            }
        }

        if let Some(typo) = &tokens.typography {
            if let Some(ff) = &typo.font_family {
                if let Some(v) = &ff.sans {
                    out.insert("typography.font_family.sans".into(), v.clone());
                }
                if let Some(v) = &ff.mono {
                    out.insert("typography.font_family.mono".into(), v.clone());
                }
                if let Some(v) = &ff.display {
                    out.insert("typography.font_family.display".into(), v.clone());
                }
            }
            if let Some(sz) = &typo.size {
                if let Some(v) = &sz.xs {
                    out.insert("typography.size.xs".into(), v.clone());
                }
                if let Some(v) = &sz.sm {
                    out.insert("typography.size.sm".into(), v.clone());
                }
                if let Some(v) = &sz.base {
                    out.insert("typography.size.base".into(), v.clone());
                }
                if let Some(v) = &sz.lg {
                    out.insert("typography.size.lg".into(), v.clone());
                }
                if let Some(v) = &sz.xl {
                    out.insert("typography.size.xl".into(), v.clone());
                }
                if let Some(v) = &sz.xxl {
                    out.insert("typography.size.xxl".into(), v.clone());
                }
            }
            if let Some(w) = &typo.weight {
                if let Some(v) = &w.regular {
                    out.insert("typography.weight.regular".into(), v.clone());
                }
                if let Some(v) = &w.medium {
                    out.insert("typography.weight.medium".into(), v.clone());
                }
                if let Some(v) = &w.semibold {
                    out.insert("typography.weight.semibold".into(), v.clone());
                }
                if let Some(v) = &w.bold {
                    out.insert("typography.weight.bold".into(), v.clone());
                }
            }
            if let Some(lh) = &typo.line_height {
                if let Some(v) = &lh.tight {
                    out.insert("typography.line_height.tight".into(), v.clone());
                }
                if let Some(v) = &lh.normal {
                    out.insert("typography.line_height.normal".into(), v.clone());
                }
                if let Some(v) = &lh.relaxed {
                    out.insert("typography.line_height.relaxed".into(), v.clone());
                }
            }
        }

        if let Some(sp) = &tokens.spacing {
            if let Some(d) = &sp.density {
                out.insert("spacing.density".into(), d.to_string());
            }
            if let Some(scale) = &sp.scale {
                for (k, v) in scale {
                    out.insert(format!("spacing.scale.{}", k), v.clone());
                }
            }
            if let Some(v) = &sp.message_padding {
                out.insert("spacing.message_padding".into(), v.clone());
            }
            if let Some(v) = &sp.avatar_size {
                out.insert("spacing.avatar_size".into(), v.clone());
            }
        }

        if let Some(r) = &tokens.radii {
            if let Some(v) = &r.none {
                out.insert("radii.none".into(), v.clone());
            }
            if let Some(v) = &r.sm {
                out.insert("radii.sm".into(), v.clone());
            }
            if let Some(v) = &r.md {
                out.insert("radii.md".into(), v.clone());
            }
            if let Some(v) = &r.lg {
                out.insert("radii.lg".into(), v.clone());
            }
            if let Some(v) = &r.xl {
                out.insert("radii.xl".into(), v.clone());
            }
            if let Some(v) = &r.xxl {
                out.insert("radii.2xl".into(), v.clone());
            }
            if let Some(v) = &r.full {
                out.insert("radii.full".into(), v.clone());
            }
        }

        if let Some(el) = &tokens.elevation {
            if let Some(v) = &el.none {
                out.insert("elevation.none".into(), v.clone());
            }
            if let Some(v) = &el.sm {
                out.insert("elevation.sm".into(), v.clone());
            }
            if let Some(v) = &el.md {
                out.insert("elevation.md".into(), v.clone());
            }
            if let Some(v) = &el.lg {
                out.insert("elevation.lg".into(), v.clone());
            }
        }

        if let Some(m) = &tokens.motion {
            if let Some(d) = &m.duration {
                if let Some(v) = &d.fast {
                    out.insert("motion.duration.fast".into(), v.clone());
                }
                if let Some(v) = &d.normal {
                    out.insert("motion.duration.normal".into(), v.clone());
                }
                if let Some(v) = &d.slow {
                    out.insert("motion.duration.slow".into(), v.clone());
                }
            }
            if let Some(e) = &m.easing {
                if let Some(v) = &e.ease {
                    out.insert("motion.easing.ease".into(), v.clone());
                }
                if let Some(v) = &e.ease_in_out {
                    out.insert("motion.easing.ease_in_out".into(), v.clone());
                }
            }
            if let Some(b) = m.reduced_motion {
                out.insert("motion.reduced_motion".into(), b.to_string());
            }
        }

        if let Some(l) = &tokens.layout {
            if let Some(ms) = &l.message_style {
                out.insert("layout.message_style".into(), ms.to_string());
            }
            if let Some(ash) = &l.avatar_shape {
                out.insert("layout.avatar_shape".into(), ash.to_string());
            }
        }

        if let Some(ch) = &tokens.chat {
            if let Some(bg) = &ch.background {
                if let Some(v) = &bg.kind {
                    out.insert("chat.background.kind".into(), v.clone());
                }
                if let Some(v) = &bg.value {
                    out.insert("chat.background.value".into(), v.clone());
                }
                if let Some(v) = &bg.opacity {
                    out.insert("chat.background.opacity".into(), v.clone());
                }
            } else {
                // No background specified, default to none
                out.insert("chat.background.kind".into(), "none".into());
                out.insert("chat.background.value".into(), "none".into());
                out.insert("chat.background.opacity".into(), "1".into());
            }
        } else {
            // No chat token, default all values
            out.insert("chat.background.kind".into(), "none".into());
            out.insert("chat.background.value".into(), "none".into());
            out.insert("chat.background.opacity".into(), "1".into());
        }
        out.insert("chat.background".into(), "none".into());

        Ok(())
    }

    /// Validate that all color tokens parse as valid colors.
    fn validate_colors(flat_tokens: &BTreeMap<String, String>) -> Result<()> {
        for (path, value) in flat_tokens {
            if path.starts_with("color.") {
                Color::parse(value)
                    .map_err(|_| ThemeError::InvalidColor(format!("{}: '{}'", path, value)))?;
            }
        }
        Ok(())
    }

    /// Validate that all mandatory base tokens are present in the resolved map.
    fn validate_required_tokens(flat_tokens: &BTreeMap<String, String>) -> Result<()> {
        let required = [
            "color.surface.canvas",
            "color.background",
            "color.text.primary",
            "color.text.secondary",
            "color.text.muted",
            "color.text.link",
            "color.accent.primary",
            "color.status.online",
            "color.bubble.own.background",
            "color.bubble.own.text",
            "color.bubble.other.background",
            "color.bubble.other.text",
            "typography.font_family.sans",
            "typography.size.base",
            "spacing.density",
            "radii.md",
            "layout.message_style",
            "layout.avatar_shape",
        ];

        for req in required {
            if !flat_tokens.contains_key(req) {
                return Err(ThemeError::MissingToken(req.to_string()));
            }
        }
        Ok(())
    }

    /// WCAG 2.1 contrast ratios on every foreground / background pair the UI spends.
    ///
    /// Thresholds are 4.5:1 for normal text and 3.0:1 only for non-text UI (the focus ring). The
    /// pairs cover the surfaces each token is actually painted on (A11Y-01/09/10/11/12/13/14):
    /// muted text on every surface, error text, link text, the avatar initials on every palette
    /// entry, the unread badge, the derived own-bubble meta, the derived destructive fill and the
    /// worst case of the translucent chrome/conversation material.
    ///
    /// `mode` selects the material recipe (light vs dark chrome/chat opacity and aurora strength,
    /// see ADR 0015). Pairs whose tokens are absent (a theme that does not use a bubble or a
    /// palette) are skipped, exactly as before.
    pub fn check_contrast(
        flat_tokens: &BTreeMap<String, String>,
        mode: ThemeMode,
    ) -> Result<ContrastReport> {
        let mut c = ContrastCollector::new(flat_tokens);
        let surfaces = ["canvas", "subtle", "panel", "elevated", "hover"];

        // Body and secondary text on the main reading surfaces.
        c.pair(
            "text_primary_on_canvas",
            "color.text.primary",
            "color.surface.canvas",
            4.5,
            7.0,
        );
        c.pair(
            "text_secondary_on_canvas",
            "color.text.secondary",
            "color.surface.canvas",
            4.5,
            7.0,
        );
        c.pair(
            "text_primary_on_subtle",
            "color.text.primary",
            "color.surface.subtle",
            4.5,
            7.0,
        );

        // Muted text is normal-size text (timestamps, placeholders, hints): 4.5:1 on every surface
        // it is painted on (A11Y-10).
        for surface in surfaces {
            c.pair(
                &format!("muted_on_{}", surface),
                "color.text.muted",
                &format!("color.surface.{}", surface),
                4.5,
                7.0,
            );
        }
        c.pair(
            "muted_on_bubble_other",
            "color.text.muted",
            "color.bubble.other.background",
            4.5,
            7.0,
        );

        // Inline error text is normal-size text on every surface a form, dialog or banner can use
        // (A11Y-09).
        for surface in surfaces {
            c.pair(
                &format!("error_on_{}", surface),
                "color.status.error",
                &format!("color.surface.{}", surface),
                4.5,
                7.0,
            );
        }

        // Link text is normal-size text, so the old 3.0 "large text" threshold is not enough
        // (A11Y-12).
        for surface in surfaces {
            c.pair(
                &format!("text_link_on_{}", surface),
                "color.text.link",
                &format!("color.surface.{}", surface),
                4.5,
                7.0,
            );
        }

        // Focus ring / accent used as a UI boundary is non-text: 3.0:1.
        c.pair(
            "accent_primary_on_canvas",
            "color.accent.primary",
            "color.surface.canvas",
            3.0,
            4.5,
        );

        // Bubble text on its own fill.
        c.pair(
            "bubble_own_text_on_background",
            "color.bubble.own.text",
            "color.bubble.own.background",
            4.5,
            7.0,
        );
        c.pair(
            "bubble_other_text_on_background",
            "color.bubble.other.text",
            "color.bubble.other.background",
            4.5,
            7.0,
        );

        // Text on the accent fill (primary buttons, filled badges).
        c.pair(
            "text_on_accent",
            "color.text.on_accent",
            "color.accent.fill",
            4.5,
            7.0,
        );

        // The unread badge paints on-accent text on status.mention (A11Y-11).
        c.pair(
            "text_on_accent_mention_badge",
            "color.text.on_accent",
            "color.status.mention",
            4.5,
            7.0,
        );

        // The destructive button keeps on-accent text on the derived `--alloy-danger-fill`
        // (status.error mixed 70% with black, A11Y-09). status.error itself is tuned for text, so
        // it is too light to carry a white label on its own.
        if let (Some(on_accent), Some(error)) = (
            c.token("color.text.on_accent"),
            c.token("color.status.error"),
        ) {
            let fill = ThemeResolver::danger_fill(&error.color);
            c.add(
                "text_on_accent_danger_fill",
                on_accent,
                ReportedColor::derived("derived:danger-fill", fill),
                4.5,
                7.0,
            );
        }

        // The derived own-bubble meta is 82% of the own text over the own bubble fill (A11Y-13).
        if let (Some(text), Some(background)) = (
            c.token("color.bubble.own.text"),
            c.token("color.bubble.own.background"),
        ) {
            let meta = ThemeResolver::own_bubble_meta(&text.color, &background.color);
            c.add(
                "bubble_own_meta_on_background",
                ReportedColor::derived("derived:bubble-own-meta", meta),
                background,
                4.5,
                7.0,
            );
        }

        // User identity colours: sender names on canvas and received bubbles, and the avatar
        // initials that sit on the palette fill itself (A11Y-01). The initials colour is chosen
        // from the fill's luminance exactly as `pickAvatarOnColor` does in the UI.
        for i in 1..=12 {
            let user_path = format!("color.user.{}", i);
            if let Some(fill) = c.token(&user_path) {
                let initials = ThemeResolver::avatar_initials_color(&fill.color);
                c.add(
                    &format!("avatar_initials_user_{}", i),
                    ReportedColor::derived("derived:avatar-initials", initials),
                    fill,
                    4.5,
                    7.0,
                );
                c.pair(
                    &format!("user_{}_on_canvas", i),
                    &user_path,
                    "color.surface.canvas",
                    4.5,
                    7.0,
                );
                c.pair(
                    &format!("user_{}_on_bubble_other", i),
                    &user_path,
                    "color.bubble.other.background",
                    4.5,
                    7.0,
                );
            }
        }

        // Worst case of the translucent material (A11Y-14): the pane fill over the strongest
        // aurora bloom, which is the lowest-contrast backdrop the text can land on. See ADR 0015.
        if let Some((chrome, chat)) = ThemeResolver::worst_case_materials(flat_tokens, mode) {
            for (token, name) in [
                ("primary", "text_primary"),
                ("secondary", "text_secondary"),
                ("muted", "text_muted"),
                ("link", "text_link"),
            ] {
                if let Some(foreground) = c.token(&format!("color.text.{}", token)) {
                    c.add(
                        &format!("{}_on_chrome_worst", name),
                        foreground.clone(),
                        ReportedColor::derived("derived:chrome-worst", chrome),
                        4.5,
                        7.0,
                    );
                    c.add(
                        &format!("{}_on_chat_worst", name),
                        foreground,
                        ReportedColor::derived("derived:chat-worst", chat),
                        4.5,
                        7.0,
                    );
                }
            }
        }

        Ok(c.report)
    }

    /// Near-black or white, whichever leaves the better contrast on an avatar fill (A11Y-01).
    ///
    /// `#ffffff` and [`AVATAR_INK`] are placed so that one of them always clears 4.5:1 for any
    /// possible fill (the worst case is ~4.51:1 at the crossover luminance).
    pub fn avatar_initials_color(fill: &Color) -> Color {
        let white = Color::new(1.0, 1.0, 1.0, 1.0);
        let ink = Color::parse(AVATAR_INK).unwrap_or(white);
        if white.contrast_ratio(fill) >= ink.contrast_ratio(fill) {
            white
        } else {
            ink
        }
    }

    /// The derived own-bubble meta colour: [`BUBBLE_OWN_META_ALPHA`] of the own text over the
    /// bubble fill, matching `--alloy-bubble-own-meta` in `tokens.css` (A11Y-13).
    pub fn own_bubble_meta(text: &Color, background: &Color) -> Color {
        Color::new(text.r, text.g, text.b, BUBBLE_OWN_META_ALPHA).blend_over(background)
    }

    /// The derived destructive-button fill: `color.status.error` mixed
    /// [`DANGER_FILL_ALPHA`] with black, matching `--alloy-danger-fill` in `tokens.css` (A11Y-09).
    pub fn danger_fill(error: &Color) -> Color {
        Color::new(
            error.r * DANGER_FILL_ALPHA,
            error.g * DANGER_FILL_ALPHA,
            error.b * DANGER_FILL_ALPHA,
            1.0,
        )
    }

    /// Worst-case backdrop of the translucent chrome and conversation panes (A11Y-14).
    ///
    /// The recipe comes from ADR 0014 / `tokens.css`: a pane fill (86% light / 80% dark for the
    /// chrome, 92% / 84% for the conversation) over the aurora's strongest bloom. The bloom is the
    /// accent-primary radial at its peak alpha (82% light / 42% dark) composited over the canvas,
    /// which is the point of the aurora that moves the backdrop furthest from the theme surface.
    fn worst_case_materials(
        flat_tokens: &BTreeMap<String, String>,
        mode: ThemeMode,
    ) -> Option<(Color, Color)> {
        let canvas = token_color(flat_tokens, "color.surface.canvas")?;
        let panel = token_color(flat_tokens, "color.surface.panel")?;
        let accent = token_color(flat_tokens, "color.accent.primary")?;
        let (chrome_fill, chat_fill, bloom_alpha) = match mode {
            ThemeMode::Light => (LIGHT_CHROME_FILL, LIGHT_CHAT_FILL, LIGHT_BLOOM_ALPHA),
            ThemeMode::Dark | ThemeMode::Amoled => {
                (DARK_CHROME_FILL, DARK_CHAT_FILL, DARK_BLOOM_ALPHA)
            }
        };
        let bloom = Color::new(accent.r, accent.g, accent.b, bloom_alpha).blend_over(&canvas);
        let chrome = Color::new(panel.r, panel.g, panel.b, chrome_fill).blend_over(&bloom);
        let chat = Color::new(canvas.r, canvas.g, canvas.b, chat_fill).blend_over(&bloom);
        Some((chrome, chat))
    }
}

/// Near-black initials on a light avatar fill (A11Y-01). Paired with `#ffffff`, one of the two
/// always clears 4.5:1: the worst possible fill sits at the crossover luminance, ~4.51:1.
pub const AVATAR_INK: &str = "#050505";

/// `--alloy-bubble-own-meta` in `tokens.css`: 82% of the own bubble text over the own bubble fill.
pub const BUBBLE_OWN_META_ALPHA: f64 = 0.82;

/// `--alloy-danger-fill` in `tokens.css`: `color.status.error` mixed 70% with black, so a white
/// label still clears 4.5:1 while the error colour itself is tuned for text (A11Y-09).
pub const DANGER_FILL_ALPHA: f64 = 0.70;

// Material recipe from ADR 0014 / `tokens.css`; the checker models the worst-case composite.
const LIGHT_CHROME_FILL: f64 = 0.86;
const DARK_CHROME_FILL: f64 = 0.80;
const LIGHT_CHAT_FILL: f64 = 0.92;
const DARK_CHAT_FILL: f64 = 0.84;
const LIGHT_BLOOM_ALPHA: f64 = 0.82;
const DARK_BLOOM_ALPHA: f64 = 0.42;

/// Parse one colour token, keeping the original string for the report.
fn token_color(tokens: &BTreeMap<String, String>, path: &str) -> Option<Color> {
    tokens.get(path).and_then(|raw| Color::parse(raw).ok())
}

/// One side of a contrast check: the label shown in the report and the parsed colour.
#[derive(Clone)]
struct ReportedColor {
    name: String,
    raw: String,
    color: Color,
}

impl ReportedColor {
    /// A value the checker derived rather than read straight from a token.
    fn derived(name: &str, color: Color) -> Self {
        Self {
            name: name.to_string(),
            raw: color.to_hex(),
            color,
        }
    }
}

/// Accumulates the contrast checks for one theme.
struct ContrastCollector<'a> {
    tokens: &'a BTreeMap<String, String>,
    report: ContrastReport,
}

impl<'a> ContrastCollector<'a> {
    fn new(tokens: &'a BTreeMap<String, String>) -> Self {
        Self {
            tokens,
            report: ContrastReport::new(),
        }
    }

    /// Resolve a token path to its label, raw string and parsed colour.
    fn token(&self, path: &str) -> Option<ReportedColor> {
        self.tokens.get(path).and_then(|raw| {
            Color::parse(raw).ok().map(|color| ReportedColor {
                name: path.to_string(),
                raw: raw.clone(),
                color,
            })
        })
    }

    /// Record one foreground/background pair.
    fn add(
        &mut self,
        name: &str,
        foreground: ReportedColor,
        background: ReportedColor,
        min_aa: f64,
        min_aaa: f64,
    ) {
        let ratio = foreground.color.contrast_ratio(&background.color);
        self.report.add_check(ContrastCheck {
            name: name.to_string(),
            foreground_name: foreground.name,
            foreground_color: foreground.raw,
            background_name: background.name,
            background_color: background.raw,
            ratio,
            min_aa,
            min_aaa,
            passes_aa: ratio >= min_aa,
            passes_aaa: ratio >= min_aaa,
        });
    }

    /// Record a foreground/background pair from two token paths (skipped when either is absent).
    fn pair(
        &mut self,
        name: &str,
        foreground_path: &str,
        background_path: &str,
        min_aa: f64,
        min_aaa: f64,
    ) {
        if let (Some(foreground), Some(background)) =
            (self.token(foreground_path), self.token(background_path))
        {
            self.add(name, foreground, background, min_aa, min_aaa);
        }
    }
}

impl Default for ThemeResolver {
    fn default() -> Self {
        Self::new()
    }
}
