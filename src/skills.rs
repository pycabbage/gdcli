use crate::cli::Topic;
use std::io::Write;

const SKILL: &str = include_str!("../skills/gdcli/SKILL.md");
const MEET: &str = include_str!("../skills/gdcli/references/meet.md");
const CALENDAR: &str = include_str!("../skills/gdcli/references/calendar.md");
const DOCS: &str = include_str!("../skills/gdcli/references/docs.md");
const AUTH: &str = include_str!("../skills/gdcli/references/auth.md");

pub fn print(topic: Option<Topic>, out: &mut impl Write) -> std::io::Result<()> {
    let parts: &[&str] = match topic {
        None => &[SKILL],
        Some(Topic::Meet) => &[MEET],
        Some(Topic::Calendar) => &[CALENDAR],
        Some(Topic::Docs) => &[DOCS],
        Some(Topic::Auth) => &[AUTH],
        Some(Topic::Full) => &[SKILL, MEET, CALENDAR, DOCS, AUTH],
    };
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.write_all(b"\n")?;
        }
        out.write_all(part.as_bytes())?;
    }
    Ok(())
}
