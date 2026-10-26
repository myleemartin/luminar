// 发布版不弹出 Windows 控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "windows")]
    setup_webview2_runtime();
    luminar_lib::run()
}

/// 如果安装包内带了 WebView2 固定版运行时，优先使用它。
/// 这样在未安装 WebView2 的 Windows 10/LTSC 上也能直接启动，
/// 不再依赖目标电脑是否装过 Edge WebView2。
#[cfg(target_os = "windows")]
fn setup_webview2_runtime() {
    if std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER").is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else {
        return;
    };
    for rel in ["resources/WebView2Runtime", "WebView2Runtime"] {
        let candidate = dir.join(rel);
        if candidate.join("msedgewebview2.exe").is_file() {
            prepare_fixed_runtime_permissions(&candidate);
            std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &candidate);
            return;
        }
    }
}

/// WebView2 Fixed Version ≥120 在 Windows 10 上要求运行时目录对
/// AppContainer 可读（Microsoft 文档的 icacls 步骤）。这里在首次启动时静默执行，
/// 避免用户看到白屏或自己开命令行。Windows 11 不要求，但执行也无害。
#[cfg(target_os = "windows")]
fn prepare_fixed_runtime_permissions(runtime: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let marker = runtime.join(".luminar_acl_ok");
    if marker.is_file() {
        return;
    }
    let sids = ["*S-1-15-2-2:(OI)(CI)(RX)", "*S-1-15-2-1:(OI)(CI)(RX)"];
    let mut all_ok = true;
    for sid in sids {
        let status = Command::new("icacls")
            .arg(runtime)
            .arg("/grant")
            .arg(sid)
            .arg("/T")
            .arg("/C")
            .arg("/Q")
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        if !status.map(|s| s.success()).unwrap_or(false) {
            all_ok = false;
        }
    }
    // 权限设置失败时不写 marker，下次启动会重试
    if all_ok {
        let _ = std::fs::write(marker, b"ok");
    }
}
