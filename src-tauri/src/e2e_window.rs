//! Test-profile window guard (Windows only, E2E CDP port set). The E2E suite runs on Shay's PC
//! while he games, so a test window must never become the foreground window (#2513, #4712).
//!
//! `focused(false)` alone wasn't enough: tao clears its "don't focus" marker right after creating
//! the window, so any later style refresh shows it again with `SW_SHOW`, which activates it. Windows
//! normally refuses that while another app is in front, but not once the foreground lock timeout has
//! passed with no keyboard or mouse input (a game played with a controller), and then the test window
//! jumps in front of the game for a second.
//!
//! Three layers, all test-only:
//! - the window is built `focusable(false)` (`WS_EX_NOACTIVATE`, see `create_main_window`);
//! - this subclass adds `SWP_NOACTIVATE` to every position change, so `ShowWindow`/`SetWindowPos`
//!   can't activate it, and answers mouse activation with `MA_NOACTIVATE`;
//! - if the window is activated anyway, it hands the foreground straight back to the window it took
//!   it from (allowed, since it is now the foreground process).

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IsWindowVisible, SetForegroundWindow, MA_NOACTIVATE, SWP_NOACTIVATE, WA_INACTIVE, WINDOWPOS,
    WM_ACTIVATE, WM_MOUSEACTIVATE, WM_WINDOWPOSCHANGING,
};

const SUBCLASS_ID: usize = 0x4712;

/// Installs the guard on `hwnd`. Must run on the thread that owns the window (the main thread,
/// in `setup`). Returns false if Windows refused the subclass.
pub fn install(hwnd: HWND) -> bool {
    // SAFETY: `hwnd` is a live window owned by this thread; the callback is a plain function.
    unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) != 0 }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_WINDOWPOSCHANGING if lparam != 0 => {
            // SAFETY: for WM_WINDOWPOSCHANGING, lParam points at a WINDOWPOS the caller owns.
            let pos = unsafe { &mut *(lparam as *mut WINDOWPOS) };
            pos.flags |= SWP_NOACTIVATE;
        }
        WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
        WM_ACTIVATE if (wparam & 0xFFFF) as u32 != WA_INACTIVE => {
            let previous = lparam as HWND;
            // SAFETY: plain Win32 calls on a window handle Windows just gave us.
            unsafe {
                if !previous.is_null() && previous != hwnd && IsWindowVisible(previous) != 0 {
                    SetForegroundWindow(previous);
                }
            }
        }
        _ => {}
    }
    // SAFETY: forwards to the next window procedure in the subclass chain.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_activation_is_refused() {
        // A null window still reaches our match arm before DefSubclassProc would be needed.
        let result =
            unsafe { subclass_proc(std::ptr::null_mut(), WM_MOUSEACTIVATE, 0, 0, SUBCLASS_ID, 0) };
        assert_eq!(result, MA_NOACTIVATE as LRESULT);
    }

    #[test]
    fn position_changes_never_activate() {
        // SAFETY: a zeroed WINDOWPOS is a valid value; nothing else holds it.
        let mut pos: WINDOWPOS = unsafe { std::mem::zeroed() };
        unsafe {
            subclass_proc(
                std::ptr::null_mut(),
                WM_WINDOWPOSCHANGING,
                0,
                &mut pos as *mut WINDOWPOS as LPARAM,
                SUBCLASS_ID,
                0,
            );
        }
        assert_ne!(pos.flags & SWP_NOACTIVATE, 0);
    }
}
