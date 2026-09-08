use super::*;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Icon {
    Bot,
    Workflow,
    Route,
    Brain,
    Search,
    Code,
    Palette,
    Shield,
    Terminal,
    Wrench,
    Book,
    Sparkles,
    Target,
    Pen,
    Lightbulb,
    Rocket,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    Blue,
    Green,
    Cyan,
    Yellow,
    Red,
    Purple,
    Neutral,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Appearance {
    pub icon: Icon,
    pub color: Color,
}
