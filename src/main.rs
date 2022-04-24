use iced::{executor, Application, Clipboard, Command, Element, Settings, Text, Column, Button};

mod ripper;

pub fn main() -> iced::Result {
    Hello::run(Settings::default())
}

#[derive(Debug, Default)]
struct Hello {
    start: iced::button::State,
}

#[derive(Debug, Clone)]
pub enum Message {
    StartClicked,
}

impl Application for Hello {
    type Executor = executor::Default;
    type Message = Message;
    type Flags = ();
    
    fn new(_flags: ()) -> (Hello, Command<Self::Message>) {
        (Hello { start: iced::button::State::new()}, Command::none())
    }
    
    fn title(&self) -> String {
        String::from("A cool application")
    }
    
    fn update(&mut self, message: Message, _: &mut Clipboard) -> Command<Self::Message> {
        match message {
            Message::StartClicked => ripper::extract(),
        }
        Command::none()
    }
    
    fn view(&mut self) -> Element<Self::Message> {
        let mut content = Column::new();
        content = content.push(Text::new("Hello, world!"));
        content = content.push(Button::new(&mut self.start, Text::new("start")).on_press(Message::StartClicked));
        content.into()
    }
}
