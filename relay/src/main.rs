use iced::{
    run,
    widget::{column, text},
    Element,
};

fn main() -> iced::Result {
    iced::run("RELAY", update, view)
}

#[derive(Debug)]
enum Message {}

#[derive(Default)]
struct State {}

fn view(state: &State) -> Element<Message> {
    text("Hello, world").into()
}

fn update(state: &mut State, message: Message) {}
