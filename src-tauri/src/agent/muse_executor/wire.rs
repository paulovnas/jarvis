use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct Messages {
    items: HashMap<String, (String, String)>,
    generation: Vec<String>,
    generation_cursors: HashSet<String>,
}

impl Messages {
    pub(super) fn take_generation(&mut self, cursor: &str) -> Option<Vec<String>> {
        self.generation_cursors
            .insert(cursor.into())
            .then(|| std::mem::take(&mut self.generation))
    }

    pub(super) fn apply(&mut self, method: &str, params: &Value) -> Result<Vec<Value>, String> {
        if method == "item/delta" {
            if params["field"]
                .as_str()
                .is_some_and(|field| field != "text")
            {
                return Ok(vec![]);
            }
            let id = params["itemId"].as_str().ok_or("Evento Muse sem itemId.")?;
            let Some((kind, text)) = self.items.get_mut(id) else {
                return Ok(vec![]);
            };
            let delta = params["delta"].as_str().ok_or("Texto Muse inválido.")?;
            if text.len().saturating_add(delta.len()) > 4 * 1024 * 1024 {
                return Err("A resposta Muse excedeu o limite local de texto.".into());
            }
            text.push_str(delta);
            let field = if kind == "reasoning" {
                "thinking"
            } else {
                "text"
            };
            let mut payload = json!({"type":"content_block_delta","index":0,"delta":{"type":format!("{field}_delta")}});
            payload["delta"][field] = json!(delta);
            return Ok(vec![json!({"type":"stream_event","event":payload})]);
        }
        if !matches!(method, "item/started" | "item/completed") {
            return Ok(vec![]);
        }
        let item = &params["item"];
        let kind = item["kind"].as_str().unwrap_or_default();
        if method == "item/completed" && kind == "toolCall" {
            self.generation.clear();
        }
        if !matches!(kind, "agentMessage" | "reasoning") {
            return Ok(vec![]);
        }
        let id = item["itemId"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("Mensagem Muse sem itemId.")?;
        if !self.generation.iter().any(|message| message == id) {
            self.generation.push(id.into());
        }
        if method == "item/started" {
            if self.items.len() >= 64 {
                return Err("O Muse excedeu a fila de mensagens.".into());
            }
            self.items.insert(
                id.into(),
                (
                    kind.into(),
                    item["text"].as_str().unwrap_or_default().into(),
                ),
            );
            return Ok(vec![
                json!({"type":"stream_event","event":{"type":"message_start","message":{"id":id}}}),
            ]);
        }
        let previous = self.items.remove(id);
        let text = item["text"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| previous.map(|(_, text)| text))
            .unwrap_or_default();
        let block = if kind == "reasoning" {
            json!({"type":"thinking","thinking":text})
        } else {
            json!({"type":"text","text":text})
        };
        Ok(vec![
            json!({"type":"assistant","uuid":id,"message":{"id":id,"role":"assistant","content":[block]}}),
            json!({"type":"stream_event","event":{"type":"message_stop"}}),
        ])
    }
}

pub(super) fn tool(item: &Value) -> Option<(String, String, Value)> {
    if item["kind"] != "toolCall" {
        return None;
    }
    item["callId"].as_str().filter(|id| !id.is_empty())?;
    // Provider call IDs can repeat across turns; MSP item IDs identify durable receipts.
    let id = item["itemId"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))?;
    let name = item["tool"]
        .as_str()?
        .strip_prefix("mcp__jarvis__")?
        .to_owned();
    if name.is_empty()
        || name.len() > 200
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let args: Value = serde_json::from_str(item["args"].as_str()?).ok()?;
    args.is_object().then(|| (id.into(), name, args))
}

