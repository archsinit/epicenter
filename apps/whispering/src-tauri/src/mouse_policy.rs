//! Pure mouse-button policy, independently testable without macOS or Tauri.
use std::collections::BTreeMap;

pub const COMMANDS: &[&str] = &[
    "pressEnter",
    "pushToTalk",
    "toggleManualRecording",
    "startManualRecording",
    "stopManualRecording",
    "cancelManualRecording",
    "startVadRecording",
    "stopVadRecording",
    "toggleVadRecording",
    "openTransformationPicker",
    "runTransformationOnClipboard",
];

pub fn validate(bindings: Vec<(u32, String)>) -> Result<BTreeMap<u32, String>, String> {
    let mut validated = BTreeMap::new();
    for (button, command) in bindings {
        // Reserve primary/secondary clicks so users cannot accidentally disable normal navigation.
        if !(2..=31).contains(&button) {
            return Err("Choose a middle or extra mouse button (2–31)".into());
        }
        if !COMMANDS.contains(&command.as_str()) {
            return Err("Unknown mouse shortcut action".into());
        }
        if validated.insert(button, command).is_some() {
            return Err("Each mouse button can only have one action".into());
        }
    }
    Ok(validated)
}

#[derive(Clone, Copy)]
pub enum ButtonEvent {
    Down,
    Up,
    Drag,
}

#[derive(Default)]
pub struct MousePolicy {
    pub bindings: BTreeMap<u32, String>,
    held: BTreeMap<u32, String>,
}

impl MousePolicy {
    /// Return suppression and optional (action, pressed) without executing anything in the tap.
    pub fn handle(&mut self, button: u32, event: ButtonEvent) -> (bool, Option<(String, bool)>) {
        match event {
            ButtonEvent::Down => {
                if self.held.contains_key(&button) {
                    return (true, None);
                }
                if let Some(command) = self.bindings.get(&button).cloned() {
                    self.held.insert(button, command.clone());
                    (true, Some((command, true)))
                } else {
                    (false, None)
                }
            }
            ButtonEvent::Up => match self.held.remove(&button) {
                Some(command) => (true, Some((command, false))),
                None => (false, None),
            },
            ButtonEvent::Drag => (self.held.contains_key(&button), None),
        }
    }

    /// A disabled event tap must release push-to-talk rather than leaving recording stuck on.
    pub fn release_all(&mut self) -> Vec<(String, bool)> {
        std::mem::take(&mut self.held)
            .into_values()
            .map(|command| (command, false))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> MousePolicy {
        MousePolicy {
            bindings: validate(vec![(3, "pushToTalk".into())]).unwrap(),
            ..Default::default()
        }
    }
    #[test]
    fn defaults_do_not_intercept_any_buttons() {
        let mut p = MousePolicy::default();
        for button in 0..32 {
            assert_eq!(p.handle(button, ButtonEvent::Down), (false, None));
        }
    }
    #[test]
    fn assigned_click_drag_and_release_are_suppressed() {
        let mut p = policy();
        assert_eq!(
            p.handle(3, ButtonEvent::Down),
            (true, Some(("pushToTalk".into(), true)))
        );
        assert_eq!(p.handle(3, ButtonEvent::Drag), (true, None));
        assert_eq!(
            p.handle(3, ButtonEvent::Up),
            (true, Some(("pushToTalk".into(), false)))
        );
    }
    #[test]
    fn other_buttons_keep_their_normal_behaviour() {
        let mut p = policy();
        for event in [ButtonEvent::Down, ButtonEvent::Drag, ButtonEvent::Up] {
            assert_eq!(p.handle(4, event), (false, None));
        }
    }
    #[test]
    fn changing_bindings_mid_click_still_releases_original_action() {
        let mut p = policy();
        p.handle(3, ButtonEvent::Down);
        p.bindings = validate(vec![(3, "pressEnter".into())]).unwrap();
        assert_eq!(
            p.handle(3, ButtonEvent::Up),
            (true, Some(("pushToTalk".into(), false)))
        );
    }
    #[test]
    fn clearing_bindings_mid_click_still_suppresses_release() {
        let mut p = policy();
        p.handle(3, ButtonEvent::Down);
        p.bindings.clear();
        assert!(p.handle(3, ButtonEvent::Up).0);
        assert!(!p.handle(3, ButtonEvent::Down).0);
    }
    #[test]
    fn repeated_down_is_not_repeated_action() {
        let mut p = policy();
        p.handle(3, ButtonEvent::Down);
        assert_eq!(p.handle(3, ButtonEvent::Down), (true, None));
    }
    #[test]
    fn tap_failure_releases_held_actions() {
        let mut p = policy();
        p.handle(3, ButtonEvent::Down);
        assert_eq!(p.release_all(), vec![("pushToTalk".into(), false)]);
        assert!(p.release_all().is_empty());
    }
    #[test]
    fn invalid_and_duplicate_bindings_are_rejected() {
        assert!(validate(vec![(0, "pressEnter".into())]).is_err());
        assert!(validate(vec![(32, "pressEnter".into())]).is_err());
        assert!(validate(vec![(3, "unknown".into())]).is_err());
        assert!(validate(vec![(3, "pressEnter".into()), (3, "pushToTalk".into())]).is_err());
    }
}
