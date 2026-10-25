//! Read server definitions from the config files other tools already own.
//!
//! Grove never writes these files. The scanner is deliberately forgiving: an absent file is simply a
//! harness the user does not use, and a file it cannot parse becomes a [`McpConfigWarning`] row so the
//! panel can still show everything the other files declare.

use super::{McpConfigWarning, McpDiscovery, McpServerView, McpService, McpTransport};
use crate::paths::display_path;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// How a config file is shaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `{ "mcpServers": { … } }` — Claude Code's `.mcp.json` and `~/.claude.json`.
    JsonMcpServers,
    /// `{ "mcp": { … } }` — OpenCode, with `mcpServers` accepted as an alias.
    JsonOpencode,
    /// `[mcp_servers.<name>]` — Codex.
    TomlMcpServers,
}

struct SourceSpec {
    /// The label the panel groups by, e.g. `.mcp.json` or `~/.codex/config.toml`.
    label: String,
    path: PathBuf,
    shape: Shape,
}

pub(crate) fn discover(service: &McpService) -> McpDiscovery {
    let config = service.config();
    let mut sources = Vec::new();
    let project = service.project_root();
    let home = super::home_dir(config.home.as_deref());

    if let Some(project) = &project {
        sources.push(SourceSpec {
            label: ".mcp.json".to_string(),
            path: project.join(".mcp.json"),
            shape: Shape::JsonMcpServers,
        });
    }
    sources.push(SourceSpec {
        label: "~/.claude.json".to_string(),
        path: home.join(".claude.json"),
        shape: Shape::JsonMcpServers,
    });
    sources.push(SourceSpec {
        label: "~/.codex/config.toml".to_string(),
        path: home.join(".codex").join("config.toml"),
        shape: Shape::TomlMcpServers,
    });
    if let Some(project) = &project {
        sources.push(SourceSpec {
            label: "opencode.json".to_string(),
            path: project.join("opencode.json"),
            shape: Shape::JsonOpencode,
        });
    }
    if let Some(adapter) = &config.adapter_config {
        let name = adapter
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config".to_string());
        let shape = match adapter.extension() {
            Some(extension) if extension.eq_ignore_ascii_case("toml") => Shape::TomlMcpServers,
            _ => Shape::JsonMcpServers,
        };
        sources.push(SourceSpec {
            label: format!("adapter:{name}"),
            path: adapter.clone(),
            shape,
        });
    }

    let mut servers: Vec<McpServerView> = Vec::new();
    let mut warnings = Vec::new();

    for source in sources {
        let raw = match read_if_present(&source.path) {
            Ok(Some(raw)) => raw,
            // A harness the user does not use is not a warning.
            Ok(None) => continue,
            Err(err) => {
                warnings.push(McpConfigWarning {
                    source: source.label.clone(),
                    message: format!("could not read {}: {err}", display_path(&source.path)),
                });
                continue;
            }
        };

        let definitions = match source.shape {
            Shape::JsonMcpServers => parse_json(&raw, &["mcpServers"]),
            Shape::JsonOpencode => parse_json(&raw, &["mcp", "mcpServers"]),
            Shape::TomlMcpServers => parse_toml(&raw),
        };
        let definitions = match definitions {
            Ok(definitions) => definitions,
            Err(message) => {
                warnings.push(McpConfigWarning {
                    source: source.label,
                    message,
                });
                continue;
            }
        };

        for (name, definition) in definitions {
            match normalise(&name, &definition, &source.label) {
                Ok(server) => merge(&mut servers, server),
                Err(message) => warnings.push(McpConfigWarning {
                    source: source.label.clone(),
                    message: format!("{name}: {message}"),
                }),
            }
        }
    }

    McpDiscovery { servers, warnings }
}

