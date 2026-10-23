//! Reads only accessibility roles and call-control labels in supported desktop apps.
use super::controls::{active_control, idle_control, microphone_control};
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
    unsafe fn attribute(element: Ref, name: &[u8]) -> Option<Owned> {
        let key = Owned(CFStringCreateWithCString(
            std::ptr::null(),
            name.as_ptr().cast(),
            0x08000100,
        ));
        let mut value = std::ptr::null();
        if AXUIElementCopyAttributeValue(element, key.0, &mut value) == 0 && !value.is_null() {
            Some(Owned(value))
        } else {
            None
        }
    }
    unsafe fn text(element: Ref, name: &[u8]) -> Option<String> {
        let value = attribute(element, name)?;
        if CFGetTypeID(value.0) != CFStringGetTypeID() {
            return None;
        }
        let mut buffer = [0i8; 256];
        if CFStringGetCString(value.0, buffer.as_mut_ptr(), 256, 0x08000100) {
            Some(
                CStr::from_ptr(buffer.as_ptr())
                    .to_string_lossy()
                    .to_lowercase(),
            )
        } else {
            None
        }
    }
    unsafe fn scan(pid: i32) -> Observation {
        let root = Owned(AXUIElementCreateApplication(pid));
        AXUIElementSetMessagingTimeout(root.0, 0.05);
        let started = Instant::now();
        let mut budget = Budget::default();
        let mut stack = vec![(root, 0usize)];
        let mut idle = false;
        let mut leave = false;
        let mut microphone = false;
        let mut incomplete = false;
        while let Some((node, depth)) = stack.pop() {
            if !budget.visit(depth, started.elapsed()) {
                return Observation::Unknown;
            }
            let Some(role) = text(node.0, b"AXRole\0") else {
                incomplete = true;
                continue;
            };
            if role == "axbutton" || role == "axmenubutton" {
                // Do not read text bodies, window titles, participants, or URLs.
                for key in [
                    b"AXTitle\0".as_slice(),
                    b"AXDescription\0".as_slice(),
                    b"AXIdentifier\0".as_slice(),
                ] {
                    if let Some(label) = text(node.0, key) {
                        leave |= active_control(&label);
                        microphone |= microphone_control(&label);
                        if leave && microphone {
                            return Observation::Active;
                        }
                        idle |= idle_control(&label);
                    }
                }
            }
            if let Some(children) = attribute(node.0, b"AXChildren\0") {
                if CFGetTypeID(children.0) != CFArrayGetTypeID() {
                    incomplete = true;
                    continue;
                }
                let count = CFArrayGetCount(children.0);
                if count > 0 && depth >= MAX_DEPTH {
                    incomplete = true;
                    continue;
                }
                if count > MAX_ELEMENTS as isize || stack.len() + count as usize > MAX_ELEMENTS {
                    return Observation::Unknown;
                }
                for i in 0..count {
                    stack.push((
                        Owned(CFRetain(CFArrayGetValueAtIndex(children.0, i))),
                        depth + 1,
                    ));
                }
            }
        }
        if idle && !incomplete {
            Observation::Inactive
        } else {
            Observation::Unknown
        }
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
                let observation = scan(pid);
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
