use model::enums::*;
use std::fmt::{Display, Formatter, Error};

#[derive(Debug, Clone, PartialEq)]
pub struct NationDetails {
    pub nation: Nation,
    pub status: NationStatus,
    pub submitted: SubmissionStatus,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Nation {
    pub id: u32,
    pub name: & 'static str,
    pub era: Era,
}

impl Display for Nation {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{} {} ({})\n", self.era, self.name, self.id)
    }
}
