mod token;

pub use setup::{install_client_secret, validate_client_secret};
pub use token::{authenticator, login};

pub const SCOPES: [&str; 3] = [
    "https://www.googleapis.com/auth/calendar.events.readonly",
    "https://www.googleapis.com/auth/meetings.space.readonly",
    "https://www.googleapis.com/auth/drive.readonly",
];

pub mod setup;