/// `Ok(None)` when the file does not exist; every other read problem is an error worth reporting.
fn read_if_present(path: &Path) -> Result<Option<String>, std::io::Error> {
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Every `{name: definition}` pair under the first key that is present, sorted by name so discovery
/// output is stable across runs and platforms.
fn parse_json(raw: &str, keys: &[&str]) -> Result<Vec<(String, Value)>, String> {
    let value: Value = serde_json::from_str(raw).map_err(|err| format!("invalid JSON: {err}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "expected a JSON object at the top level".to_string())?;
    for key in keys {
        let Some(entry) = object.get(*key) else {
            continue;
        };
        let map = entry
            .as_object()
            .ok_or_else(|| format!("`{key}` must be an object"))?;
        let mut definitions: Vec<(String, Value)> = map
            .iter()
            .map(|(name, definition)| (name.clone(), definition.clone()))
            .collect();
        definitions.sort_by(|a, b| a.0.cmp(&b.0));
        return Ok(definitions);
    }
    // A valid config with no MCP section is not a problem.
    Ok(Vec::new())
}

fn parse_toml(raw: &str) -> Result<Vec<(String, Value)>, String> {
    let value: toml::Value = toml::from_str(raw).map_err(|err| format!("invalid TOML: {err}"))?;
    let Some(section) = value.get("mcp_servers") else {
        return Ok(Vec::new());
    };
    let table = section
        .as_table()
        .ok_or_else(|| "`mcp_servers` must be a table".to_string())?;
    let mut definitions = Vec::with_capacity(table.len());
    for (name, definition) in table {
        let definition = serde_json::to_value(definition)
            .map_err(|err| format!("{name}: could not read the definition: {err}"))?;
        definitions.push((name.clone(), definition));
    }
    definitions.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(definitions)
}

/// Turn one raw definition into a row. The keys Grove understands are `type`/`transport`, `command`
/// (a string, or a word array as OpenCode writes it), `args` and `url`; anything else is ignored.
fn normalise(name: &str, definition: &Value, source: &str) -> Result<McpServerView, String> {
    let object = definition
        .as_object()
        .ok_or_else(|| "the definition must be an object".to_string())?;

    let declared = object
        .get("type")
        .or_else(|| object.get("transport"))
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    let url = object
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string);

    // `command` is a string for Claude/Codex and a full word array for OpenCode.
    let (command, mut args) = match object.get("command") {
        Some(Value::String(command)) if !command.trim().is_empty() => {
            (Some(command.trim().to_string()), Vec::new())
        }
        Some(Value::Array(words)) => {
            let mut words = words.iter().filter_map(|word| word.as_str());
            let command = words
                .next()
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(str::to_string);
            (command, words.map(str::to_string).collect())
        }
        _ => (None, Vec::new()),
    };
    if let Some(Value::Array(extra)) = object.get("args") {
        args.extend(
            extra
                .iter()
                .filter_map(|arg| arg.as_str())
                .map(str::to_string),
        );
    }

    // An explicit `type` wins; otherwise a URL with no command means HTTP, and anything else is stdio.
    let http = match declared.as_deref() {
        Some("http" | "sse" | "streamable-http" | "streamable_http" | "remote") => true,
        Some("stdio" | "local" | "process") => false,
        _ => url.is_some() && command.is_none(),
    };

    if http {
        let url = url.ok_or_else(|| "an HTTP server needs a `url`".to_string())?;
        Ok(McpServerView {
            name: name.to_string(),
            source: source.to_string(),
            sources: vec![source.to_string()],
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            url: Some(url),
        })
    } else {
        let command = command.ok_or_else(|| "a stdio server needs a `command`".to_string())?;
        Ok(McpServerView {
            name: name.to_string(),
            source: source.to_string(),
            sources: vec![source.to_string()],
            transport: McpTransport::Stdio,
            command: Some(command),
            args,
            url: None,
        })
    }
}

/// Identical definitions collapse into one row that lists every source that declares them.
fn merge(servers: &mut Vec<McpServerView>, candidate: McpServerView) {
    if let Some(existing) = servers
        .iter_mut()
        .find(|server| server.same_definition(&candidate))
    {
        if !existing.sources.contains(&candidate.source) {
            existing.sources.push(candidate.source);
        }
        return;
    }
    servers.push(candidate);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_command_string_and_an_args_array_normalise_to_stdio() {
        let server = normalise(
            "filesystem",
            &json!({ "command": "npx", "args": ["-y", "server-filesystem"] }),
            ".mcp.json",
        )
        .unwrap();
        assert_eq!(server.transport, McpTransport::Stdio);
        assert_eq!(server.command.as_deref(), Some("npx"));
        assert_eq!(server.args, ["-y", "server-filesystem"]);
        assert_eq!(server.url, None);
    }

    #[test]
    fn an_opencode_word_array_is_split_into_command_and_args() {
        let server = normalise(
            "postgres",
            &json!({ "type": "local", "command": ["npx", "-y", "pg-mcp"] }),
            "opencode.json",
        )
        .unwrap();
        assert_eq!(server.command.as_deref(), Some("npx"));
        assert_eq!(server.args, ["-y", "pg-mcp"]);
    }

    #[test]
    fn a_url_is_http_and_sse_is_normalised_to_http() {
        for definition in [
            json!({ "url": "https://mcp.example.com/docs" }),
            json!({ "type": "sse", "url": "https://mcp.example.com/docs" }),
            json!({ "type": "http", "url": "https://mcp.example.com/docs" }),
        ] {
            let server = normalise("docs", &definition, "~/.claude.json").unwrap();
            assert_eq!(server.transport, McpTransport::Http);
            assert_eq!(server.url.as_deref(), Some("https://mcp.example.com/docs"));
            assert_eq!(server.command, None);
        }
    }

    #[test]
    fn a_declared_stdio_server_without_a_command_is_refused() {
        let error = normalise("broken", &json!({ "type": "stdio" }), ".mcp.json").unwrap_err();
        assert!(error.contains("stdio"), "{error}");
        let error = normalise("broken", &json!({ "type": "http" }), ".mcp.json").unwrap_err();
        assert!(error.contains("HTTP"), "{error}");
    }

    #[test]
    fn identical_definitions_merge_and_list_every_source() {
        let mut servers = Vec::new();
        merge(
            &mut servers,
            normalise("fs", &json!({ "command": "npx" }), ".mcp.json").unwrap(),
        );
        merge(
            &mut servers,
            normalise("fs", &json!({ "command": "npx" }), "~/.claude.json").unwrap(),
        );
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].sources, [".mcp.json", "~/.claude.json"]);
        assert_eq!(servers[0].source, ".mcp.json");
    }

    #[test]
    fn codex_toml_tables_are_read_and_converted() {
        let definitions = parse_toml(
            "[mcp_servers.linear]\ncommand = \"npx\"\nargs = [\"-y\", \"linear-mcp\"]\n",
        )
        .unwrap();
        assert_eq!(definitions.len(), 1);
        let server =
            normalise(&definitions[0].0, &definitions[0].1, "~/.codex/config.toml").unwrap();
        assert_eq!(server.name, "linear");
        assert_eq!(server.args, ["-y", "linear-mcp"]);
    }

    #[test]
    fn a_missing_or_sectionless_file_yields_nothing() {
        assert!(parse_json("{\"theme\":\"dark\"}", &["mcpServers"])
            .unwrap()
            .is_empty());
        assert!(parse_toml("model = \"gpt-5\"").unwrap().is_empty());
        assert!(parse_json("[]", &["mcpServers"]).is_err());
        assert!(parse_json("{\"mcpServers\":5}", &["mcpServers"]).is_err());
        assert!(parse_toml("mcp_servers = 3\n").is_err());
    }
}
