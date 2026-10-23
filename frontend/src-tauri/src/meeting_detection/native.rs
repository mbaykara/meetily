//! Reads only accessibility roles and call-control labels in supported desktop apps.
use super::controls::{windows_observation, AppKind, WindowEvidence};
use super::policy::Observation;

#[cfg(not(target_os = "macos"))]
pub fn observe() -> (bool, [Observation; 2]) {
    (false, [Observation::Unknown; 2])
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::meeting_detection::budget::{Budget, MAX_DEPTH, MAX_ELEMENTS};
    use objc::{class, msg_send, sel, sel_impl};
    use std::{
        ffi::{c_void, CStr},
        time::Instant,
    };
    type Ref = *const c_void;
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXUIElementCreateApplication(pid: i32) -> Ref;
        fn AXUIElementCopyAttributeValue(element: Ref, attribute: Ref, value: *mut Ref) -> i32;
        fn AXUIElementSetMessagingTimeout(element: Ref, timeout: f32) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(allocator: Ref, value: *const i8, encoding: u32) -> Ref;
        fn CFStringGetCString(value: Ref, buffer: *mut i8, size: isize, encoding: u32) -> bool;
        fn CFGetTypeID(value: Ref) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFArrayGetTypeID() -> usize;
        fn CFArrayGetCount(value: Ref) -> isize;
        fn CFArrayGetValueAtIndex(value: Ref, index: isize) -> Ref;
        fn CFRetain(value: Ref) -> Ref;
        fn CFRelease(value: Ref);
    }
    struct Owned(Ref);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0) }
        }
    }
    // AXError codes: optional unsupported/no-value attributes are normal leaves;
    // IPC failure, invalid elements and permission errors must remain failures.
    unsafe fn attribute(element: Ref, name: &[u8]) -> Result<Option<Owned>, i32> {
        let key = Owned(CFStringCreateWithCString(
            std::ptr::null(),
            name.as_ptr().cast(),
            0x08000100,
        ));
        let mut value = std::ptr::null();
        let code = AXUIElementCopyAttributeValue(element, key.0, &mut value);
        if code == 0 && !value.is_null() {
            Ok(Some(Owned(value)))
        } else if code == -25212 {
            Ok(None)
        } else {
            Err(code)
        }
    }
    unsafe fn text(element: Ref, name: &[u8]) -> Result<Option<String>, i32> {
        let optional = match attribute(element, name) {
            Err(-25205) => None,
            other => other?,
        };
        let Some(value) = optional else {
            return Ok(None);
        };
        if CFGetTypeID(value.0) != CFStringGetTypeID() {
            return Err(-25200);
        }
        let mut buffer = [0i8; 256];
        if CFStringGetCString(value.0, buffer.as_mut_ptr(), 256, 0x08000100) {
            Ok(Some(
                CStr::from_ptr(buffer.as_ptr())
                    .to_string_lossy()
                    .to_lowercase(),
            ))
        } else {
            Err(-25200)
        }
    }
    unsafe fn scan_window(window: Ref, app: AppKind, app_started: Instant) -> WindowEvidence {
        let mut evidence = WindowEvidence::default();
        let started = Instant::now();
        let mut budget = Budget::default();
        let mut stack = vec![(Owned(CFRetain(window)), 0usize)];
        while let Some((node, depth)) = stack.pop() {
            if AXUIElementSetMessagingTimeout(node.0, 0.05) != 0 {
                evidence.incomplete = true;
                continue;
            }
            if !budget.visit(depth, started.elapsed()) || app_started.elapsed().as_millis() >= 2500
            {
                evidence.incomplete = true;
                break;
            }
            let role = match text(node.0, b"AXRole\0") {
                Ok(Some(role)) => role,
                _ => {
                    evidence.incomplete = true;
                    continue;
                }
            };
            // Only controls/toolbar metadata, never text bodies or window titles.
            if matches!(
                role.as_str(),
                "axbutton"
                    | "axmenubutton"
                    | "axpopupbutton"
                    | "axtoolbar"
                    | "axtab"
                    | "axradiobutton"
            ) {
                for key in [
                    b"AXTitle\0".as_slice(),
                    b"AXDescription\0".as_slice(),
                    b"AXIdentifier\0".as_slice(),
                ] {
                    match text(node.0, key) {
                        Ok(Some(label)) => evidence.control(app, &role, &label),
                        Ok(None) => {}
                        Err(_) => evidence.incomplete = true,
                    }
                }
                if evidence.observation() == Observation::Active {
                    return evidence;
                }
            }
            match attribute(node.0, b"AXChildren\0") {
                Ok(Some(children)) => {
                    if CFGetTypeID(children.0) != CFArrayGetTypeID() {
                        evidence.incomplete = true;
                        continue;
                    }
                    let count = CFArrayGetCount(children.0);
                    if (count > 0 && depth >= MAX_DEPTH)
                        || count > MAX_ELEMENTS as isize
                        || stack.len() + count as usize > MAX_ELEMENTS
                    {
                        evidence.incomplete = true;
                        continue;
                    }
                    for i in (0..count).rev() {
                        stack.push((
                            Owned(CFRetain(CFArrayGetValueAtIndex(children.0, i))),
                            depth + 1,
                        ));
                    }
                }
                Err(-25205) => {
                    if matches!(
                        role.as_str(),
                        "axwindow"
                            | "axgroup"
                            | "axscrollarea"
                            | "axtoolbar"
                            | "axsplitgroup"
                            | "axtabgroup"
                    ) {
                        evidence.incomplete = true;
                    }
                }
                Ok(None) => {}
                Err(_) => evidence.incomplete = true,
            }
        }
        evidence
    }
    unsafe fn scan(pid: i32, app: AppKind) -> Observation {
        let root = Owned(AXUIElementCreateApplication(pid));
        if AXUIElementSetMessagingTimeout(root.0, 0.05) != 0 {
            return Observation::Unknown;
        }
        let started = Instant::now();
        let windows = match attribute(root.0, b"AXWindows\0") {
            Ok(Some(windows)) => windows,
            _ => return Observation::Unknown,
        };
        if CFGetTypeID(windows.0) != CFArrayGetTypeID() {
            return Observation::Unknown;
        }
        let count = CFArrayGetCount(windows.0);
        if count > 16 {
            return Observation::Unknown;
        }
        let mut evidence = Vec::new();
        let mut complete = true;
        for i in 0..count {
            if started.elapsed().as_millis() >= 2500 {
                complete = false;
                break;
            }
            let window = scan_window(CFArrayGetValueAtIndex(windows.0, i), app, started);
            if window.observation() == Observation::Active {
                return Observation::Active;
            }
            evidence.push(window);
        }
        windows_observation(&evidence, complete)
    }
    pub fn observe() -> (bool, [Observation; 2]) {
        unsafe {
            if !AXIsProcessTrusted() {
                return (false, [Observation::Unknown; 2]);
            }
            let pool: *mut objc::runtime::Object = msg_send![class!(NSAutoreleasePool), new];
            let workspace: *mut objc::runtime::Object =
                msg_send![class!(NSWorkspace), sharedWorkspace];
            let apps: *mut objc::runtime::Object = msg_send![workspace, runningApplications];
            let count: usize = msg_send![apps, count];
            let mut result = [Observation::Inactive; 2];
            for i in 0..count {
                let app: *mut objc::runtime::Object = msg_send![apps, objectAtIndex:i];
                let bundle: *mut objc::runtime::Object = msg_send![app, bundleIdentifier];
                if bundle.is_null() {
                    continue;
                }
                let id: *const i8 = msg_send![bundle, UTF8String];
                if id.is_null() {
                    continue;
                }
                let slot = match CStr::from_ptr(id).to_bytes() {
                    b"com.microsoft.teams2" | b"com.microsoft.teams" => 0,
                    b"us.zoom.xos" => 1,
                    _ => continue,
                };
                let pid: i32 = msg_send![app, processIdentifier];
                let observation = scan(
                    pid,
                    if slot == 0 {
                        AppKind::Teams
                    } else {
                        AppKind::Zoom
                    },
                );
                result[slot] = match (result[slot], observation) {
                    (Observation::Active, _) | (_, Observation::Active) => Observation::Active,
                    (Observation::Unknown, _) | (_, Observation::Unknown) => Observation::Unknown,
                    _ => Observation::Inactive,
                };
            }
            let _: () = msg_send![pool, drain];
            (true, result)
        }
    }
}
#[cfg(target_os = "macos")]
pub use macos::observe;
