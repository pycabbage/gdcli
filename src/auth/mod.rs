mod secret;
mod token;

pub use secret::{install_client_secret, validate_client_secret};
pub use token::{Token, authenticator, login};

pub const SCOPES: [&str; 3] = [
    "https://www.googleapis.com/auth/calendar.events.readonly",
    "https://www.googleapis.com/auth/meetings.space.readonly",
    "https://www.googleapis.com/auth/drive.readonly",
];

pub mod setup;