pub(super) fn input(value: &Value) -> Result<Value, String> {
    if let Some(text) = value.as_str() {
        return Ok(json!([{"type":"text","text":text}]));
    }
    let blocks = value.as_array().ok_or("Entrada Muse inválida.")?;
    blocks.iter().map(|block| {
        match block["type"].as_str() {
            Some("text") if block["text"].is_string() => Ok(json!({"type":"text","text":block["text"]})),
            Some("image") if block["source"]["type"] == "base64"
                && block["source"]["media_type"].is_string() && block["source"]["data"].is_string() => {
                Ok(json!({"type":"image","mediaType":block["source"]["media_type"],"base64Data":block["source"]["data"]}))
            }
            _ => Err("O anexo não é compatível com a entrada nativa Muse.".into()),
        }
    }).collect::<Result<Vec<_>, String>>().map(Value::Array)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_messages_commit_exact_text_once_and_keep_reasoning_separate() {
        let mut messages = Messages::default();
        let started = messages
            .apply(
                "item/started",
                &json!({"item":{"itemId":"a","kind":"agentMessage","text":""}}),
            )
            .unwrap();
        assert_eq!(started[0]["event"]["message"]["id"], "a");
        let delta = messages
            .apply(
                "item/delta",
                &json!({"itemId":"a","field":"text","delta":"Olá"}),
            )
            .unwrap();
        assert_eq!(delta[0]["event"]["delta"]["text"], "Olá");
        let done = messages.apply("item/completed", &json!({"item":{"itemId":"a","kind":"agentMessage","text":"Olá mundo","messagePhase":"finalAnswer"}})).unwrap();
        assert_eq!(done[0]["message"]["content"][0]["text"], "Olá mundo");
        let reasoning = messages
            .apply(
                "item/completed",
                &json!({"item":{"itemId":"r","kind":"reasoning","text":"Plano"}}),
            )
            .unwrap();
        assert_eq!(reasoning[0]["message"]["content"][0]["thinking"], "Plano");
        assert!(messages
            .apply(
                "item/started",
                &json!({"item":{"itemId":"t","kind":"toolCall"}})
            )
            .unwrap()
            .is_empty());
        assert_eq!(
            messages.take_generation("usage-1"),
            Some(vec!["a".into(), "r".into()])
        );
        assert!(messages.take_generation("usage-1").is_none());
        assert_eq!(messages.take_generation("usage-2"), Some(vec![]));
    }

    #[test]
    fn only_correlated_jarvis_tool_calls_with_complete_object_arguments_are_admitted() {
        let call = tool(&json!({"kind":"toolCall","itemId":"item-1","callId":"native-1","tool":"mcp__jarvis__read_file","args":"{\"path\":\"a.txt\"}"})).unwrap();
        assert_eq!(
            call,
            ("item-1".into(), "read_file".into(), json!({"path":"a.txt"}))
        );
        let later = tool(&json!({"kind":"toolCall","itemId":"item-2","callId":"native-1","tool":"mcp__jarvis__read_file","args":"{\"path\":\"a.txt\"}"})).unwrap();
        assert_ne!(call.0, later.0);
        for item in [
            json!({"kind":"toolCall","tool":"mcp__jarvis__write_file","args":"{}"}),
            json!({"kind":"toolCall","callId":"x","tool":"bash","args":"{}"}),
            json!({"kind":"toolCall","callId":"x","tool":"mcp__jarvis__write_file","args":"[1]"}),
            json!({"kind":"toolCall","callId":"x","tool":"mcp__jarvis__write_file","args":"{"}),
        ] {
            assert!(tool(&item).is_none());
        }
    }

    #[test]
    fn images_use_msp_typed_input_without_losing_user_text() {
        assert_eq!(
            input(&json!("Somente texto")).unwrap(),
            json!([{"type":"text","text":"Somente texto"}])
        );
        let value = input(&json!([
            {"type":"text","text":"Verifique"},
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":"YWJj"}}
        ]))
        .unwrap();
        assert_eq!(value[0], json!({"type":"text","text":"Verifique"}));
        assert_eq!(
            value[1],
            json!({"type":"image","mediaType":"image/png","base64Data":"YWJj"})
        );
        assert!(input(&json!([{"type":"unknown"}])).is_err());
    }
}
