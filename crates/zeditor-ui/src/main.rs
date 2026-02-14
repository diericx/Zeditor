use zeditor_ui::app::App;

fn dark_theme(_app: &App) -> iced::Theme {
    iced::Theme::Dark
}

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(dark_theme)
        .run()
}
