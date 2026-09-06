use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Region {
    label: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Preview {
    Wireframe { elements: Vec<Region> },
    Palette { colors: Vec<String>, sample: String },
    Ascii { text: String },
}
impl Preview {
    pub(super) fn valid(&self) -> bool {
        match self {
            Self::Wireframe { elements } => {
                !elements.is_empty()
                    && elements.len() <= 16
                    && elements.iter().all(|e| {
                        !e.label.trim().is_empty()
                            && e.label.chars().count() <= 60
                            && [e.x, e.y, e.width, e.height].iter().all(|v| v.is_finite())
                            && e.x >= 0.0
                            && e.y >= 0.0
                            && e.width >= 5.0
                            && e.height >= 5.0
                            && e.x + e.width <= 100.0
                            && e.y + e.height <= 100.0
                    })
            }
            Self::Palette { colors, sample } => {
                (2..=8).contains(&colors.len())
                    && colors.iter().all(|c| {
                        c.len() == 7
                            && c.starts_with('#')
                            && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
                    })
                    && !sample.trim().is_empty()
                    && sample.chars().count() <= 160
            }
            Self::Ascii { text } => {
                !text.trim().is_empty()
                    && text.len() <= 6000
                    && text.lines().count() <= 40
                    && text.lines().all(|line| line.chars().count() <= 120)
            }
        }
    }
}
pub(super) fn schema() -> Value {
    json!({"description":"Optional visual choice. Use wireframe for layout decisions (coordinates and sizes from 0 to 100), palette for color directions, or ascii for monospaced diagrams. Previews are illustrative, not screenshots or implemented UI. Keep alternatives comparable.","anyOf":[
        {"type":"object","required":["type","elements"],"additionalProperties":false,"properties":{"type":{"const":"wireframe"},"elements":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"object","required":["label","x","y","width","height"],"additionalProperties":false,"properties":{"label":{"type":"string","maxLength":60},"x":{"type":"number","minimum":0,"maximum":95},"y":{"type":"number","minimum":0,"maximum":95},"width":{"type":"number","minimum":5,"maximum":100},"height":{"type":"number","minimum":5,"maximum":100}}}}}},
        {"type":"object","required":["type","colors","sample"],"additionalProperties":false,"properties":{"type":{"const":"palette"},"colors":{"type":"array","minItems":2,"maxItems":8,"items":{"type":"string","pattern":"^#[a-fA-F0-9]{6}$"}},"sample":{"type":"string","maxLength":160}}},
        {"type":"object","required":["type","text"],"additionalProperties":false,"properties":{"type":{"const":"ascii"},"text":{"type":"string","maxLength":6000}}}
    ]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visual_questions_reject_overflow_and_executable_content() {
        let good: Preview = serde_json::from_value(json!({"type":"wireframe","elements":[{"label":"Menu","x":0,"y":0,"width":25,"height":100}]})).unwrap();
        assert!(good.valid());
        let bad: Preview = serde_json::from_value(json!({"type":"wireframe","elements":[{"label":"Menu","x":90,"y":0,"width":25,"height":100}]})).unwrap();
        assert!(!bad.valid());
        assert!(serde_json::from_value::<Preview>(
            json!({"type":"html","html":"<script>run()</script>"})
        )
        .is_err());
        assert!(!Preview::Palette {
            colors: vec!["url(https://invalid)".into(), "#ffffff".into()],
            sample: "Example".into()
        }
        .valid());
    }
}
