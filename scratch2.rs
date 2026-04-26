use std::process::Command;
use serde_json::Value;

fn get_value_by_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for part in path.split('.') {
        if let Ok(idx) = part.parse::<usize>() {
            current = current.get(idx)?;
        } else {
            current = current.get(part)?;
        }
    }
    Some(current)
}

fn process_output(raw: &str, output_filter: Option<&str>) -> String {
    let trimmed = raw.trim();
    
    // Find the outermost JSON block (first { to last })
    let json_str = if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start <= end {
            Some(&trimmed[start..=end])
        } else {
            None
        }
    } else {
        Some(trimmed)
    };

    if let Some(jstr) = json_str {
        if let Ok(val) = serde_json::from_str::<Value>(jstr) {
            if let Some(filter) = output_filter {
                if let Some(extracted) = get_value_by_path(&val, filter) {
                    return match extracted {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                } else {
                    return format!("FILTER FAILED. JSON keys: {:?}", val.as_object().map(|o| o.keys().collect::<Vec<_>>()));
                }
            }
        } else {
            return format!("JSON PARSE FAILED. err: {}", serde_json::from_str::<Value>(jstr).unwrap_err());
        }
    }
    
    trimmed.to_string()
}

fn main() {
    let mut cmd = Command::new("wsl.exe");
    cmd.env_remove("WSLENV");
    cmd.args(["-d", "Ubuntu"]);
    cmd.args(["-e", "bash", "-lc", "exec \"$@\"", "bash"]);
    cmd.arg("/home/zhiqing/.npm-global/bin/openclaw");
    cmd.args(["agent", "--agent", "main", "--local", "--json", "-m"]);
    cmd.arg("Say hello in one short sentence.");
    cmd.stdin(std::process::Stdio::null());
    
    let output = cmd.output().expect("Failed to execute command");
    let raw_stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let raw_stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    
    let combined = format!("{}\n{}", raw_stdout, raw_stderr);
    
    println!("Combined output length: {}", combined.len());
    let processed = process_output(&combined, Some("finalAssistantVisibleText"));
    println!("Processed: {}", processed);
}
