use std::borrow::Cow;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum SubmissionStatus {
    NotSubmitted,
    PartiallySubmitted,
    Submitted,
    Unknown(u8),
}
impl SubmissionStatus {
    pub fn show(self) -> Cow<'static, str> {
        match self {
            SubmissionStatus::NotSubmitted => Cow::from("X"),
            SubmissionStatus::PartiallySubmitted => Cow::from("/"),
            SubmissionStatus::Submitted => Cow::from("✓"),
            SubmissionStatus::Unknown(x) => Cow::from(format!("{}", x)),
        }
    }

    pub fn from_int(x: u8) -> Self {
        match x {
            0 => SubmissionStatus::NotSubmitted,
            1 => SubmissionStatus::PartiallySubmitted,
            2 => SubmissionStatus::Submitted,
            _ => SubmissionStatus::Unknown(x),
        }
    }

    pub fn from_bool(x: bool) -> Self {
        if x { SubmissionStatus::Submitted } else { SubmissionStatus::NotSubmitted }
    }
}
