use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{io::Write, path::PathBuf};

pub(super) fn definition() -> super::ToolDefinition {
    super::ToolDefinition {
        name: "NotebookEdit".into(),
        description: "Read or edit Jupyter notebooks without losing metadata. Supports create, read, list_cells, add_cell, edit_cell, delete_cell, move_cell, clear_outputs. Does not execute cells.".into(),
        input_schema: json!({"type":"object","properties":{
            "path":{"type":"string"}, "operation":{"type":"string","enum":["create","read","list_cells","add_cell","edit_cell","delete_cell","move_cell","clear_outputs"]},
            "cell_index":{"type":"integer","minimum":0}, "cell_type":{"type":"string","enum":["code","markdown","raw"]},
            "source":{"type":"array","items":{"type":"string"}}, "metadata":{"type":"object"}, "new_index":{"type":"integer","minimum":0}
        },"required":["path","operation"]}),
    }
}

pub(super) fn execute(input: Value, cwd: &str) -> Result<Value> {
    let path = input["path"].as_str().context("path is required")?;
    let path = PathBuf::from(super::resolve_path(path, cwd)?);
    if path.extension().and_then(|p| p.to_str()) != Some("ipynb") {
        bail!("Notebook path must end in .ipynb");
    }
    let operation = input["operation"]
        .as_str()
        .context("operation is required")?;
    let writable = !matches!(operation, "read" | "list_cells");
    let parent = path.parent().context("Notebook parent is missing")?;
    let lock = if writable {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("ipynb.claude-lock"))?;
        file.try_lock()
            .context("Notebook is being edited by another process")?;
        Some(file)
    } else {
        None
    };
    let mut notebook = if operation == "create" {
        if path.exists() {
            bail!("Notebook already exists");
        }
        json!({"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":[]})
    } else {
        let bytes = std::fs::read(&path)?;
        if bytes.len() > 32 * 1024 * 1024 {
            bail!("Notebook exceeds 32 MiB");
        }
        serde_json::from_slice::<Value>(&bytes).context("Invalid notebook JSON")?
    };
    if notebook["nbformat"] != 4 {
        bail!("Only Jupyter notebook format 4 is supported");
    }
    let cells = notebook["cells"]
        .as_array_mut()
        .context("Notebook cells must be an array")?;
    let index = || -> Result<usize> {
        let index = input["cell_index"]
            .as_u64()
            .context("cell_index is required")?;
        usize::try_from(index).context("cell_index is too large")
    };
    match operation {
        "create" | "read" => {}
        "list_cells" => return Ok(json!({"path":path,"cells":cells})),
        "add_cell" => {
            let kind = input["cell_type"].as_str().unwrap_or("code");
            if !matches!(kind, "code" | "markdown" | "raw") {
                bail!("Invalid cell_type");
            }
            let source = source(&input)?;
            let metadata = input.get("metadata").cloned().unwrap_or(json!({}));
            if !metadata.is_object() {
                bail!("metadata must be an object");
            }
            let mut cell = json!({"id":uuid::Uuid::new_v4().to_string(),"cell_type":kind,"source":source,"metadata":metadata});
            if kind == "code" {
                cell["outputs"] = json!([]);
                cell["execution_count"] = Value::Null;
            }
            let at = if input.get("cell_index").is_some() {
                index()?
            } else {
                cells.len()
            };
            if at > cells.len() {
                bail!("cell_index is out of range");
            }
            cells.insert(at, cell);
        }
        "edit_cell" => {
            let cell = cells
                .get_mut(index()?)
                .context("cell_index is out of range")?;
            if input.get("source").is_some() {
                cell["source"] = source(&input)?;
            }
            if let Some(metadata) = input.get("metadata") {
                if !metadata.is_object() {
                    bail!("metadata must be an object");
                }
                cell["metadata"] = metadata.clone();
            }
            // Results from the previous source are stale after editing code.
            if cell["cell_type"] == "code" && input.get("source").is_some() {
                cell["outputs"] = json!([]);
                cell["execution_count"] = Value::Null;
            }
        }
        "delete_cell" => {
            let at = index()?;
            if at >= cells.len() {
                bail!("cell_index is out of range");
            }
            cells.remove(at);
        }
        "move_cell" => {
            let from = index()?;
            let to = input
                .get("new_index")
                .or_else(|| input["metadata"].get("new_index"))
                .and_then(Value::as_u64)
                .context("new_index is required")? as usize;
            if from >= cells.len() || to >= cells.len() {
                bail!("Cell index is out of range");
            }
            let cell = cells.remove(from);
            cells.insert(to, cell);
        }
        "clear_outputs" => {
            let selected = if input.get("cell_index").is_some() {
                let at = index()?;
                if at >= cells.len() {
                    bail!("cell_index is out of range");
                }
                Some(at)
            } else {
                None
            };
            for (at, cell) in cells.iter_mut().enumerate() {
                if selected.is_none_or(|selected| selected == at) && cell["cell_type"] == "code" {
                    cell["outputs"] = json!([]);
                    cell["execution_count"] = Value::Null;
                }
            }
        }
        _ => bail!("Unknown notebook operation: {operation}"),
    }
    if writable {
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&serde_json::to_vec_pretty(&notebook)?)?;
        temporary.as_file().sync_all()?;
        if operation == "create" {
            temporary.persist_noclobber(&path)?;
        } else {
            temporary.persist(&path)?;
        }
    }
    drop(lock);
    Ok(json!({"path":path,"notebook":notebook}))
}

