//! Визуальный язык OPORA: «ночная бумага».
//! Токены — здесь, чтобы список и график не пришлось потом перекрашивать.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    Vec2,
};

const GOLOS: &[u8] = include_bytes!("../assets/fonts/GolosText.ttf");
const LITERATA: &[u8] = include_bytes!("../assets/fonts/Literata.ttf");

/// Фон окна — тёплый уголь.
pub const BG: Color32 = Color32::from_rgb(0x17, 0x16, 0x1A);
/// Поля ввода, наведение.
pub const SURFACE: Color32 = Color32::from_rgb(0x22, 0x20, 0x27);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x2C, 0x29, 0x33);
/// Основной текст — тёплая бумага.
pub const TEXT: Color32 = Color32::from_rgb(0xED, 0xE7, 0xDD);
/// Подписи, второстепенное.
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8E, 0x87, 0x7E);
/// Единственный акцент: выбранное настроение, главная кнопка.
pub const ACCENT: Color32 = Color32::from_rgb(0xC8, 0xA1, 0x5A);
/// Линии вместо рамок.
pub const LINE: Color32 = Color32::from_rgb(0x35, 0x31, 0x3A);

const SERIF: &str = "serif";

pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SERIF.into()))
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("golos".to_owned(), FontData::from_static(GOLOS).into());
    fonts.font_data.insert(
        "literata".to_owned(),
        FontData::from_static(LITERATA).into(),
    );

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "golos".to_owned());
    fonts
        .families
        .entry(FontFamily::Name(SERIF.into()))
        .or_default()
        .insert(0, "literata".to_owned());

    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);

    ctx.all_styles_mut(|style| {
        apply_style(style);
    });
}

fn apply_style(style: &mut egui::Style) {
    style.text_styles = [
        (TextStyle::Heading, serif(30.0)),
        (TextStyle::Body, body(15.0)),
        (TextStyle::Button, body(15.0)),
        (TextStyle::Small, body(12.5)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();

    let visuals = &mut style.visuals;
    visuals.dark_mode = true;
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.extreme_bg_color = SURFACE;
    visuals.faint_bg_color = SURFACE;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, TEXT);

    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::same(4);
    }

    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, TEXT_DIM);
    visuals.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.bg_fill = SURFACE_HOVER;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.active.bg_fill = SURFACE_HOVER;

    style.spacing.item_spacing = Vec2::new(12.0, 12.0);
    style.spacing.button_padding = Vec2::new(16.0, 8.0);
    style.spacing.interact_size.y = 30.0;
}
