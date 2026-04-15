use eframe::egui::{self, Color32, Stroke};

pub const CORNER_RADIUS: f32 = 8.0;
pub const CARD_PADDING: f32 = 18.0;
pub const PANEL_MARGIN: f32 = 16.0;
pub const SECTION_GAP: f32 = 14.0;

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.animation_time = 0.0;
    style.spacing.item_spacing = egui::vec2(14.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size = egui::vec2(40.0, 32.0);
    style.spacing.indent = 18.0;

    let mut visuals = egui::Visuals::light();
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.popup_shadow = egui::epaint::Shadow::NONE;
    visuals.window_fill = app_background();
    visuals.panel_fill = app_background();
    visuals.faint_bg_color = subtle_background();
    visuals.extreme_bg_color = surface();
    visuals.code_bg_color = subtle_background();
    visuals.hyperlink_color = primary();
    visuals.override_text_color = Some(text_primary());
    visuals.selection.bg_fill = primary();
    visuals.selection.stroke = Stroke::new(1.0, primary());
    visuals.window_stroke = Stroke::new(1.0, border());
    visuals.widgets.noninteractive.bg_fill = surface();
    visuals.widgets.noninteractive.weak_bg_fill = surface();
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border());
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_secondary());
    visuals.widgets.noninteractive.rounding = egui::Rounding::same(CORNER_RADIUS);
    visuals.widgets.inactive.bg_fill = surface();
    visuals.widgets.inactive.weak_bg_fill = surface();
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border());
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text_primary());
    visuals.widgets.inactive.rounding = egui::Rounding::same(CORNER_RADIUS);
    visuals.widgets.hovered.bg_fill = hover_fill();
    visuals.widgets.hovered.weak_bg_fill = hover_fill();
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, primary_soft_border());
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, text_primary());
    visuals.widgets.hovered.rounding = egui::Rounding::same(CORNER_RADIUS);
    visuals.widgets.hovered.expansion = 0.0;
    visuals.widgets.active.bg_fill = primary_soft();
    visuals.widgets.active.weak_bg_fill = primary_soft();
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, primary());
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, text_primary());
    visuals.widgets.active.rounding = egui::Rounding::same(CORNER_RADIUS);
    visuals.widgets.active.expansion = 0.0;
    visuals.widgets.open.bg_fill = primary_soft();
    visuals.widgets.open.weak_bg_fill = primary_soft();
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, primary_soft_border());
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, text_primary());
    visuals.widgets.open.rounding = egui::Rounding::same(CORNER_RADIUS);
    style.visuals = visuals;

    ctx.set_style(style);
}

pub fn card_shadow() -> egui::epaint::Shadow {
    egui::epaint::Shadow::NONE
}

pub fn app_background() -> Color32 {
    Color32::from_rgb(249, 250, 251)
}

pub fn surface() -> Color32 {
    Color32::from_rgb(255, 255, 255)
}

pub fn subtle_background() -> Color32 {
    Color32::from_rgb(243, 244, 246)
}

pub fn hover_fill() -> Color32 {
    Color32::from_rgb(248, 250, 252)
}

pub fn text_primary() -> Color32 {
    Color32::from_rgb(17, 24, 39)
}

pub fn text_secondary() -> Color32 {
    Color32::from_rgb(107, 114, 128)
}

pub fn border() -> Color32 {
    Color32::from_rgb(229, 231, 235)
}

pub fn primary() -> Color32 {
    Color32::from_rgb(59, 130, 246)
}

pub fn primary_dark() -> Color32 {
    Color32::from_rgb(37, 99, 235)
}

pub fn primary_soft() -> Color32 {
    Color32::from_rgb(239, 246, 255)
}

pub fn primary_soft_border() -> Color32 {
    Color32::from_rgb(191, 219, 254)
}

pub fn warning() -> Color32 {
    Color32::from_rgb(245, 158, 11)
}

pub fn warning_dark() -> Color32 {
    Color32::from_rgb(217, 119, 6)
}

pub fn warning_soft() -> Color32 {
    Color32::from_rgb(255, 251, 235)
}

pub fn warning_soft_border() -> Color32 {
    Color32::from_rgb(253, 230, 138)
}

pub fn danger() -> Color32 {
    Color32::from_rgb(239, 68, 68)
}

pub fn danger_dark() -> Color32 {
    Color32::from_rgb(220, 38, 38)
}

pub fn danger_soft() -> Color32 {
    Color32::from_rgb(254, 242, 242)
}

pub fn danger_soft_border() -> Color32 {
    Color32::from_rgb(254, 202, 202)
}
