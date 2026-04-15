use std::hash::Hash;

use eframe::egui::{self, DragValue, RichText, TextEdit, Ui};

use crate::ui::theme;

const FORM_LABEL_WIDTH: f32 = 108.0;
const FORM_TEXT_WIDTH: f32 = 248.0;
const FORM_NUMBER_WIDTH: f32 = 140.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct CardMetrics {
    pub actual_height: f32,
    pub displayed_height: f32,
}

pub fn card<R>(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    card_with_body_spacing(ui, title, subtitle, 8.0, add_contents)
}

pub fn card_with_body_spacing<R>(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    body_top_spacing: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    card_with_options(ui, title, subtitle, body_top_spacing, None, add_contents)
}

pub fn card_with_body_spacing_and_height<R>(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    body_top_spacing: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (R, f32) {
    let (inner, metrics) =
        card_with_options_and_metrics(ui, title, subtitle, body_top_spacing, None, add_contents);
    (inner, metrics.displayed_height)
}

pub fn card_with_body_spacing_and_min_height_and_metrics<R>(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    body_top_spacing: f32,
    min_height: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (R, CardMetrics) {
    card_with_options_and_metrics(
        ui,
        title,
        subtitle,
        body_top_spacing,
        Some(min_height),
        add_contents,
    )
}

pub fn card_with_body_spacing_and_min_height<R>(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    body_top_spacing: f32,
    min_height: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    card_with_options_and_metrics(
        ui,
        title,
        subtitle,
        body_top_spacing,
        Some(min_height),
        add_contents,
    )
    .0
}

fn card_with_options<R>(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    body_top_spacing: f32,
    min_height: Option<f32>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    card_with_options_and_metrics(
        ui,
        title,
        subtitle,
        body_top_spacing,
        min_height,
        add_contents,
    )
    .0
}

fn card_with_options_and_metrics<R>(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    body_top_spacing: f32,
    min_height: Option<f32>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (R, CardMetrics) {
    let available_width = ui.available_width();

    let response = egui::Frame::group(ui.style())
        .fill(theme::surface())
        .stroke(egui::Stroke::new(1.0, theme::border()))
        .inner_margin(egui::Margin::same(theme::CARD_PADDING))
        .rounding(egui::Rounding::same(theme::CORNER_RADIUS))
        .shadow(theme::card_shadow())
        .show(ui, |ui| {
            ui.set_min_width((available_width - theme::CARD_PADDING * 2.0).max(0.0));
            let content = ui.vertical(|ui| {
                ui.label(
                    RichText::new(title)
                        .strong()
                        .size(20.0)
                        .color(theme::text_primary()),
                );
                if !subtitle.is_empty() {
                    ui.label(
                        RichText::new(subtitle)
                            .size(13.0)
                            .color(theme::text_secondary()),
                    );
                }
                ui.add_space(body_top_spacing);
                add_contents(ui)
            });
            let actual_inner_height = content.response.rect.height();
            if let Some(min_height) = min_height {
                // `ui.vertical(...)` and the following spacer are two separate layout items.
                // egui inserts one vertical item gap between them, so we subtract it here to
                // keep the final displayed card height aligned with the target height.
                let implicit_gap = ui.spacing().item_spacing.y;
                let filler_height = (min_height - actual_inner_height - implicit_gap).max(0.0);
                if filler_height > 0.0 {
                    ui.add_space(filler_height);
                }
            }
            (content.inner, actual_inner_height)
        });

    let (inner, actual_inner_height) = response.inner;
    (
        inner,
        CardMetrics {
            actual_height: actual_inner_height + theme::CARD_PADDING * 2.0,
            displayed_height: response.response.rect.height(),
        },
    )
}

pub fn form_grid<R>(ui: &mut Ui, id_source: impl Hash, add_rows: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Grid::new(id_source)
        .num_columns(2)
        .spacing(egui::vec2(14.0, 10.0))
        .min_col_width(FORM_LABEL_WIDTH)
        .show(ui, add_rows)
        .inner
}

pub fn form_row<R>(ui: &mut Ui, label: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    form_label_cell(ui, label);
    let result = add_contents(ui);
    ui.end_row();
    result
}

pub fn form_note_row(ui: &mut Ui, note: impl Into<String>) {
    form_row(ui, "", |ui| {
        ui.add(
            egui::Label::new(
                RichText::new(note.into())
                    .size(12.5)
                    .color(theme::text_secondary()),
            )
            .wrap(),
        );
    });
}

pub fn text_row(ui: &mut Ui, label: &str, value: &mut String, hint: &str) {
    form_row(ui, label, |ui| {
        text_field(ui, value, hint);
    });
}

pub fn optional_text_row(ui: &mut Ui, label: &str, value: &mut Option<String>, hint: &str) {
    let mut buffer = value.clone().unwrap_or_default();
    text_row(ui, label, &mut buffer, hint);
    if buffer.trim().is_empty() {
        *value = None;
    } else {
        *value = Some(buffer);
    }
}

pub fn number_row(ui: &mut Ui, label: &str, value: &mut u64, suffix: &str, speed: f64) {
    form_row(ui, label, |ui| {
        number_field(ui, value, suffix, speed);
    });
}

pub fn text_field(ui: &mut Ui, value: &mut String, hint: &str) {
    ui.add_sized(
        [FORM_TEXT_WIDTH, 30.0],
        TextEdit::singleline(value)
            .hint_text(hint)
            .margin(egui::Margin::symmetric(8.0, 4.0))
            .vertical_align(egui::Align::Center),
    );
}

pub fn number_field(ui: &mut Ui, value: &mut u64, suffix: &str, speed: f64) {
    ui.add_sized(
        [FORM_NUMBER_WIDTH, 30.0],
        DragValue::new(value)
            .speed(speed)
            .range(0..=u64::MAX)
            .suffix(suffix),
    );
}

pub fn hotkey_capture_field(
    ui: &mut Ui,
    value: Option<&str>,
    placeholder: &str,
    is_recording: bool,
) -> egui::Response {
    let desired_width = FORM_TEXT_WIDTH.min(ui.available_width()).max(140.0);
    let desired_size = egui::vec2(desired_width, 30.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    let has_focus = ui.memory_mut(|memory| {
        memory.interested_in_focus(response.id);
        memory.has_focus(response.id)
    });

    if response.clicked() {
        response.request_focus();
    }

    if is_recording && has_focus {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    tab: true,
                    escape: true,
                    ..Default::default()
                },
            );
        });
    }

    let has_value = value.is_some_and(|text| !text.trim().is_empty());
    let display_text = value.unwrap_or(placeholder);

    let (fill, stroke, text_color) = if is_recording {
        (
            theme::primary_soft(),
            egui::Stroke::new(1.0, theme::primary()),
            theme::primary_dark(),
        )
    } else if has_focus {
        (
            theme::surface(),
            egui::Stroke::new(1.0, theme::primary_soft_border()),
            if has_value {
                theme::text_primary()
            } else {
                theme::text_secondary()
            },
        )
    } else if response.hovered() {
        (
            theme::hover_fill(),
            egui::Stroke::new(1.0, theme::primary_soft_border()),
            if has_value {
                theme::text_primary()
            } else {
                theme::text_secondary()
            },
        )
    } else {
        (
            theme::surface(),
            egui::Stroke::new(1.0, theme::border()),
            if has_value {
                theme::text_primary()
            } else {
                theme::text_secondary()
            },
        )
    };

    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            egui::Rounding::same(theme::CORNER_RADIUS),
            fill,
            stroke,
        );

        let font_id = egui::TextStyle::Body.resolve(ui.style());
        let text_position = egui::pos2(rect.left() + 10.0, rect.center().y);
        ui.painter().text(
            text_position,
            egui::Align2::LEFT_CENTER,
            display_text,
            font_id,
            text_color,
        );
    }

    response
}

fn form_label_cell(ui: &mut Ui, label: &str) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add_sized(
            [FORM_LABEL_WIDTH, 30.0],
            egui::Label::new(RichText::new(label).size(13.5).color(theme::text_primary())),
        );
    });
}
