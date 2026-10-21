//! Bounded read-only mission program intake. This is not the game's compiler or VM.
//! Actor serialization follows the pinned MIT mafia-workbench scene/detector layouts;
//! command lines are inspected without evaluating expressions or invoking game actions.
use crate::{Error, Result, decode_windows1250};
use std::collections::BTreeMap;

const MAX_SOURCE: usize = 1_000_000;
const MAX_STATEMENTS: usize = 65_536;

/// Decode only the observed detector v1 and human v6 embedded program layouts.
/// An unknown actor/version is reported as unsupported; a recognized truncated
/// program is an error. Remaining actor state stays opaque.
pub(crate) fn parse_actor_script(kind: u32, data: &[u8]) -> Result<Option<String>> {
    let known_minimum = match (kind, data.first()) {
        (5, Some(1)) => Some(14),
        (27, Some(6)) => Some(83),
        _ => None,
    };
    if known_minimum.is_some_and(|minimum| data.len() < minimum) {
        return Err(Error("truncated known actor program header".into()));
    }
    let offset = match kind {
        5 if data.len() >= 10 && data[0] == 1 && data[5] == 1 && data[6..10] == [0; 4] => 10,
        27 if data.len() >= 79
            && data[0] == 6
            && data[73] == 1
            && data[74] == 1
            && data[75..79] == [0; 4] =>
        {
            79
        }
        _ => return Ok(None),
    };
    let raw = data
        .get(offset..offset + 4)
        .ok_or_else(|| Error("truncated actor program length".into()))?;
    let length = u32::from_le_bytes(raw.try_into().unwrap()) as usize;
    if length > MAX_SOURCE {
        return Err(Error("actor program exceeds 1 MB quota".into()));
    }
    let source = data
        .get(offset + 4..offset + 4 + length)
        .ok_or_else(|| Error("truncated actor program source".into()))?;
    if source.contains(&0) {
        return Err(Error("NUL in actor program source".into()));
    }
    Ok(Some(decode_windows1250(source)?))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptStatement {
    /// One-based source line, retained for explicit unsupported-command reports.
    pub line: usize,
    /// ASCII-normalized command. Arguments remain original text, including quotes.
    pub command: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Default)]
pub struct ScriptProgram {
    pub statements: Vec<ScriptStatement>,
}

impl ScriptProgram {
    /// A diagnostic command histogram. Presence here does not imply VM support.
    pub fn command_counts(&self) -> BTreeMap<&str, usize> {
        let mut counts = BTreeMap::new();
        for statement in &self.statements {
            *counts.entry(statement.command.as_str()).or_default() += 1;
        }
        counts
    }
}

/// Read line-oriented source with case-insensitive commands and `//` comments.
/// Quoted strings preserve commas, slashes and whitespace. Expressions and brace
/// blocks remain source text; they are never silently interpreted as no-ops.
pub fn parse_script(source: &str) -> Result<ScriptProgram> {
    if source.len() > MAX_SOURCE {
        return Err(Error("program exceeds 1 MB quota".into()));
    }
    if source
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\r' | '\n'))
    {
        return Err(Error("control character in program source".into()));
    }
    let mut statements = Vec::new();
    for (line, raw) in source.split('\n').enumerate() {
        let mut quoted = false;
        let mut end = raw.len();
        let mut chars = raw.char_indices().peekable();
        while let Some((offset, character)) = chars.next() {
            if character == '"' {
                quoted = !quoted;
            } else if !quoted && character == '/' && chars.peek().map(|(_, c)| *c) == Some('/') {
                end = offset;
                break;
            }
        }
        if quoted {
            return Err(Error(format!(
                "unterminated program string at line {}",
                line + 1
            )));
        }
        let text = raw[..end].trim();
        if text.is_empty() {
            continue;
        }
        if statements.len() == MAX_STATEMENTS {
            return Err(Error("program statement quota exceeded".into()));
        }
        let split = text.find(char::is_whitespace).unwrap_or(text.len());
        let command = &text[..split];
        if !command.is_ascii() || command.len() > 128 {
            return Err(Error(format!(
                "unsupported program command token at line {}",
                line + 1
            )));
        }
        statements.push(ScriptStatement {
            line: line + 1,
            command: command.to_ascii_lowercase(),
            arguments: text[split..].trim().to_owned(),
        });
    }
    Ok(ScriptProgram { statements })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn detector(source: &[u8]) -> Vec<u8> {
        let mut bytes = vec![1, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        bytes.extend((source.len() as u32).to_le_bytes());
        bytes.extend(source);
        bytes
    }
    #[test]
    fn known_actor_layouts_and_bad_lengths() {
        let bytes = detector(b"end");
        assert_eq!(
            parse_actor_script(5, &bytes).unwrap().as_deref(),
            Some("end")
        );
        for end in 1..bytes.len() {
            assert!(parse_actor_script(5, &bytes[..end]).is_err());
        }
        let mut human = vec![0; 79];
        human[0] = 6;
        human[73] = 1;
        human[74] = 1;
        human.extend(3u32.to_le_bytes());
        human.extend(b"end");
        assert_eq!(
            parse_actor_script(27, &human).unwrap().as_deref(),
            Some("end")
        );
        human[74] = 2;
        assert!(parse_actor_script(27, &human).unwrap().is_none());
        assert!(parse_actor_script(4, &bytes).unwrap().is_none());
        assert!(parse_actor_script(5, &detector(b"x\0")).is_err());
        let mut oversized = bytes;
        oversized[10..14].copy_from_slice(&1_000_001u32.to_le_bytes());
        assert!(parse_actor_script(5, &oversized).is_err());
    }
    #[test]
    fn lines_comments_strings_and_opaque_expressions() {
        let program = parse_script("// demo\r\nFINDActor 0,\"a//b,c\" // comment\nlabel 42\nif flt[0]>2,42,-1\n{ branch\nEND\n").unwrap();
        assert_eq!(
            program.statements[0],
            ScriptStatement {
                line: 2,
                command: "findactor".into(),
                arguments: "0,\"a//b,c\"".into()
            }
        );
        assert_eq!(program.statements[2].arguments, "flt[0]>2,42,-1");
        assert_eq!(program.statements[3].command, "{");
        assert_eq!(program.command_counts()["end"], 1);
        assert!(parse_script("findactor 0,\"open").is_err());
        assert!(parse_script("end\0").is_err());
        assert!(parse_script(&"x".repeat(MAX_SOURCE + 1)).is_err());
    }
}
