use serde::Serialize;
use serde_json::Value;
use std::process::Command;

#[derive(Debug, PartialEq)]
enum Risk {
    Safe,
    Confirm,
    Blocked,
}
fn policy(command: &str) -> Risk {
    match command {
        "system.status" => Risk::Safe,
        "app.open" | "app.close" => Risk::Confirm,
        _ => Risk::Blocked,
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionResult {
    success: bool,
    message: Option<String>,
    error_code: Option<String>,
}
fn failed(code: &str) -> ExecutionResult {
    ExecutionResult {
        success: false,
        message: None,
        error_code: Some(code.into()),
    }
}
fn success(message: String) -> ExecutionResult {
    ExecutionResult {
        success: true,
        message: Some(message),
        error_code: None,
    }
}

fn validate(
    command: &str,
    arguments: &Value,
    confirmed: bool,
) -> Result<Option<&'static str>, &'static str> {
    let risk = policy(command);
    if risk == Risk::Blocked {
        return Err("BLOCKED");
    }
    if risk == Risk::Confirm && !confirmed {
        return Err("CONFIRMATION_REQUIRED");
    }
    let object = arguments.as_object().ok_or("INVALID_ARGUMENTS")?;
    if command == "system.status" {
        return if object.is_empty() {
            Ok(None)
        } else {
            Err("INVALID_ARGUMENTS")
        };
    }
    if object.len() != 1 {
        return Err("INVALID_ARGUMENTS");
    }
    match object.get("appName").and_then(Value::as_str) {
        Some("Notepad") => Ok(Some("Notepad")),
        Some("Calculator") => Ok(Some("Calculator")),
        _ => Err("BLOCKED"),
    }
}

#[tauri::command]
pub async fn execute_local_command(
    window: tauri::WebviewWindow,
    command_type: String,
    arguments: Value,
    confirmed: bool,
) -> ExecutionResult {
    if window.label() != "main" {
        return failed("BLOCKED");
    }
    let app = match validate(&command_type, &arguments, confirmed) {
        Ok(app) => app,
        Err(code) => return failed(code),
    };
    if command_type == "system.status" {
        return success(format!(
            "시스템: {} / {} · CPU 스레드: {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let Some(app) = app else {
            return failed("INVALID_ARGUMENTS");
        };
        let executable = if app == "Notepad" {
            "notepad.exe"
        } else {
            "calc.exe"
        };
        if command_type == "app.open" {
            let result = Command::new(executable).spawn();
            return if result.is_ok() {
                success(format!("{app} 실행 요청 완료"))
            } else {
                failed("EXECUTION_FAILED")
            };
        }
        // No arbitrary process names, shell strings or force termination are accepted.
        let image = if app == "Notepad" {
            "notepad.exe"
        } else {
            "CalculatorApp.exe"
        };
        let result = Command::new("taskkill.exe")
            .args(["/IM", image])
            .creation_flags(0x08000000)
            .output();
        return match result {
            Ok(output) if output.status.success() => success(format!("{app} 종료 요청 완료")),
            _ => failed("EXECUTION_FAILED"),
        };
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        failed("BLOCKED")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn blocked_commands_cannot_be_approved() {
        for command in ["file.delete", "shell.exec", "registry.write", "unknown"] {
            assert_eq!(validate(command, &json!({}), true), Err("BLOCKED"));
        }
    }
    #[test]
    fn confirmation_and_arguments_are_enforced_in_rust() {
        assert_eq!(
            validate("app.close", &json!({"appName":"Notepad"}), false),
            Err("CONFIRMATION_REQUIRED")
        );
        assert_eq!(
            validate("app.open", &json!({"appName":"cmd.exe"}), true),
            Err("BLOCKED")
        );
        assert_eq!(
            validate(
                "app.open",
                &json!({"appName":"Notepad", "args":"/c anything"}),
                true
            ),
            Err("INVALID_ARGUMENTS")
        );
        assert_eq!(
            validate("app.open", &json!({"appName":"Notepad"}), true),
            Ok(Some("Notepad"))
        );
        assert_eq!(validate("system.status", &json!({}), false), Ok(None));
    }
}
