use eframe::egui::{self, Color32, RichText, Ui};

use crate::ui::theme;

#[derive(Debug, Clone)]
pub struct StatusPanelData {
    pub state_label: String,
    pub state_color: Color32,
    pub completed_actions: u64,
    pub elapsed_label: String,
}

pub fn render_status_panel(ui: &mut Ui, data: &StatusPanelData) {
    let width = ui.available_width();
    let inner_width = (width - theme::CARD_PADDING * 2.0).max(0.0);

    egui::Frame::group(ui.style())
        .fill(theme::surface())
        .stroke(egui::Stroke::new(1.0, theme::border()))
        .inner_margin(egui::Margin::symmetric(theme::CARD_PADDING, 12.0))
        .rounding(egui::Rounding::same(theme::CORNER_RADIUS))
        .shadow(theme::card_shadow())
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(inner_width, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                    ui.columns(6, |columns| {
                        render_centered_text(
                            &mut columns[0],
                            "运行状态",
                            false,
                            theme::text_secondary(),
                        );
                        render_centered_text(
                            &mut columns[1],
                            &data.state_label,
                            true,
                            data.state_color,
                        );
                        render_centered_text(
                            &mut columns[2],
                            "执行次数",
                            false,
                            theme::text_secondary(),
                        );
                        render_centered_text(
                            &mut columns[3],
                            &format!("{} 次", data.completed_actions),
                            true,
                            theme::text_primary(),
                        );
                        render_centered_text(
                            &mut columns[4],
                            "执行时长",
                            false,
                            theme::text_secondary(),
                        );
                        render_centered_text(
                            &mut columns[5],
                            &data.elapsed_label,
                            true,
                            theme::text_primary(),
                        );
                    });
                },
            );
        });
}

fn render_centered_text(ui: &mut Ui, text: &str, emphasize: bool, color: Color32) {
    let mut rich = RichText::new(text).color(color);
    rich = if emphasize {
        rich.size(17.0).strong()
    } else {
        rich.size(13.0)
    };

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 28.0),
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
        |ui| {
            ui.label(rich);
        },
    );
}
