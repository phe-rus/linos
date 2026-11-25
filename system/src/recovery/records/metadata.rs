//! Bounded immutable metadata lists and current account compatibility.
pub mod accounts;
pub mod registry;
pub mod services;

use super::{invalid, state::decimal};
use std::io;

const LIMIT: usize = 16 * 1024 * 1024;

pub struct Requirements {
    pub accounts: accounts::Contract,
    pub services: services::Contract,
}

impl Requirements {
    pub fn load(artifacts: &super::deployment::artifacts::VerifiedArtifacts) -> io::Result<Self> {
        let accounts = accounts::Contract::parse(&artifacts.metadata("accounts")?)?;
        let services = services::Contract::parse(&artifacts.metadata("services")?)?;
        if artifacts.metadata("extensions")? != b"kind=linos-extensions\nformat=1\ncount=0\n" {
            return Err(invalid(
                "Only the canonical empty extension set is supported",
            ));
        }
        Ok(Self { accounts, services })
    }
}

pub(super) struct Fields<'a> {
    lines: std::str::Split<'a, char>,
    source: &'a [u8],
    consumed: usize,
}

impl<'a> Fields<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> io::Result<Self> {
        if bytes.is_empty()
            || bytes.len() > LIMIT
            || !bytes.ends_with(b"\n")
            || !bytes
                .iter()
                .all(|b| *b == b'\n' || (b' '..=b'~').contains(b))
        {
            return Err(invalid(
                "Metadata requires bounded ASCII and a final newline",
            ));
        }
        let text = std::str::from_utf8(bytes).map_err(invalid)?;
        Ok(Self {
            lines: text[..text.len() - 1].split('\n'),
            source: bytes,
            consumed: 0,
        })
    }
    pub(super) fn next(&mut self, expected: &str) -> io::Result<&'a str> {
        let line = self
            .lines
            .next()
            .ok_or_else(|| invalid("Missing metadata field"))?;
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| invalid("Missing metadata value"))?;
        self.consumed += line.len() + 1;
        if key != expected || value.is_empty() || value.len() > 4096 {
            return Err(invalid("Unknown, missing or out of order metadata field"));
        }
        Ok(value)
    }
    fn position(&self) -> usize {
        self.consumed
    }
    fn bytes_since(&self, start: usize) -> &[u8] {
        &self.source[start..self.consumed]
    }
    pub(super) fn number(&mut self, expected: &str, max: u64) -> io::Result<u64> {
        let value = decimal(self.next(expected)?)?;
        if value > max {
            return Err(invalid("Metadata number exceeds its bound"));
        }
        Ok(value)
    }
    pub(super) fn finish(mut self) -> io::Result<()> {
        if self.lines.next().is_some() {
            return Err(invalid("Extra or duplicate metadata fields"));
        }
        Ok(())
    }
}

fn id(value: &str) -> io::Result<String> {
    if value.len() > 64
        || !value
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
    {
        return Err(invalid("Invalid metadata identifier"));
    }
    Ok(value.into())
}
