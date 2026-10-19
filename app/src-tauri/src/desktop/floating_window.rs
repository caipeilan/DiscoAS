//! Frameless floating windows: framework visibility and native drawing share one lifecycle.
use std::cell::RefCell;
use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tokio::sync::oneshot;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Dwm::{DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY},
    System::Threading::GetCurrentThreadId,
    UI::{
        Shell::{DefSubclassProc, GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            CallNextHookEx, GetWindowLongPtrW, GetWindowRect, IsWindowVisible, SetWindowLongPtrW,
            SetWindowPos, SetWindowsHookExW, UnhookWindowsHookEx, GWL_EXSTYLE, GWL_STYLE,
            HCBT_ACTIVATE, HHOOK, HWND_TOPMOST, MA_NOACTIVATE, STYLESTRUCT, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WH_CBT, WM_DWMCOMPOSITIONCHANGED,
            WM_MOUSEACTIVATE, WM_NCACTIVATE, WM_NCCALCSIZE, WM_NCDESTROY, WM_NCPAINT,
            WM_STYLECHANGING, WS_CAPTION, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_NOACTIVATE,
            WS_EX_STATICEDGE, WS_EX_WINDOWEDGE, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP,
            WS_SYSMENU, WS_THICKFRAME,
        },
    },
};

const SUBCLASS_ID: usize = 0xDA50;
const FRAME: u32 = WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MAXIMIZEBOX | WS_MINIMIZEBOX;
const EDGE: u32 = WS_EX_CLIENTEDGE | WS_EX_DLGMODALFRAME | WS_EX_STATICEDGE | WS_EX_WINDOWEDGE;
type Completion = oneshot::Receiver<Result<(), String>>;

struct ActivationHook(HHOOK);

impl Drop for ActivationHook {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.0) };
    }
}

thread_local! {
    static ACTIVATION_HOOK: RefCell<Option<ActivationHook>> = const { RefCell::new(None) };
}

unsafe extern "system" fn activation_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HCBT_ACTIVATE as i32 {
        let mut passive = 0;
        if GetWindowSubclass(
            wparam as HWND,
            Some(floating_proc),
            SUBCLASS_ID,
            &mut passive,
        ) != 0
            && passive != 0
        {
            // Tao's visibility setter calls SW_SHOW. Reject activation before it
            // happens, while still letting Tao update visibility and draw the window.
            return 1;
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

fn install_activation_hook() -> Result<(), String> {
    ACTIVATION_HOOK.with(|slot| {
        let mut hook = slot.borrow_mut();
        if hook.is_none() {
            let handle = unsafe {
                SetWindowsHookExW(
                    WH_CBT,
                    Some(activation_proc),
                    std::ptr::null_mut(),
                    GetCurrentThreadId(),
                )
            };
            if handle.is_null() {
                return Err("错误：无法设置浮窗被动交互".into());
            }
            *hook = Some(ActivationHook(handle));
        }
        Ok(())
    })
}

fn dispatch(
    window: &WebviewWindow,
    action: impl FnOnce(&WebviewWindow) -> Result<(), String> + Send + 'static,
) -> Result<Completion, String> {
    let w = window.clone();
    let (sender, receiver) = oneshot::channel();
    window
        .run_on_main_thread(move || {
            // Synchronous callers deliberately do not await the UI queue. Keep native
            // initialization failures visible when their completion receiver is dropped.
            let result = action(&w);
            if let Err(error) = &result {
                eprintln!("{error}");
            }
            let _ = sender.send(result);
        })
        .map_err(|_| "错误：浮窗窗口不可用")?;
    Ok(receiver)
}

fn extended_style(style: u32, passive: bool) -> u32 {
    let style = style & !EDGE;
    if passive {
        style | WS_EX_NOACTIVATE
    } else {
        style & !WS_EX_NOACTIVATE
    }
}

unsafe fn disable_native_frame(hwnd: HWND) -> Result<(), String> {
    let policy = DWMNCRP_DISABLED;
    let result = DwmSetWindowAttribute(
        hwnd,
        DWMWA_NCRENDERING_POLICY as u32,
        (&policy as *const i32).cast(),
        std::mem::size_of_val(&policy) as u32,
    );
    if result < 0 {
        Err(format!(
            "错误：无法隐藏浮窗系统边框（0x{:08X}）",
            result as u32
        ))
    } else {
        Ok(())
    }
}

unsafe extern "system" fn floating_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    reference: usize,
) -> LRESULT {
    let passive = reference != 0;
    match message {
        WM_STYLECHANGING => {
            // Tao reconstructs its complete style on visibility/cursor changes.
            // Filter that write instead of removing the caption again after painting.
            let styles = &mut *(lparam as *mut STYLESTRUCT);
            match wparam as i32 {
                GWL_STYLE => styles.styleNew = (styles.styleNew & !FRAME) | WS_POPUP,
                GWL_EXSTYLE => styles.styleNew = extended_style(styles.styleNew, passive),
                _ => {}
            }
        }
        WM_NCCALCSIZE | WM_NCPAINT => return 0,
        WM_NCACTIVATE => {
            // Preserve Tao's activation bookkeeping; -1 tells DefWindowProc not to
            // paint a native caption for this state change.
            return DefSubclassProc(hwnd, message, wparam, -1);
        }
        WM_MOUSEACTIVATE if passive => return MA_NOACTIVATE as LRESULT,
        WM_DWMCOMPOSITIONCHANGED => {
            if let Err(error) = disable_native_frame(hwnd) {
                eprintln!("{error}");
            }
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(floating_proc), id);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}

fn queue_install(window: &WebviewWindow, passive: bool) -> Result<Completion, String> {
    // Resolve the handle before dispatch: no blocking runtime getter inside the UI callback.
    let handle = window.hwnd().map_err(|_| "错误：浮窗窗口不可用")?.0 as usize;
    dispatch(window, move |w| {
        let hwnd = handle as HWND;
        if passive {
            install_activation_hook()?;
        }
        unsafe {
            if SetWindowSubclass(hwnd, Some(floating_proc), SUBCLASS_ID, passive as usize) == 0 {
                return Err("错误：无法初始化浮窗窗口".into());
            }
            disable_native_frame(hwnd)?;
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            SetWindowLongPtrW(hwnd, GWL_STYLE, ((style & !FRAME) | WS_POPUP) as isize);
            let extended = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                extended_style(extended, passive) as isize,
            );
        }
        w.set_focusable(!passive)
            .map_err(|_| "错误：无法设置浮窗交互")?;
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
        Ok(())
    })
}

/// Setup/UI-thread entry point. Native failures are logged; worker initialization
/// can await `install_confirmed` before presenting its first frame.
pub fn install(window: &WebviewWindow, passive: bool) -> Result<(), String> {
    queue_install(window, passive).map(|_| ())
}

pub async fn install_confirmed(window: &WebviewWindow, passive: bool) -> Result<(), String> {
    queue_install(window, passive)?
        .await
        .map_err(|_| "错误：浮窗窗口不可用")?
}

pub fn show(window: &WebviewWindow, visible: bool) -> Result<(), String> {
    dispatch(window, move |w| {
        // These setters update Tao's VISIBLE state as well as the actual HWND.
        let result = if visible { w.show() } else { w.hide() };
        result.map_err(|_| "错误：无法更新浮窗显示".into())
    })
    .map(|_| ())
}

/// Raise any passive floating surface to the top of the topmost band.
pub fn raise_topmost(window: &WebviewWindow) -> Result<(), String> {
    let handle = window.hwnd().map_err(|_| "错误：浮窗窗口不可用")?.0 as usize;
    dispatch(window, move |_| {
        let hwnd = handle as HWND;
        unsafe {
            if SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            ) == 0
            {
                return Err("错误：无法调整浮窗层级".into());
            }
        }
        Ok(())
    })
    .map(|_| ())
}