fn source(input: &Value) -> Result<Value> {
    let source = input["source"]
        .as_array()
        .context("source must be an array of strings")?;
    if source.iter().any(|line| !line.is_string()) {
        bail!("source must contain strings");
    }
    Ok(Value::Array(source.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_preserve_unknown_metadata_and_invalidate_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        let path = dir.path().join("test.ipynb");
        std::fs::write(&path, json!({"nbformat":4,"nbformat_minor":5,"metadata":{"custom":"preserve"},"vendor":"preserve","cells":[{"cell_type":"code","source":["old"],"metadata":{"tags":["keep"]},"outputs":[{"text":"stale"}],"execution_count":3}]}).to_string()).unwrap();
        execute(
            json!({"path":"test.ipynb","operation":"edit_cell","cell_index":0,"source":["new"]}),
            cwd,
        )
        .unwrap();
        let result = execute(json!({"path":"test.ipynb","operation":"read"}), cwd).unwrap();
        assert_eq!(result["notebook"]["vendor"], "preserve");
        assert_eq!(
            result["notebook"]["cells"][0]["metadata"]["tags"],
            json!(["keep"])
        );
        assert_eq!(result["notebook"]["cells"][0]["outputs"], json!([]));
        let before = std::fs::read(&path).unwrap();
        assert!(execute(
            json!({"path":"test.ipynb","operation":"delete_cell","cell_index":8}),
            cwd
        )
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(execute(json!({"path":"test.ipynb","operation":"create"}), cwd).is_err());
    }
    #[test]
    fn create_add_move_delete_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        execute(json!({"path":"test.ipynb","operation":"create"}), cwd).unwrap();
        for kind in ["code", "markdown"] {
            execute(json!({"path":"test.ipynb","operation":"add_cell","cell_type":kind,"source":[kind]}),cwd).unwrap();
        }
        execute(json!({"path":"test.ipynb","operation":"move_cell","cell_index":1,"metadata":{"new_index":0}}),cwd).unwrap();
        let result = execute(json!({"path":"test.ipynb","operation":"list_cells"}), cwd).unwrap();
        assert_eq!(result["cells"][0]["cell_type"], "markdown");
        execute(
            json!({"path":"test.ipynb","operation":"clear_outputs"}),
            cwd,
        )
        .unwrap();
        execute(
            json!({"path":"test.ipynb","operation":"delete_cell","cell_index":0}),
            cwd,
        )
        .unwrap();
        let result = execute(json!({"path":"test.ipynb","operation":"list_cells"}), cwd).unwrap();
        assert_eq!(result["cells"].as_array().unwrap().len(), 1);
    }
}
