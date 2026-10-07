use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Add,
    Overlay,
    Difference,
}
impl BlendMode {
    pub const ALL: [Self; 6] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Add,
        Self::Overlay,
        Self::Difference,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
            Self::Add => "Add",
            Self::Overlay => "Overlay",
            Self::Difference => "Difference",
        }
    }
    pub(crate) fn is_normal(&self) -> bool {
        *self == Self::Normal
    }
}
