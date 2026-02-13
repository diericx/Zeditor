use zeditor_ui::app::App;

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Zeditor")
        .subscription(App::subscription)
        .run()
}
