use crate::modem::sstv::{
    image::Channel,
    modes::ModeSpecification,
};

#[derive(Clone, Copy, Debug)]
pub enum State {
    Header { header_state: HeaderState },
    Line { y: usize, line_state: LineState },
}

impl Default for State {
    fn default() -> Self {
        State::Header {
            header_state: HeaderState::Leader1,
        }
    }
}

impl State {
    pub fn next(&self, mode: Option<&ModeSpecification>) -> Option<Self> {
        let mut state = *self;
        match &mut state {
            Self::Header { header_state } => {
                match header_state {
                    HeaderState::Leader1 => *header_state = HeaderState::LeaderBreak,
                    HeaderState::LeaderBreak => *header_state = HeaderState::Leader2,
                    HeaderState::Leader2 => *header_state = HeaderState::VisStart,
                    HeaderState::VisStart => {
                        *header_state = HeaderState::VisBit { bit: 0 };
                    }
                    HeaderState::VisBit { bit } => {
                        *bit += 1;
                        if *bit == 8 {
                            *header_state = HeaderState::VisStop;
                        }
                    }
                    HeaderState::VisStop => {
                        state = State::Line {
                            y: 0,
                            line_state: LineState::Sync,
                        }
                    }
                }
            }
            Self::Line { y, line_state } => {
                let mode = mode.expect("expected mode specification in line state");
                match line_state {
                    LineState::Sync => {
                        *line_state = LineState::Porch;
                    }
                    LineState::Porch => {
                        *line_state = LineState::Scan {
                            channel: Channel::default(),
                            x: 0,
                        }
                    }
                    LineState::Scan { channel, x } => {
                        *x += 1;
                        if *x == mode.pixels_per_line {
                            *line_state = LineState::Separator { channel: *channel };
                        }
                    }
                    LineState::Separator { channel } => {
                        if let Some(channel) = channel.next() {
                            *line_state = LineState::Scan { channel, x: 0 }
                        }
                        else {
                            *y += 1;
                            if *y == mode.num_lines {
                                return None;
                            }
                            *line_state = LineState::Sync;
                        }
                    }
                }
            }
        }

        Some(state)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HeaderState {
    Leader1,
    LeaderBreak,
    Leader2,
    VisStart,
    VisBit { bit: u8 },
    VisStop,
}

#[derive(Clone, Copy, Debug)]
pub enum LineState {
    Sync,
    Porch,
    Scan { channel: Channel, x: usize },
    Separator { channel: Channel },
}