/// Discovery takes precedence over held cards and their control dock while visible.
pub fn raise_discovery(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("overlay") else {
        return;
    };
    let Ok(handle) = window.hwnd() else {
        return;
    };
    let handle = handle.0 as usize;
    let _ = dispatch(&window, move |_| {
        let hwnd = handle as HWND;
        unsafe {
            if IsWindowVisible(hwnd) != 0
                && SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                ) == 0
            {
                return Err("错误：无法调整发现界面层级".into());
            }
        }
        Ok(())
    });
}

fn queue_place(
    window: &WebviewWindow,
    origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> Result<Completion, String> {
    let handle = window.hwnd().map_err(|_| "错误：浮窗窗口不可用")?.0 as usize;
    dispatch(window, move |_| {
        let hwnd = handle as HWND;
        unsafe {
            let mut current = RECT::default();
            if GetWindowRect(hwnd, &mut current) != 0
                && current.left == origin.x
                && current.top == origin.y
                && current.right - current.left == size.width as i32
                && current.bottom - current.top == size.height as i32
            {
                return Ok(());
            }
            if SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                origin.x,
                origin.y,
                size.width as i32,
                size.height as i32,
                SWP_NOACTIVATE | SWP_NOZORDER,
            ) == 0
            {
                return Err("错误：无法定位浮窗窗口".into());
            }
        }
        Ok(())
    })
}

pub fn place(
    window: &WebviewWindow,
    origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> Result<(), String> {
    queue_place(window, origin, size).map(|_| ())
}

pub async fn place_confirmed(
    window: &WebviewWindow,
    origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> Result<(), String> {
    queue_place(window, origin, size)?
        .await
        .map_err(|_| "错误：浮窗窗口不可用")?
}

pub fn ignore_cursor(window: &WebviewWindow, ignore: bool) -> Result<(), String> {
    dispatch(window, move |w| {
        w.set_ignore_cursor_events(ignore)
            .map_err(|_| "错误：无法设置浮窗穿透".into())
    })
    .map(|_| ())
}
