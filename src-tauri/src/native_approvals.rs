//! Main-window, argument-bound, expiring one-use approvals. No OS actions here.
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct Grant {
    tool: String,
    args: Value,
    snapshot: String,
    expires: Instant,
    window: String,
    execution_id: String,
    attempt_id: String,
}
#[derive(Default)]
pub struct Approvals {
    pending: Mutex<HashMap<String, Grant>>,
    grants: Mutex<HashMap<String, Grant>>,
    executions: Mutex<HashMap<String, Instant>>,
}
impl Approvals {
    pub fn prepare(
        &self,
        window: &str,
        execution_id: &str,
        attempt_id: &str,
        tool: &str,
        args: &Value,
        snapshot: String,
    ) -> Result<String, &'static str> {
        if window != "main"
            || uuid::Uuid::parse_str(execution_id).is_err()
            || uuid::Uuid::parse_str(attempt_id).is_err()
        {
            return Err("BLOCKED_BY_POLICY");
        }
        let mut pending = self.pending.lock().map_err(|_| "EXECUTION_FAILED")?;
        pending.retain(|_, item| item.expires > Instant::now());
        if pending.len() >= 128 {
            return Err("EXECUTION_FAILED");
        }
        let id = uuid::Uuid::new_v4().to_string();
        pending.insert(
            id.clone(),
            Grant {
                window: window.into(),
                execution_id: execution_id.into(),
                attempt_id: attempt_id.into(),
                tool: tool.into(),
                args: args.clone(),
                snapshot,
                expires: Instant::now() + Duration::from_secs(120),
            },
        );
        Ok(id)
    }
    pub fn inspect(&self, id: &str, window: &str) -> Result<(String, Value), &'static str> {
        let pending = self.pending.lock().map_err(|_| "EXECUTION_FAILED")?;
        let item = pending.get(id).ok_or("NATIVE_APPROVAL_STALE")?;
        if item.window != window {
            return Err("NATIVE_APPROVAL_STALE");
        }
        if item.expires <= Instant::now() {
            return Err("NATIVE_APPROVAL_EXPIRED");
        }
        Ok((item.tool.clone(), item.args.clone()))
    }
    pub fn approve(&self, id: &str, window: &str, snapshot: &str) -> Result<String, &'static str> {
        let mut pending = self.pending.lock().map_err(|_| "EXECUTION_FAILED")?;
        let item = pending.get(id).ok_or("NATIVE_APPROVAL_STALE")?;
        if item.window != window || item.snapshot != snapshot {
            return Err("NATIVE_APPROVAL_STALE");
        }
        if item.expires <= Instant::now() {
            return Err("NATIVE_APPROVAL_EXPIRED");
        }
        let mut grants = self.grants.lock().map_err(|_| "EXECUTION_FAILED")?;
        grants.retain(|_, g| g.expires > Instant::now());
        if grants.len() >= 128 {
            return Err("EXECUTION_FAILED");
        }
        let item = pending.remove(id).unwrap();
        let grant_id = uuid::Uuid::new_v4().to_string();
        grants.insert(grant_id.clone(), item);
        Ok(grant_id)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn consume_bound(
        &self,
        id: &str,
        window: &str,
        execution_id: &str,
        attempt_id: &str,
        tool: &str,
        args: &Value,
        snapshot: &str,
    ) -> Result<(), &'static str> {
        let mut grants = self.grants.lock().map_err(|_| "EXECUTION_FAILED")?;
        let g = grants.get(id).ok_or("CONFIRMATION_REQUIRED")?;
        if g.window != window
            || g.execution_id != execution_id
            || g.attempt_id != attempt_id
            || g.expires <= Instant::now()
            || g.tool != tool
            || g.args != *args
            || g.snapshot != snapshot
        {
            return Err("NATIVE_APPROVAL_STALE");
        }
        grants.remove(id);
        Ok(())
    }
    #[cfg(test)]
    pub fn issue(
        &self,
        tool: &str,
        args: &Value,
        snapshot: String,
    ) -> Result<String, &'static str> {
        let mut grants = self.grants.lock().map_err(|_| "EXECUTION_FAILED")?;
        grants.retain(|_, g| g.expires > Instant::now());
        if grants.len() >= 128 {
            return Err("EXECUTION_FAILED");
        }
        let id = uuid::Uuid::new_v4().to_string();
        grants.insert(
            id.clone(),
            Grant {
                window: "main".into(),
                execution_id: String::new(),
                attempt_id: String::new(),
                tool: tool.into(),
                args: args.clone(),
                snapshot,
                expires: Instant::now() + Duration::from_secs(120),
            },
        );
        Ok(id)
    }
    #[cfg(test)]
    pub fn consume(
        &self,
        id: &str,
        tool: &str,
        args: &Value,
        snapshot: &str,
    ) -> Result<(), &'static str> {
        let grant = self
            .grants
            .lock()
            .map_err(|_| "EXECUTION_FAILED")?
            .remove(id)
            .ok_or("CONFIRMATION_REQUIRED")?;
        if grant.expires <= Instant::now()
            || grant.tool != tool
            || grant.args != *args
            || grant.snapshot != snapshot
        {
            return Err("NATIVE_APPROVAL_STALE");
        }
        Ok(())
    }
    pub fn start(&self, id: &str) -> Result<(), &'static str> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err("INVALID_ARGUMENT");
        }
        let mut executions = self.executions.lock().map_err(|_| "EXECUTION_FAILED")?;
        if executions.contains_key(id) {
            return Err("DUPLICATE_EXECUTION");
        }
        if executions.len() >= 65536 {
            return Err("EXECUTION_FAILED");
        }
        executions.insert(id.into(), Instant::now() + Duration::from_secs(900));
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_tasks_bind_window_attempt_and_target_until_consumed() {
        let s = Approvals::default();
        let execution = uuid::Uuid::new_v4().to_string();
        let attempt = uuid::Uuid::new_v4().to_string();
        let args = serde_json::json!({"resourceId":"original"});
        let id = s
            .prepare(
                "main",
                &execution,
                &attempt,
                "file.delete",
                &args,
                "before".into(),
            )
            .unwrap();
        assert!(s.approve(&id, "voice", "before").is_err());
        assert!(s.approve(&id, "main", "changed").is_err());
        let grant = s.approve(&id, "main", "before").unwrap();
        assert!(s.approve(&id, "main", "before").is_err());
        assert!(s
            .consume_bound(
                &grant,
                "voice",
                &execution,
                &attempt,
                "file.delete",
                &args,
                "before"
            )
            .is_err());
        assert!(s
            .consume_bound(
                &grant,
                "main",
                &execution,
                &attempt,
                "file.delete",
                &args,
                "changed"
            )
            .is_err());
        assert!(s
            .consume_bound(
                &grant,
                "main",
                &execution,
                &uuid::Uuid::new_v4().to_string(),
                "file.delete",
                &args,
                "before"
            )
            .is_err());
        assert!(s
            .consume_bound(
                &grant,
                "main",
                &execution,
                &attempt,
                "file.delete",
                &args,
                "before"
            )
            .is_ok());
        assert!(s
            .consume_bound(
                &grant,
                "main",
                &execution,
                &attempt,
                "file.delete",
                &args,
                "before"
            )
            .is_err());
        let id = s
            .prepare(
                "main",
                &execution,
                &attempt,
                "file.delete",
                &args,
                "before".into(),
            )
            .unwrap();
        s.pending.lock().unwrap().get_mut(&id).unwrap().expires =
            Instant::now() - Duration::from_secs(1);
        assert_eq!(s.inspect(&id, "main"), Err("NATIVE_APPROVAL_EXPIRED"));
        assert_eq!(
            s.approve(&id, "main", "before"),
            Err("NATIVE_APPROVAL_EXPIRED")
        );
    }
    #[test]
    fn grants_bind_tool_args_snapshot_and_cannot_replay() {
        let s = Approvals::default();
        let args = serde_json::json!({"resourceId":"one"});
        for (tool, a, snapshot) in [
            ("file.delete", args.clone(), "changed"),
            ("file.copy", args.clone(), "original"),
            (
                "file.delete",
                serde_json::json!({"resourceId":"two"}),
                "original",
            ),
        ] {
            let id = s.issue("file.delete", &args, "original".into()).unwrap();
            assert!(s.consume(&id, tool, &a, snapshot).is_err());
            assert!(s.consume(&id, "file.delete", &args, "original").is_err());
        }
        let id = s.issue("file.delete", &args, "original".into()).unwrap();
        assert!(s.consume(&id, "file.delete", &args, "original").is_ok());
        assert!(s.consume(&id, "file.delete", &args, "original").is_err());
    }
    #[test]
    fn expired_grant_and_duplicate_execution_are_rejected() {
        let s = Approvals::default();
        let args = serde_json::json!({});
        let id = s.issue("system.lock", &args, "".into()).unwrap();
        s.grants.lock().unwrap().get_mut(&id).unwrap().expires =
            Instant::now() - Duration::from_secs(1);
        assert!(s.consume(&id, "system.lock", &args, "").is_err());
        let id = uuid::Uuid::new_v4().to_string();
        assert!(s.start(&id).is_ok());
        assert_eq!(s.start(&id), Err("DUPLICATE_EXECUTION"));
    }
}
