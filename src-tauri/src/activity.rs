use std::sync::{Mutex, OnceLock};
#[derive(Default)]
struct State {
    active: usize,
    installing: bool,
}
impl State {
    fn begin(&mut self) -> Result<(), &'static str> {
        if self.installing {
            return Err("UPDATE_INSTALLING");
        }
        self.active += 1;
        Ok(())
    }
    fn try_install(&mut self) -> bool {
        if self.active != 0 || self.installing {
            return false;
        }
        self.installing = true;
        true
    }
}
fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Default::default)
}
pub struct Activity;
pub fn begin() -> Result<Activity, &'static str> {
    let mut s = state().lock().map_err(|_| "UPDATE_BUSY")?;
    s.begin()?;
    Ok(Activity)
}
impl Drop for Activity {
    fn drop(&mut self) {
        if let Ok(mut s) = state().lock() {
            s.active = s.active.saturating_sub(1);
        }
    }
}
pub fn try_install() -> bool {
    let Ok(mut s) = state().lock() else {
        return false;
    };
    s.try_install()
}
pub fn finish_install() {
    if let Ok(mut s) = state().lock() {
        s.installing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_and_file_work_defer_install_and_new_work_cannot_start_during_install() {
        let mut s = State::default();
        s.begin().unwrap();
        s.begin().unwrap();
        assert!(!s.try_install());
        s.active -= 1;
        assert!(!s.try_install());
        s.active -= 1;
        assert!(s.try_install());
        assert_eq!(s.begin(), Err("UPDATE_INSTALLING"));
        assert!(!s.try_install());
        s.installing = false;
        assert!(s.begin().is_ok());
    }
}
